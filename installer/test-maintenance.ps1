# Elevated Windows-only regression tests. All fixtures live in a unique temp tree.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'maintenance.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
# Execute the production bootstrap too. Testing extracted helpers with autoload
# enabled previously concealed the missing Security module in cold child processes.
foreach ($statement in $ast.EndBlock.Statements) {
    if ($statement -is [Management.Automation.Language.TryStatementAst]) { break }
    . ([scriptblock]::Create($statement.Extent.Text))
}
foreach ($name in @('Get-Acl', 'Set-Acl')) {
    if ((Get-Command $name).ModuleName -ne 'Microsoft.PowerShell.Security') {
        throw "Missing explicit inbox Security command: $name"
    }
}
# Exercise the actual production helpers, without invoking installer actions.
$native = $ast.Find({ param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Add-Type' }, $true)
. ([scriptblock]::Create($native.Extent.Text))
foreach ($name in @('Assert-SafeItem', 'Assert-Tree', 'Protect-Item', 'Invoke-MonitorCommand', 'Invoke-ExeCommand', 'Assert-PurgeItem', 'Assert-PurgeTree', 'Remove-OwnedTree', 'Remove-OwnedRunValue')) {
    $fn = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name }, $true)
    . ([scriptblock]::Create($fn.Extent.Text))
}
$trusted = @('S-1-5-18', 'S-1-5-32-544', 'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464')
$pins = [Collections.Generic.List[IDisposable]]::new()
$UninstallerDataPath = ''
$root = Join-Path ([IO.Path]::GetTempPath()) ('SecblitzAclTest-' + [guid]::NewGuid().ToString('N'))
$junction = Join-Path $root 'junction'
function Release-Pins { foreach ($pin in $pins) { $pin.Dispose() }; $pins.Clear() }
function Expect-Rejected([scriptblock]$Body) {
    $rejected = $false
    try { & $Body } catch { $rejected = $true } finally { Release-Pins }
    if (-not $rejected) { throw 'Unsafe fixture was accepted.' }
}
try {
    $null = New-Item -ItemType Directory -Path $root
    Protect-Item $root $true
    $file = Join-Path $root 'image.exe'
    [IO.File]::WriteAllText($file, 'test fixture')
    Protect-Item $file $false
    # Verify native dispatch and exact child exit codes even with PATHEXT absent.
    $exe = Join-Path $root 'native-exit-fixture.exe'
    Add-Type -TypeDefinition 'public static class ExitFixture { public static int Main(string[] args) { return args.Length == 2 && args[0] == "service" && args[1] == "install" ? 0 : 37; } }' -OutputAssembly $exe -OutputType ConsoleApplication
    $oldPathExt = $env:PATHEXT
    try {
        Remove-Item Env:PATHEXT -ErrorAction SilentlyContinue
        Invoke-MonitorCommand 'install'
        $failed = $false
        try { Invoke-MonitorCommand 'uninstall' } catch {
            if ($_.Exception.Message -notmatch 'failed \(37\)') { throw }
            $failed = $true
        }
        if (-not $failed) { throw 'Nonzero native child exit was ignored.' }
    } finally { $env:PATHEXT = $oldPathExt }
    Protect-Item $exe $false
    Assert-Tree $root
    Release-Pins

    # A pinned safe tree cannot be swapped out during ACL validation.
    Assert-SafeItem $file
    $renameFailed = $false
    try { [IO.File]::Move($file, "$file.moved") } catch { $renameFailed = $true }
    if (-not $renameFailed) { throw 'Pinned file was renamed.' }
    Release-Pins

    # Model Inno's exclusive data-file handle. Only the exact active file gets
    # metadata-only access; all owner/DACL/link checks must still execute.
    $dataFile = Join-Path $root 'unins000.dat'
    [IO.File]::WriteAllText($dataFile, 'owned Inno fixture')
    Protect-Item $dataFile $false
    $locked = [IO.File]::Open($dataFile, 'Open', 'ReadWrite', 'None')
    try {
        Expect-Rejected { Assert-SafeItem $dataFile }
        $UninstallerDataPath = $dataFile
        Assert-SafeItem $dataFile
        Release-Pins
        $acl = Get-Acl -LiteralPath $dataFile
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-5-32-545'), 'Write', 'Allow'))
        Set-Acl -LiteralPath $dataFile -AclObject $acl
        Expect-Rejected { Assert-SafeItem $dataFile }
        Protect-Item $dataFile $false
        # A different locked file is NOT exempt just because its name looks similar.
        $other = Join-Path $root 'unins001.dat'
        [IO.File]::WriteAllText($other, 'other fixture')
        Protect-Item $other $false
        $otherLock = [IO.File]::Open($other, 'Open', 'ReadWrite', 'None')
        try { Expect-Rejected { Assert-SafeItem $other } } finally { $otherLock.Dispose() }
    } finally {
        Release-Pins
        $locked.Dispose()
        $UninstallerDataPath = ''
    }

    $link = Join-Path $root 'alias.exe'
    $null = New-Item -ItemType HardLink -Path $link -Target $file
    Expect-Rejected { Assert-SafeItem $file }
    Remove-Item -LiteralPath $link

    $null = New-Item -ItemType Junction -Path $junction -Target $root
    Expect-Rejected { Assert-SafeItem $junction }
    [IO.Directory]::Delete($junction)

    # The tray status directory lets LocalService modify status.json, nothing more.
    $statusDir = Join-Path $root 'Status'
    $statusSddl = 'O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;LS)(A;OICI;0x1200a9;;;BU)'
    $null = New-Item -ItemType Directory -Path $statusDir
    $acl = Get-Acl -LiteralPath $statusDir
    $acl.SetSecurityDescriptorSddlForm($statusSddl)
    Set-Acl -LiteralPath $statusDir -AclObject $acl
    $statusFile = Join-Path $statusDir 'status.json'
    [IO.File]::WriteAllText($statusFile, '{}')
    Assert-Tree $statusDir
    Release-Pins
    $acl = Get-Acl -LiteralPath $statusDir
    $acl.SetSecurityDescriptorSddlForm('O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;LS)(A;OICI;0x1200a9;;;BU)')
    Set-Acl -LiteralPath $statusDir -AclObject $acl
    Expect-Rejected { Assert-SafeItem $statusDir }
    Remove-Item -LiteralPath $statusDir -Recurse -Force

    foreach ($extra in @('(A;;GW;;;BU)', '(A;OIIO;GW;;;BU)', '(A;;WD;;;BU)', '(A;;WO;;;BU)', '(A;;SD;;;BU)')) {
        $acl = Get-Acl -LiteralPath $root
        $acl.SetSecurityDescriptorSddlForm('O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)' + $extra)
        Set-Acl -LiteralPath $root -AclObject $acl
        Expect-Rejected { Assert-SafeItem $root }
    }
    Protect-Item $root $true
    $acl = Get-Acl -LiteralPath $file
    $acl.SetOwner([Security.Principal.WindowsIdentity]::GetCurrent().User)
    Set-Acl -LiteralPath $file -AclObject $acl
    Expect-Rejected { Assert-SafeItem $file }
    Protect-Item $file $false
    # The web protection commands use the same native dispatch and only two fixed
    # argument strings; anything else is refused before a process starts.
    $failed = $false
    try { Invoke-ExeCommand 'filter install' } catch {
        if ($_.Exception.Message -notmatch 'failed \(37\)') { throw }
        $failed = $true
    }
    if (-not $failed) { throw 'Nonzero filter command exit was ignored.' }
    Expect-Rejected { Invoke-ExeCommand 'filter run' }
    Expect-Rejected { Invoke-ExeCommand 'filter install --other' }

    # Full cleanup: a plain protected tree (with the filter's own LocalService
    # write access) is removed completely.
    $purge = Join-Path $root 'purge'
    $dataSddl = 'O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;LS)(A;OICI;0x1200a9;;;BU)'
    function New-PurgeFixture {
        $null = New-Item -ItemType Directory -Path $purge
        Protect-Item $purge $true
        $null = New-Item -ItemType Directory -Path (Join-Path $purge 'Filter\Data') -Force
        $acl = Get-Acl -LiteralPath (Join-Path $purge 'Filter\Data')
        $acl.SetSecurityDescriptorSddlForm($dataSddl)
        Set-Acl -LiteralPath (Join-Path $purge 'Filter\Data') -AclObject $acl
        [IO.File]::WriteAllText((Join-Path $purge 'journal.json'), '{}')
        [IO.File]::WriteAllText((Join-Path $purge 'Filter\Data\status.json'), '{}')
    }
    New-PurgeFixture
    Remove-OwnedTree $purge
    if (Test-Path -LiteralPath $purge) { throw 'Owned tree was not removed.' }
    Remove-OwnedTree $purge # an absent folder is fine

    # A folder that is missing a safe property is left completely untouched.
    $outside = Join-Path $root 'outside'
    $null = New-Item -ItemType Directory -Path $outside
    Protect-Item $outside $true
    $keep = Join-Path $outside 'must-survive.txt'
    [IO.File]::WriteAllText($keep, 'keep')
    Protect-Item $keep $false
    function Expect-PurgeRefused([string]$Why, [string]$Link = '') {
        Expect-Rejected { Remove-OwnedTree $purge }
        if (-not (Test-Path -LiteralPath (Join-Path $purge 'journal.json'))) { throw "Refused purge still deleted files ($Why)." }
        if (-not (Test-Path -LiteralPath $keep)) { throw "Purge followed a link out of its folder ($Why)." }
        # Never let the cleanup itself follow the planted junction.
        if ($Link) { [IO.Directory]::Delete($Link) }
        Remove-Item -LiteralPath $purge -Recurse -Force
    }
    New-PurgeFixture
    $null = New-Item -ItemType Junction -Path (Join-Path $purge 'link') -Target $outside
    Expect-PurgeRefused 'junction' (Join-Path $purge 'link')
    New-PurgeFixture
    $null = New-Item -ItemType HardLink -Path (Join-Path $purge 'alias.txt') -Target $keep
    Expect-PurgeRefused 'hard link'
    New-PurgeFixture
    $acl = Get-Acl -LiteralPath (Join-Path $purge 'journal.json')
    $acl.SetSecurityDescriptorSddlForm('O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;GW;;;BU)')
    Set-Acl -LiteralPath (Join-Path $purge 'journal.json') -AclObject $acl
    Expect-PurgeRefused 'users can write'
    New-PurgeFixture
    $acl = Get-Acl -LiteralPath (Join-Path $purge 'Filter\Data')
    $acl.SetSecurityDescriptorSddlForm('O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;LS)(A;OICI;0x1200a9;;;BU)')
    Set-Acl -LiteralPath (Join-Path $purge 'Filter\Data') -AclObject $acl
    Expect-PurgeRefused 'service account could change permissions'
    New-PurgeFixture
    $acl = Get-Acl -LiteralPath (Join-Path $purge 'journal.json')
    $acl.SetOwner([Security.Principal.WindowsIdentity]::GetCurrent().User)
    Set-Acl -LiteralPath (Join-Path $purge 'journal.json') -AclObject $acl
    Expect-PurgeRefused 'foreign owner'

    # A logon Run value goes only when it starts our own executable.
    $runKeyPath = 'Software\SecblitzRunTest-' + [guid]::NewGuid().ToString('N')
    $runKey = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($runKeyPath)
    try {
        $runKey.SetValue('SecblitzTray', '"' + $file + '" tray')
        $runKey.SetValue('Other', '"C:\Elsewhere\secblitz.exe" tray')
        Remove-OwnedRunValue ([Microsoft.Win32.Registry]::CurrentUser) $runKeyPath 'SecblitzTray' 'C:\Elsewhere\secblitz.exe'
        if ($null -eq $runKey.GetValue('SecblitzTray', $null)) { throw 'A Run value for another program was removed.' }
        Remove-OwnedRunValue ([Microsoft.Win32.Registry]::CurrentUser) $runKeyPath 'SecblitzTray' $file
        if ($null -ne $runKey.GetValue('SecblitzTray', $null)) { throw 'Our Run value was not removed.' }
        Remove-OwnedRunValue ([Microsoft.Win32.Registry]::CurrentUser) $runKeyPath 'SecblitzTray' $file # already gone
        Remove-OwnedRunValue ([Microsoft.Win32.Registry]::CurrentUser) ($runKeyPath + '-missing') 'SecblitzTray' $file # no key
        if ($null -eq $runKey.GetValue('Other', $null)) { throw 'An unrelated Run value was removed.' }
    } finally {
        $runKey.Dispose()
        [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($runKeyPath, $false)
    }
    Write-Host 'PASS: native exit 0/37 without PATHEXT, safe RX, pinning, exact locked Inno metadata validation, hard links, junctions, write/ACL/delete grants, inherit-only writes, untrusted owner, fixed filter commands, full-cleanup tree checks (junction, hard link, writable, foreign owner) and Run value ownership.'
} finally {
    Release-Pins
    if (Test-Path -LiteralPath $junction) { [IO.Directory]::Delete($junction) }
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
