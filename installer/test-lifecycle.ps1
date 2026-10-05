# Run only on a disposable, elevated Windows runner with no existing installation.
[CmdletBinding()]
param([string]$SetupPath, [switch]$OwnershipFixtures)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($OwnershipFixtures) {
    # Exercise the actual pure XML validator on the host, without COM or Windows.
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'maintenance.ps1'), [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
    $validator = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Assert-OwnedUpdateXml' }, $true)
    . ([scriptblock]::Create($validator.Extent.Text))
    $root = 'C:\Program Files\Secblitz'
    $exe = $root + '\secblitz.exe'
    $fixture = '<Task xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task" version="1.2"><Triggers><TimeTrigger><Repetition><Interval>PT1H</Interval><StopAtDurationEnd>false</StopAtDurationEnd></Repetition><StartBoundary>2026-10-03T12:00:00</StartBoundary><Enabled>true</Enabled></TimeTrigger></Triggers><Principals><Principal id="Updater"><UserId>S-1-5-18</UserId><LogonType>ServiceAccount</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals><Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><Enabled>true</Enabled><ExecutionTimeLimit>PT1H</ExecutionTimeLimit></Settings><Actions Context="Updater"><Exec><Command>"C:\Program Files\Secblitz\secblitz.exe"</Command><Arguments>update check</Arguments><WorkingDirectory>C:\Program Files\Secblitz</WorkingDirectory></Exec></Actions></Task>'
    Assert-OwnedUpdateXml ([xml]$fixture) $exe $root
    Assert-OwnedUpdateXml ([xml]$fixture.Replace('"C:\Program Files\Secblitz\secblitz.exe"', $exe)) $exe $root
    Assert-OwnedUpdateXml ([xml]$fixture.Replace('<LogonType>ServiceAccount</LogonType>', '')) $exe $root
    $mutations = @(
        @('update check', 'update check --other'), @('secblitz.exe', 'foreign.exe'),
        @('HighestAvailable', 'LeastPrivilege'), @('ServiceAccount', 'InteractiveToken'),
        @('S-1-5-18', 'S-1-5-19'), @('<Enabled>true</Enabled>', '<Enabled>false</Enabled>'),
        @('<Interval>PT1H</Interval>', '<Interval>PT1M</Interval>'),
        @('</Triggers>', '<BootTrigger /></Triggers>'), @('<Triggers>', '<Triggers injected="true">'),
        @('</Repetition>', '<Duration>PT2H</Duration></Repetition>'),
        @('</Settings>', '<DeleteExpiredTaskAfter>PT1H</DeleteExpiredTaskAfter></Settings>'),
        @('IgnoreNew', 'Parallel'), @('<ExecutionTimeLimit>PT1H</ExecutionTimeLimit>', ''),
        @('</Exec>', '<Other /></Exec>'), @('</Actions>', '<Exec /></Actions>'),
        @('<Arguments>update check</Arguments>', '<Arguments> update check</Arguments>'),
        @('<WorkingDirectory>C:\Program Files\Secblitz</WorkingDirectory>', '<WorkingDirectory>C:\Windows</WorkingDirectory>'),
        @('</Principals>', '<Principal id="Other" /></Principals>'),
        @('<TimeTrigger>', '<TimeTrigger injected="true">'),
        @('</TimeTrigger>', '<EndBoundary>2026-10-04T12:00:00</EndBoundary></TimeTrigger>'),
        @('<StartBoundary>2026-10-03T12:00:00</StartBoundary>', '<StartBoundary>invalid</StartBoundary>')
    )
    foreach ($mutation in $mutations) {
        $rejected = $false
        try { Assert-OwnedUpdateXml ([xml]$fixture.Replace($mutation[0], $mutation[1])) $exe $root }
        catch { $rejected = $true }
        if (-not $rejected) { throw "Ownership mutation accepted: $($mutation[0])" }
    }
    Write-Host "PASS: 3 accepted ownership fixtures, $($mutations.Count) rejected mutations."
    # The web protection reconcile task: SYSTEM running our executable with the
    # fixed arguments is ours; the triggers only decide when it runs.
    $reconcileValidator = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Assert-OwnedReconcileXml' }, $true)
    . ([scriptblock]::Create($reconcileValidator.Extent.Text))
    $reconcile = '<Task xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task" version="1.2"><Triggers><BootTrigger><Enabled>true</Enabled></BootTrigger><TimeTrigger><Repetition><Interval>PT1H</Interval></Repetition><StartBoundary>2026-10-03T12:00:00</StartBoundary></TimeTrigger><EventTrigger><Subscription>&lt;QueryList /&gt;</Subscription></EventTrigger></Triggers><Principals><Principal id="Reconcile"><UserId>S-1-5-18</UserId><LogonType>ServiceAccount</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals><Actions Context="Reconcile"><Exec><Command>"C:\Program Files\Secblitz\secblitz.exe"</Command><Arguments>filter reconcile</Arguments><WorkingDirectory>C:\Program Files\Secblitz</WorkingDirectory></Exec></Actions></Task>'
    Assert-OwnedReconcileXml ([xml]$reconcile) $exe $root
    Assert-OwnedReconcileXml ([xml]$reconcile.Replace('"C:\Program Files\Secblitz\secblitz.exe"', $exe)) $exe $root
    Assert-OwnedReconcileXml ([xml]$reconcile.Replace('<LogonType>ServiceAccount</LogonType>', '')) $exe $root
    $reconcileMutations = @(
        @('filter reconcile', 'filter run'), @('filter reconcile', 'filter reconcile --other'),
        @('secblitz.exe', 'foreign.exe'), @('HighestAvailable', 'LeastPrivilege'),
        @('ServiceAccount', 'InteractiveToken'), @('S-1-5-18', 'S-1-5-19'),
        @('</Exec>', '<Other /></Exec>'), @('</Actions>', '<Exec /></Actions>'),
        @('<Arguments>filter reconcile</Arguments>', '<Arguments> filter reconcile</Arguments>'),
        @('<WorkingDirectory>C:\Program Files\Secblitz</WorkingDirectory>', '<WorkingDirectory>C:\Windows</WorkingDirectory>'),
        @('</Principals>', '<Principal id="Other" /></Principals>'),
        @('<Exec>', '<ComHandler><ClassId>{00000000-0000-0000-0000-000000000000}</ClassId></ComHandler><Exec>')
    )
    foreach ($mutation in $reconcileMutations) {
        $rejected = $false
        try { Assert-OwnedReconcileXml ([xml]$reconcile.Replace($mutation[0], $mutation[1])) $exe $root }
        catch { $rejected = $true }
        if (-not $rejected) { throw "Reconcile ownership mutation accepted: $($mutation[0])" }
    }
    Write-Host "PASS: 3 accepted reconcile fixtures, $($reconcileMutations.Count) rejected mutations."
    exit 0
}
if (-not $SetupPath) { throw 'SetupPath is required for native lifecycle testing.' }
$app = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Secblitz'
$data = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'Secblitz'
$desktopIcon = Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'Secblitz.lnk'
Import-Module (Join-Path ([Environment]::SystemDirectory) 'WindowsPowerShell\v1.0\Modules\ScheduledTasks\ScheduledTasks.psd1')
if (Get-ScheduledTask -TaskName SecblitzUpdate -TaskPath '\' -ErrorAction SilentlyContinue) {
    throw 'Lifecycle test requires no existing SecblitzUpdate task.'
}
if ($null -ne [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\Software\Secblitz', 'AutoUpdatesEnabled', $null)) {
    throw 'Lifecycle test requires no existing updater preference.'
}
if ((Test-Path -LiteralPath $desktopIcon) -or (Get-Process secblitz -ErrorAction SilentlyContinue)) {
    throw 'Lifecycle test requires no existing Secblitz desktop shortcut or app process.'
}
if ((Test-Path -LiteralPath $app) -or (Test-Path -LiteralPath $data) -or (Get-Service SecblitzMonitor -ErrorAction SilentlyContinue)) {
    throw 'Lifecycle test requires a disposable runner with no Secblitz installation/data.'
}
$logs = Join-Path ([IO.Path]::GetTempPath()) ('SecblitzLifecycle-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $logs
$journal = Join-Path $data 'journal-sentinel.json'
$journalHash = ''
function New-Journal {
    # Stands in for the journals and app copies: a protected data folder, like the
    # real one, that setup and upgrades must keep and removing Secblitz deletes.
    $null = New-Item -ItemType Directory -Path $data
    $acl = Get-Acl -LiteralPath $data
    $acl.SetSecurityDescriptorSddlForm('O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)')
    Set-Acl -LiteralPath $data -AclObject $acl
    [IO.File]::WriteAllText($journal, '{"installer-test":"must survive upgrade, not removal"}')
    $script:journalHash = (Get-FileHash -LiteralPath $journal).Hash
}
New-Journal
function Run-Setup([string]$Exe, [string[]]$Arguments, [int]$ExpectedExit = 0, [int]$TimeoutMs = 240000) {
    $p = Start-Process -FilePath $Exe -ArgumentList $Arguments -PassThru
    try {
        $null = $p.Handle
        if (-not $p.WaitForExit($TimeoutMs)) { throw 'Installer test timed out; inspect the disposable runner.' }
        "exit=$($p.ExitCode) exe=$Exe arguments=$($Arguments -join ' ')" | Add-Content (Join-Path $logs 'status.txt')
        if ($p.ExitCode -ne $ExpectedExit) { throw "Installer exited $($p.ExitCode), expected $ExpectedExit; logs: $logs" }
    } finally { $p.Dispose() }
}
function Assert-Journal {
    if ((Get-FileHash -LiteralPath $journal).Hash -ne $journalHash) { throw 'Journal sentinel changed.' }
}
function Assert-Removed {
    # Removing Secblitz leaves none of its background parts, data or settings.
    if (Get-Service SecblitzMonitor -ErrorAction SilentlyContinue) { throw 'Monitor service survived removal.' }
    if (Get-Service SecblitzFilter -ErrorAction SilentlyContinue) { throw 'Web protection service survived removal.' }
    if (Get-ScheduledTask -TaskName SecblitzFilterReconcile -TaskPath '\' -ErrorAction SilentlyContinue) { throw 'Reconcile task survived removal.' }
    if (Get-ScheduledTask -TaskName SecblitzUpdate -TaskPath '\' -ErrorAction SilentlyContinue) { throw 'Update task survived removal.' }
    if (Test-Path -LiteralPath $data) { throw 'The data folder survived removal.' }
    if (Test-Path -LiteralPath (Join-Path $app 'Monitor')) { throw 'The monitor folder survived removal.' }
    if ($null -ne [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\Software\Secblitz', 'AutoUpdatesEnabled', $null)) { throw 'The settings key survived removal.' }
    if ($null -ne [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\Software\Microsoft\Windows\CurrentVersion\Run', 'SecblitzTray', $null)) { throw 'The tray Run value survived removal.' }
    if (Test-Path -LiteralPath (Join-Path $app 'secblitz.exe')) { throw 'The application binary survived removal.' }
}
function Assert-LogHas([string]$Name, [string]$Text) {
    if (-not (Select-String -LiteralPath (Join-Path $logs $Name) -SimpleMatch $Text -Quiet)) { throw "$Name does not say: $Text" }
}
function Assert-NoPutBack([string]$Name) {
    # A silent removal keeps the changes: it must never run the put-back.
    if (Select-String -LiteralPath (Join-Path $logs $Name) -SimpleMatch 'Secblitz put back' -Quiet) { throw "$Name ran the put-back." }
}
function Assert-Filter {
    # Registered turned off, as LocalService; the reconcile task runs as SYSTEM.
    $service = Get-CimInstance Win32_Service -Filter "Name='SecblitzFilter'"
    if ($null -eq $service) { throw 'Web protection service was not registered.' }
    if ($service.StartName -ine 'NT AUTHORITY\LocalService' -or $service.ServiceType -ne 'Own Process' -or
        $service.PathName -cne ('"' + (Join-Path $app 'secblitz.exe') + '" filter run')) { throw 'Wrong web protection service account or image path.' }
    $task = Get-ScheduledTask -TaskName SecblitzFilterReconcile -TaskPath '\' -ErrorAction SilentlyContinue
    if ($null -eq $task -or @($task.Actions).Count -ne 1 -or
        $task.Actions[0].Execute -cne ('"' + (Join-Path $app 'secblitz.exe') + '"') -or
        $task.Actions[0].Arguments -cne 'filter reconcile' -or $task.Actions[0].WorkingDirectory -cne $app -or
        $task.Principal.UserId -notin @('SYSTEM', 'S-1-5-18') -or $task.Principal.RunLevel -ne 'Highest' -or
        @($task.Triggers).Count -ne 3) { throw 'Unexpected web protection reconcile task.' }
}
function Assert-NoAppLaunch {
    # Source checks also enforce skipifsilent; this runtime check catches a surviving UI process.
    $interactive = Get-CimInstance Win32_Process -Filter "Name='secblitz.exe'" |
        Where-Object { $_.CommandLine -cne ('"' + (Join-Path $app 'secblitz.exe') + '" service run') }
    if ($interactive) { throw 'Silent setup launched an unexpected application process.' }
}
function Assert-Updates([bool]$Enabled) {
    $task = Get-ScheduledTask -TaskName SecblitzUpdate -TaskPath '\' -ErrorAction SilentlyContinue
    $saved = [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\Software\Secblitz', 'AutoUpdatesEnabled', $null)
    if ($saved -ne [int]$Enabled) { throw 'Updater preference was not persisted.' }
    if (-not $Enabled) {
        if ($null -ne $task) { throw 'Opted-out updater task exists.' }
        return
    }
    if ($null -eq $task -or @($task.Actions).Count -ne 1 -or
        $task.Actions[0].Execute -cne ('"' + (Join-Path $app 'secblitz.exe') + '"') -or
        $task.Actions[0].Arguments -cne 'update check' -or $task.Actions[0].WorkingDirectory -cne $app -or
        $task.Principal.UserId -notin @('SYSTEM', 'S-1-5-18') -or $task.Principal.RunLevel -ne 'Highest' -or
        @($task.Triggers).Count -ne 1 -or $task.Triggers[0].Repetition.Interval -cne 'PT1H') { throw 'Unexpected updater task definition.' }
    $info = Get-ScheduledTaskInfo -TaskName SecblitzUpdate -TaskPath '\'
    if ($info.NextRunTime -lt (Get-Date).AddMinutes(45) -or $info.NextRunTime -gt (Get-Date).AddMinutes(61)) {
        throw 'Updater must first run approximately one hour after setup, never during installation.'
    }
    $scheduler = New-Object -ComObject Schedule.Service
    $scheduler.Connect()
    $sd = [Security.AccessControl.RawSecurityDescriptor]::new($scheduler.GetFolder('\').GetTask('SecblitzUpdate').GetSecurityDescriptor(7))
    $rights = @{}
    foreach ($ace in $sd.DiscretionaryAcl) {
        if ($ace.AceQualifier -ne 'AccessAllowed') { throw 'Unexpected updater ACL.' }
        $rights[$ace.SecurityIdentifier.Value] = $ace.AccessMask
        if ($ace.SecurityIdentifier.Value -notin @('S-1-5-18', 'S-1-5-32-544') -and
            ($ace.AccessMask -band (-bnot 0x1200a9)) -ne 0) { throw 'Users can modify updater task.' }
    }
    if (($rights['S-1-5-18'] -band 0x1f01ff) -ne 0x1f01ff -or
        ($rights['S-1-5-32-544'] -band 0x1f01ff) -ne 0x1f01ff -or
        ($rights['S-1-5-32-545'] -band 0x1200a9) -ne 0x1200a9) { throw 'Missing required task ACL rights.' }
    $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($scheduler)
}
# No /TASKS override: desktop and hourly updater selected, monitor unchecked.
Run-Setup $SetupPath @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', "/LOG=`"$logs\default-install.log`"")
if (-not (Test-Path -LiteralPath $desktopIcon)) { throw 'Default install omitted the desktop shortcut.' }
$shell = New-Object -ComObject WScript.Shell
try {
    $shortcut = $shell.CreateShortcut($desktopIcon)
    try {
        if ($shortcut.TargetPath -ine (Join-Path $app 'secblitz.exe') -or $shortcut.Arguments) {
            throw 'Desktop shortcut must target only the installed app, without arguments.'
        }
    } finally { $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcut) }
} finally { $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell) }
if (Get-Service SecblitzMonitor -ErrorAction SilentlyContinue) { throw 'Default install registered the optional monitor.' }
Assert-NoAppLaunch
Assert-Journal
Assert-Updates $true
Assert-Filter
$filterService = Get-CimInstance Win32_Service -Filter "Name='SecblitzFilter'"
if ($filterService.StartMode -ne 'Disabled' -or $filterService.State -ne 'Stopped') { throw 'Web protection must be registered turned off.' }
if ($null -eq [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\Software\Microsoft\Windows\CurrentVersion\Run', 'SecblitzTray', $null)) { throw 'The default install omitted the tray Run value.' }
# An upgrade stops a running web protection service while files are replaced and
# starts it again afterwards. (Started by hand: no switch is on, so it just idles.)
& sc.exe config SecblitzFilter start= demand | Out-Null
if ($LASTEXITCODE) { throw 'Cannot prepare the web protection upgrade fixture.' }
$filterController = Get-Service SecblitzFilter
try {
    $filterController.Start()
    $filterController.WaitForStatus([ServiceProcess.ServiceControllerStatus]::Running, [TimeSpan]::FromSeconds(30))
    Run-Setup $SetupPath @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', '/TASKS=""', '/SECBLITZUPDATE=1', "/LOG=`"$logs\filter-upgrade.log`"")
    $filterController.Refresh()
    if ($filterController.Status -ne 'Running') { throw 'Upgrade did not start web protection again.' }
    $filterController.Stop()
    $filterController.WaitForStatus([ServiceProcess.ServiceControllerStatus]::Stopped, [TimeSpan]::FromSeconds(30))
} finally { $filterController.Dispose() }
Assert-Filter
# The verified worker's empty task list must retain the updater preference.
Run-Setup $SetupPath @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', '/TASKS=""', '/SECBLITZUPDATE=1', "/LOG=`"$logs\automatic-upgrade.log`"")
Assert-Updates $true
Assert-NoAppLaunch
Run-Setup $SetupPath @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', '/TASKS=""', "/LOG=`"$logs\legacy-automatic-upgrade.log`"")
Assert-Updates $true
Assert-NoAppLaunch
# Same-name tasks with different XML or ACLs are foreign. Neither setup nor
# uninstall may alter them. Hourly triggers remain in the future during tests.
$scheduler = New-Object -ComObject Schedule.Service
$scheduler.Connect()
$folder = $scheduler.GetFolder('\')
$ownedXml = $folder.GetTask('SecblitzUpdate').Xml
$ownedSddl = $folder.GetTask('SecblitzUpdate').GetSecurityDescriptor(7)
foreach ($variant in @('command', 'arguments', 'directory', 'principal', 'extra-action', 'writable-acl', 'disabled', 'interval', 'extra-trigger', 'settings')) {
    $foreign = $scheduler.NewTask(0)
    $foreign.XmlText = $ownedXml
    # Retain the hourly trigger so each fixture isolates its changed field.
    switch ($variant) {
        command { $foreign.Actions.Item(1).Path = '"' + (Join-Path $app 'foreign.exe') + '"' }
        arguments { $foreign.Actions.Item(1).Arguments = 'guide' }
        directory { $foreign.Actions.Item(1).WorkingDirectory = [Environment]::SystemDirectory }
        principal { $foreign.Principal.UserId = 'S-1-5-19' }
        extra-action { $extra = $foreign.Actions.Create(0); $extra.Path = '"' + (Join-Path $app 'secblitz.exe') + '"'; $extra.Arguments = 'guide' }
        disabled { $foreign.Settings.Enabled = $false }
        interval { $foreign.Triggers.Item(1).Repetition.Interval = 'PT2H' }
        extra-trigger { $extraTrigger = $foreign.Triggers.Create(1); $extraTrigger.StartBoundary = [DateTime]::Now.AddDays(1).ToString('yyyy-MM-ddTHH:mm:ss') }
        settings { $foreign.Settings.ExecutionTimeLimit = 'PT2H' }
    }
    $fixtureSddl = $ownedSddl
    if ($variant -eq 'writable-acl') { $fixtureSddl = 'O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;BU)' }
    $null = $folder.RegisterTaskDefinition('SecblitzUpdate', $foreign, 20, $foreign.Principal.UserId, $null, 5, $fixtureSddl)
    $before = $folder.GetTask('SecblitzUpdate').Xml
    $beforeAcl = $folder.GetTask('SecblitzUpdate').GetSecurityDescriptor(7)
    Run-Setup $SetupPath @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', "/LOG=`"$logs\foreign-$variant-install.log`"") 7
    Run-Setup (Join-Path $app 'unins000.exe') @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=`"$logs\foreign-$variant-uninstall.log`"") 1
    if ($folder.GetTask('SecblitzUpdate').Xml -cne $before) { throw 'Foreign task was modified.' }
    if ($folder.GetTask('SecblitzUpdate').GetSecurityDescriptor(7) -cne $beforeAcl) { throw 'Foreign task ACL was modified.' }
    $null = $folder.RegisterTask('SecblitzUpdate', $ownedXml, 20, 'S-1-5-18', $null, 5, $ownedSddl)
}
$null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($folder)
$null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($scheduler)
# A silent removal without /SECBLITZDONE keeps the changes: no prompt, no put-back,
# yet every background part, the data and the settings go.
Run-Setup (Join-Path $app 'unins000.exe') @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=`"$logs\default-uninstall.log`"")
Assert-LogHas 'default-uninstall.log' 'a silent uninstall keeps the changes'
Assert-NoPutBack 'default-uninstall.log'
Assert-Removed
if (Test-Path -LiteralPath $desktopIcon) { throw 'Uninstall retained the owned desktop shortcut.' }
New-Journal
# Start-Process joins arguments into a Windows command line: keep literal double
# quotes for the empty list. PowerShell single quotes alone are not passed onward.
Run-Setup $SetupPath @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', '/TASKS=""', "/LOG=`"$logs\no-tasks-install.log`"")
if (Test-Path -LiteralPath $desktopIcon) { throw 'Empty task list did not clear the default desktop task.' }
if (Get-Service SecblitzMonitor -ErrorAction SilentlyContinue) { throw 'Empty task list installed the monitor.' }
Assert-Updates $false
Run-Setup $SetupPath @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', '/TASKS=""', '/SECBLITZUPDATE=1', "/LOG=`"$logs\opted-out-automatic-upgrade.log`"")
Assert-Updates $false
# No task override must retain the saved updater opt-out.
Run-Setup $SetupPath @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', "/LOG=`"$logs\opted-out-default-upgrade.log`"")
Assert-Updates $false
Assert-NoAppLaunch
Assert-Journal
if (Test-Path -LiteralPath $desktopIcon) { throw 'Default upgrade lost the saved desktop opt-out.' }
# /SECBLITZDONE: the app already answered the question; nothing is asked or put back.
Run-Setup (Join-Path $app 'unins000.exe') @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SECBLITZDONE', "/LOG=`"$logs\no-tasks-uninstall.log`"")
Assert-LogHas 'no-tasks-uninstall.log' 'already answered the question'
Assert-NoPutBack 'no-tasks-uninstall.log'
Assert-Removed
New-Journal
# Explicit task list opts out of desktopicon while opting into the monitor.
Run-Setup $SetupPath @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', '/TASKS=monitor', "/LOG=`"$logs\install.log`"")
if (Test-Path -LiteralPath $desktopIcon) { throw 'Desktop opt-out was ignored.' }
Assert-NoAppLaunch
$unrelated = Join-Path $app 'unrelated-preservation.txt'
[IO.File]::WriteAllText($unrelated, 'Untracked application data must survive.')
$unrelatedHash = (Get-FileHash -LiteralPath $unrelated).Hash
$native = Get-CimInstance Win32_Service -Filter "Name='SecblitzMonitor'"
if ($native.StartName -ine 'NT AUTHORITY\LocalService' -or $native.PathName -cne ('"' + (Join-Path $app 'secblitz.exe') + '" service run')) {
    throw 'Wrong service account or image path.'
}
$report = Join-Path $app 'Monitor\latest.json'
$service = Get-Service SecblitzMonitor
try {
    if ($service.Status -ne 'Stopped') { throw 'Fresh monitor must not start automatically.' }
    $service.Start()
    $service.WaitForStatus([ServiceProcess.ServiceControllerStatus]::Running, [TimeSpan]::FromSeconds(30))
    $deadline = [DateTime]::UtcNow.AddSeconds(420)
    $scanned = $false
    while (-not $scanned -and [DateTime]::UtcNow -lt $deadline) {
        try {
            $stream = [IO.File]::Open($report, 'Open', 'Read', 'ReadWrite')
            $reader = [IO.StreamReader]::new($stream)
            try { $snapshot = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
            $scanned = $snapshot.schema -eq 1
        } catch { $scanned = $false }
        if (-not $scanned) { Start-Sleep -Milliseconds 500 }
    }
    if (-not $scanned) { throw 'Monitor did not produce a schema-1 report.' }
    $snapshot | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $logs 'monitor-report.json')
    # Task is deliberately unchecked: an existing monitor must survive upgrade.
    Run-Setup $SetupPath @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', '/TASKS=""', "/LOG=`"$logs\upgrade.log`"")
    Assert-NoAppLaunch
    if (Test-Path -LiteralPath $desktopIcon) { throw 'Upgrade ignored desktop opt-out.' }
    $service.Refresh()
    if ($service.Status -ne 'Running') { throw 'Upgrade did not resume the previously running monitor.' }
    Assert-Journal
    if (-not (Test-Path -LiteralPath $report)) { throw 'Upgrade removed the monitor report.' }
} finally { $service.Dispose() }
# Dispose SCM handles before uninstall; held handles delay DeleteService completion.
$lookalike = Join-Path $app 'unins001.dat'
if (Test-Path -LiteralPath $lookalike) { throw 'Unexpected existing lookalike fixture.' }
[IO.File]::WriteAllText($lookalike, 'Unrelated Inno-lookalike data must remain validated and preserved.')
$lookalikeHash = (Get-FileHash -LiteralPath $lookalike).Hash
$locked = [IO.File]::Open($lookalike, 'Open', 'ReadWrite', 'None')
try {
    # /VERYSILENT alone: no modal error and no broad unins*.dat bypass.
    Run-Setup (Join-Path $app 'unins000.exe') @('/VERYSILENT', '/NORESTART', "/LOG=`"$logs\uninstall-rejected.log`"") 1 30000
    if (-not (Test-Path -LiteralPath (Join-Path $app 'secblitz.exe'))) { throw 'Rejected uninstall removed the app.' }
    $service = Get-Service SecblitzMonitor
    try {
        if ($service.Status -ne 'Running') { throw 'Rejected uninstall changed the running service.' }
    } finally { $service.Dispose() }
    # A refused removal touches nothing else either.
    Assert-Filter
    Assert-Journal
} finally { $locked.Dispose() }
Run-Setup (Join-Path $app 'unins000.exe') @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=`"$logs\uninstall.log`"")
Assert-NoPutBack 'uninstall.log'
Assert-Removed
if (Test-Path -LiteralPath $desktopIcon) { throw 'Uninstall retained the owned desktop shortcut.' }
if ((Get-FileHash -LiteralPath $unrelated).Hash -ne $unrelatedHash) { throw 'Uninstall changed unrelated application data.' }
if ((Get-FileHash -LiteralPath $lookalike).Hash -ne $lookalikeHash) { throw 'Uninstall changed unrelated lookalike data.' }
Write-Host "PASS: hourly updater/ACL/delayed start, preserved update choice, foreign-task rejection, default desktop shortcut/removal, desktop opt-out, silent no-UI checks, fresh monitor opt-in, running upgrade/resume, web protection registration/upgrade resume, quiet uninstall rejection/retry, silent and /SECBLITZDONE removal keeping the changes, full cleanup of services, tasks, data, settings and Run value, unrelated-file preservation. Logs: $logs"
