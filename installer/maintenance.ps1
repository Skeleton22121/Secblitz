# Invoked only by elevated Setup/Uninstall. Never invokes apply or a bare exe.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Validate', 'Prepare', 'Secure', 'InstallMonitor', 'RemoveMonitor', 'ResumeMonitor', 'InstallFilter', 'ResumeFilter', 'RemoveFilter', 'Purge', 'EnableUpdates', 'DisableUpdates', 'PreserveUpdates')][string]$Action,
    [string]$UninstallerDataPath = ''
)
# Only inbox modules; inherited user module search paths must never run elevated.
$env:PSModulePath = [IO.Path]::Combine([Environment]::SystemDirectory, 'WindowsPowerShell\v1.0\Modules')
$PSModuleAutoLoadingPreference = 'None'
Import-Module ([IO.Path]::Combine($env:PSModulePath, 'Microsoft.PowerShell.Management\Microsoft.PowerShell.Management.psd1')) -ErrorAction Stop
Import-Module ([IO.Path]::Combine($env:PSModulePath, 'Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1')) -ErrorAction Stop
Import-Module ([IO.Path]::Combine($env:PSModulePath, 'Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1')) -ErrorAction Stop
Import-Module ([IO.Path]::Combine($env:PSModulePath, 'CimCmdlets\CimCmdlets.psd1')) -ErrorAction Stop
Import-Module ([IO.Path]::Combine($env:PSModulePath, 'ScheduledTasks\ScheduledTasks.psd1')) -ErrorAction Stop
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

try {
    if (-not [Environment]::Is64BitProcess) { throw '64-bit PowerShell is required.' }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try {
        if (-not ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
            throw 'Administrator elevation is required.'
        }
    } finally { $identity.Dispose() }
    $programFiles = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
    $root = Join-Path $programFiles 'Secblitz'
    $exe = Join-Path $root 'secblitz.exe'
    if ($UninstallerDataPath) {
        # Only Inno's active, direct-child data file can use metadata-only access.
        # The app directory is pinned and validated first; its ACL prevents an
        # unprivileged caller replacing this file while Inno holds it exclusively.
        if ($Action -notin @('RemoveMonitor', 'RemoveFilter', 'Purge') -or
            [IO.Path]::GetDirectoryName($UninstallerDataPath) -ine $root -or
            [IO.Path]::GetFileName($UninstallerDataPath) -notmatch '^unins[0-9]{3}\.dat$') {
            throw 'Unexpected active uninstaller data path.'
        }
    }
    $trusted = @('S-1-5-18', 'S-1-5-32-544', 'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464')
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class SecblitzPaths {
    [StructLayout(LayoutKind.Sequential)] struct Info {
        public uint Attributes;
        public System.Runtime.InteropServices.ComTypes.FILETIME Creation, Access, Write;
        public uint Volume, SizeHigh, SizeLow, Links, IndexHigh, IndexLow;
    }
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern SafeFileHandle CreateFile(string path, uint access, uint share, IntPtr sa, uint mode, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool GetFileInformationByHandle(SafeFileHandle handle, out Info info);
    public static SafeFileHandle Pin(string path) {
        return Open(path, 0x20081);
    }
    public static SafeFileHandle Metadata(string path) {
        return Open(path, 0x20080);
    }
    static SafeFileHandle Open(string path, uint access) {
        // Include read-data/list-directory: metadata-only handles do NOT enforce
        // share-delete restrictions. Share write for the live monitor report.
        var h = CreateFile(path, access, 3, IntPtr.Zero, 3, 0x02200000, IntPtr.Zero);
        if (h.IsInvalid) { h.Dispose(); throw new Win32Exception(); }
        Info i;
        if (!GetFileInformationByHandle(h, out i)) { h.Dispose(); throw new Win32Exception(); }
        if ((i.Attributes & 0x400) != 0 || ((i.Attributes & 0x10) == 0 && i.Links != 1)) {
            h.Dispose(); throw new InvalidOperationException("Reparse point or hard link refused: " + path);
        }
        return h;
    }
}
'@
    $pins = [Collections.Generic.List[IDisposable]]::new()

    function Assert-SafeItem([string]$Path, [bool]$Ancestor = $false, [bool]$ProgramDataDir = $false) {
        if ($UninstallerDataPath -and $Path -ieq $UninstallerDataPath) {
            # Do NOT skip metadata, link-count, owner or DACL validation.
            $pins.Add([SecblitzPaths]::Metadata($Path))
        } else {
            $pins.Add([SecblitzPaths]::Pin($Path))
        }
        $item = Get-Item -LiteralPath $Path -Force
        if ($UninstallerDataPath -and $Path -ieq $UninstallerDataPath -and $item.PSIsContainer) { throw 'Uninstaller data is not a regular file.' }
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point refused: $Path" }
        $acl = Get-Acl -LiteralPath $Path
        $statusDirOwner = Join-Path $root 'Status'
        $inStatus = ($Path -ieq $statusDirOwner -or $Path.StartsWith($statusDirOwner + '\', [StringComparison]::OrdinalIgnoreCase))
        $ownerSid = $acl.GetOwner([Security.Principal.SecurityIdentifier]).Value
        # Files the monitor (LocalService) creates in the Status directory are owned by S-1-5-19.
        if ($ownerSid -notin $trusted -and -not ($inStatus -and $ownerSid -eq 'S-1-5-19')) {
            throw "Untrusted owner: $Path"
        }
        foreach ($rule in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
            if ($rule.AccessControlType -ne 'Allow') { continue }
            if ($rule.IdentityReference.Value -in $trusted) { continue }
            if ($Ancestor -and ($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly)) { continue }
            $allowed = 0x1200a9 # Read/execute, never write/delete/change owner or ACL.
            if ($Ancestor) { $allowed = $allowed -bor 6 } # Windows root/Program Files create-child ACEs.
            # Stock C:\ProgramData grants Users (CI)(WD,AD,WEA,WA): create-child plus EA/attribute writes only.
            if ($Ancestor -and $ProgramDataDir) { $allowed = $allowed -bor 0x110 }
            if ($Path -eq (Join-Path $root 'Monitor\latest.json') -and $rule.IdentityReference.Value -eq 'S-1-5-19') {
                $allowed = 0x12019f # Existing service-owned report only.
            }
            # The tray status directory: the monitor (LocalService) writes status.json there.
            $statusDir = Join-Path $root 'Status'
            if (($Path -ieq $statusDir -or $Path.StartsWith($statusDir + '\', [StringComparison]::OrdinalIgnoreCase)) -and $rule.IdentityReference.Value -eq 'S-1-5-19') {
                $allowed = 0x1301bf # Status files only: no delete-child, no ACL or owner change.
            }
            if (([int]$rule.FileSystemRights -band (-bnot $allowed)) -ne 0) { throw "Writable object refused: $Path" }
        }
        # Empty/null DACL distinctions must fail closed; require administrator access.
        $sddl = $acl.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access)
        if ($sddl -match 'NO_ACCESS_CONTROL' -or $sddl -notmatch '\(A;[^)]*;;;(BA|SY)\)') { throw "Missing privileged ACL: $Path" }
    }

    function Assert-Tree([string]$Path) {
        Assert-SafeItem $Path
        if ((Get-Item -LiteralPath $Path -Force).PSIsContainer) {
            foreach ($child in Get-ChildItem -LiteralPath $Path -Force) {
                Assert-Tree $child.FullName
            }
        }
    }

    function Protect-Item([string]$Path, [bool]$Directory) {
        $inherit = if ($Directory) { 'OICI' } else { '' }
        $sddl = "O:BAG:BAD:P(A;${inherit};FA;;;SY)(A;${inherit};FA;;;BA)(A;${inherit};0x1200a9;;;LS)(A;${inherit};0x1200a9;;;BU)"
        $acl = Get-Acl -LiteralPath $Path
        $acl.SetSecurityDescriptorSddlForm($sddl)
        Set-Acl -LiteralPath $Path -AclObject $acl
    }

    function Get-OwnedMonitor {
        $service = Get-CimInstance Win32_Service -Filter "Name='SecblitzMonitor'"
        if ($null -ne $service) {
            if ($service.PathName -cne ('"' + $exe + '" service run') -or
                $service.StartName -ine 'NT AUTHORITY\LocalService' -or
                $service.ServiceType -ne 'Own Process') {
                throw 'SecblitzMonitor is not the expected owned service; refusing to change it.'
            }
        }
        return $service
    }

    # COM registration supplies the task DACL atomically (ScheduledTasks does not
    # expose that parameter). No shell, task command line, or module-path lookup.
    $scheduler = New-Object -ComObject Schedule.Service
    $scheduler.Connect()
    $taskFolder = $scheduler.GetFolder('\')
    $taskName = 'SecblitzUpdate'
    $taskSddl = 'O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFX;;;BU)'
    $preferenceKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('Software\Secblitz')
    if ($null -ne $preferenceKey) {
        try {
            $acl = $preferenceKey.GetAccessControl()
            if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin $trusted -or
                $acl.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access) -match 'NO_ACCESS_CONTROL') {
                throw 'Untrusted update preference registry key.'
            }
            foreach ($rule in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
                if ($rule.AccessControlType -eq 'Allow' -and $rule.IdentityReference.Value -notin $trusted -and
                    ([int]$rule.RegistryRights -band (-bnot 0x20019)) -ne 0) { throw 'Writable update preference registry key.' }
            }
        } finally { $preferenceKey.Dispose() }
    }
    function Assert-OwnedUpdateXml([xml]$xml, [string]$ExePath, [string]$AppRoot) {
        $ns = [Xml.XmlNamespaceManager]::new($xml.NameTable)
        $ns.AddNamespace('t', 'http://schemas.microsoft.com/windows/2004/02/mit/task')
        if ($xml.DocumentElement.LocalName -cne 'Task' -or
            $xml.DocumentElement.NamespaceURI -cne $ns.LookupNamespace('t')) { throw 'Unexpected task XML root.' }
        function Assert-Children($Node, [string[]]$Names) {
            if ($null -eq $Node) { throw 'Missing task XML element.' }
            $seen = @{}
            foreach ($child in $Node.ChildNodes) {
                if ($child.NodeType -ne 'Element' -or $child.NamespaceURI -cne $ns.LookupNamespace('t') -or
                    $child.LocalName -cnotin $Names -or $seen.ContainsKey($child.LocalName)) { throw 'Unexpected task XML element.' }
                $seen[$child.LocalName] = $true
            }
        }
        function Assert-Value($Node, [string]$Value) {
            if ($null -eq $Node -or $Node.Attributes.Count -ne 0 -or
                $Node.InnerText -cne $Value -or @($Node.SelectNodes('*')).Count -ne 0) { throw 'Unexpected task XML value.' }
        }
        Assert-Children $xml.DocumentElement @('RegistrationInfo', 'Triggers', 'Principals', 'Settings', 'Actions')
        $actions = $xml.SelectSingleNode('/t:Task/t:Actions', $ns)
        $principals = $xml.SelectSingleNode('/t:Task/t:Principals', $ns)
        # Accept only the scheduler's optional outer quotes, never trim arguments,
        # expand environment variables, resolve another path, or accept a prefix.
        $commandNode = $xml.SelectSingleNode('/t:Task/t:Actions/t:Exec/t:Command', $ns)
        if ($null -ne $commandNode -and $commandNode.InnerText -ceq $ExePath) {
            $commandNode.InnerText = '"' + $ExePath + '"'
        }
        $command = [Security.SecurityElement]::Escape('"' + $ExePath + '"')
        $directory = [Security.SecurityElement]::Escape($AppRoot)
        $expected = [xml]('<Task xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task"><Principals><Principal id="Updater"><UserId>S-1-5-18</UserId><LogonType>ServiceAccount</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals><Actions Context="Updater"><Exec><Command>' + $command + '</Command><Arguments>update check</Arguments><WorkingDirectory>' + $directory + '</WorkingDirectory></Exec></Actions></Task>')
        # SYSTEM implies ServiceAccount when Task Scheduler omits that XML field.
        if ($null -eq $xml.SelectSingleNode('/t:Task/t:Principals/t:Principal/t:LogonType', $ns)) {
            $null = $expected.DocumentElement.Principals.Principal.RemoveChild($expected.SelectSingleNode('/t:Task/t:Principals/t:Principal/t:LogonType', $ns))
        }
        if ($null -eq $actions -or $null -eq $principals -or
            $actions.OuterXml -cne $expected.DocumentElement.Actions.OuterXml -or
            $principals.OuterXml -cne $expected.DocumentElement.Principals.OuterXml) { throw 'Unexpected updater action or principal.' }
        $settings = $xml.SelectSingleNode('/t:Task/t:Settings', $ns)
        $values = @{
            MultipleInstancesPolicy = 'IgnoreNew'; DisallowStartIfOnBatteries = 'false'; StopIfGoingOnBatteries = 'false'
            AllowHardTerminate = 'true'; StartWhenAvailable = 'false'; RunOnlyIfNetworkAvailable = 'false'
            AllowStartOnDemand = 'true'; Enabled = 'true'; Hidden = 'false'; RunOnlyIfIdle = 'false'
            WakeToRun = 'false'; ExecutionTimeLimit = 'PT1H'; Priority = '7'
            DisallowStartOnRemoteAppSession = 'false'; UseUnifiedSchedulingEngine = 'false'; Volatile = 'false'
        }
        Assert-Children $settings (@($values.Keys) + 'IdleSettings')
        if ($settings.Attributes.Count -ne 0) { throw 'Unexpected settings attributes.' }
        foreach ($child in $settings.ChildNodes) {
            if ($child.LocalName -eq 'IdleSettings') {
                Assert-Children $child @('Duration', 'WaitTimeout', 'StopOnIdleEnd', 'RestartOnIdle')
                if ($child.Attributes.Count -ne 0) { throw 'Unexpected idle attributes.' }
                $idle = @{ Duration = 'PT10M'; WaitTimeout = 'PT1H'; StopOnIdleEnd = 'true'; RestartOnIdle = 'false' }
                foreach ($item in $child.ChildNodes) { Assert-Value $item $idle[$item.LocalName] }
            } else { Assert-Value $child $values[$child.LocalName] }
        }
        foreach ($required in @('MultipleInstancesPolicy', 'DisallowStartIfOnBatteries', 'StopIfGoingOnBatteries', 'ExecutionTimeLimit')) {
            Assert-Value $settings.SelectSingleNode(('t:' + $required), $ns) $values[$required]
        }
        $triggers = $xml.SelectSingleNode('/t:Task/t:Triggers', $ns)
        Assert-Children $triggers @('TimeTrigger')
        if ($triggers.Attributes.Count -ne 0 -or $triggers.ChildNodes.Count -ne 1) { throw 'Unexpected updater triggers.' }
        $trigger = $triggers.FirstChild
        Assert-Children $trigger @('Repetition', 'StartBoundary', 'Enabled')
        if ($trigger.Attributes.Count -ne 0) { throw 'Unexpected trigger attributes.' }
        $boundary = $trigger.SelectSingleNode('t:StartBoundary', $ns)
        $date = [DateTime]::MinValue
        if ($null -eq $boundary -or -not [DateTime]::TryParseExact($boundary.InnerText, 'yyyy-MM-ddTHH:mm:ss',
            [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::None, [ref]$date)) { throw 'Unexpected start boundary.' }
        Assert-Value $boundary $boundary.InnerText
        $enabled = $trigger.SelectSingleNode('t:Enabled', $ns)
        if ($null -ne $enabled) { Assert-Value $enabled 'true' }
        $repeat = $trigger.SelectSingleNode('t:Repetition', $ns)
        Assert-Children $repeat @('Interval', 'StopAtDurationEnd')
        if ($repeat.Attributes.Count -ne 0) { throw 'Unexpected repetition attributes.' }
        Assert-Value $repeat.SelectSingleNode('t:Interval', $ns) 'PT1H'
        $stop = $repeat.SelectSingleNode('t:StopAtDurationEnd', $ns)
        if ($null -ne $stop) { Assert-Value $stop 'false' }
    }

    function Get-OwnedUpdate {
        try { $task = $taskFolder.GetTask($taskName) }
        catch {
            if ($_.Exception.GetBaseException().HResult -eq -2147024894) { return $null } # ERROR_FILE_NOT_FOUND only
            throw
        }
        [xml]$xml = $task.Xml
        # A matching path alone is not ownership: the installed executable must
        # already exist inside the pinned, privileged-owned application tree.
        # Authenticode is optional for local builds; filesystem trust is required.
        if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'Updater task has no protected installed executable.' }
        if ($task.Path -cne '\SecblitzUpdate' -or -not $task.Enabled) {
            throw 'SecblitzUpdate is not the expected owned task; refusing to change it.'
        }
        Assert-OwnedUpdateXml $xml $exe $root
        $sd = [Security.AccessControl.RawSecurityDescriptor]::new($task.GetSecurityDescriptor(7))
        if ($sd.Owner.Value -notin @('S-1-5-18', 'S-1-5-32-544') -or $null -eq $sd.DiscretionaryAcl) {
            throw 'Untrusted updater task owner or DACL.'
        }
        foreach ($ace in $sd.DiscretionaryAcl) {
            if ($ace -isnot [Security.AccessControl.CommonAce] -or $ace.AceQualifier -ne 'AccessAllowed' -or
                ($ace.SecurityIdentifier.Value -notin @('S-1-5-18', 'S-1-5-32-544') -and
                  ($ace.AccessMask -band (-bnot 0x1200a9)) -ne 0)) { throw 'Unsafe updater task ACL.' }
        }
        foreach ($sid in @('S-1-5-18', 'S-1-5-32-544')) {
            $full = @($sd.DiscretionaryAcl | Where-Object {
                $_.SecurityIdentifier.Value -eq $sid -and ($_.AccessMask -band 0x1f01ff) -eq 0x1f01ff
            })
            if ($full.Count -eq 0) { throw 'Missing privileged updater task access.' }
        }
        return $task
    }

    function Set-UpdatePreference([int]$Enabled) {
        $acl = [Security.AccessControl.RegistrySecurity]::new()
        $acl.SetSecurityDescriptorSddlForm('O:BAG:BAD:P(A;CI;KA;;;SY)(A;CI;KA;;;BA)(A;CI;KR;;;BU)')
        $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('Software\Secblitz',
            [Microsoft.Win32.RegistryKeyPermissionCheck]::ReadWriteSubTree,
            [Security.AccessControl.RegistryRights]::FullControl)
        if ($null -eq $key) {
            $key = [Microsoft.Win32.Registry]::LocalMachine.CreateSubKey('Software\Secblitz',
                [Microsoft.Win32.RegistryKeyPermissionCheck]::ReadWriteSubTree,
                [Microsoft.Win32.RegistryOptions]::None, $acl)
        } else {
            $key.SetAccessControl($acl)
        }
        try {
            $key.SetValue('AutoUpdatesEnabled', $Enabled, [Microsoft.Win32.RegistryValueKind]::DWord)
        } finally { $key.Dispose() }
    }

    function Enable-Updates {
        $existing = Get-OwnedUpdate
        $definition = $scheduler.NewTask(0)
        $definition.RegistrationInfo.Description = 'Secblitz automatic signed updates'
        $definition.Principal.Id = 'Updater'
        $definition.Principal.UserId = 'S-1-5-18'
        $definition.Principal.LogonType = 5 # ServiceAccount
        $definition.Principal.RunLevel = 1 # Highest
        $definition.Settings.MultipleInstances = 2 # IgnoreNew
        $definition.Settings.DisallowStartIfOnBatteries = $false
        $definition.Settings.StopIfGoingOnBatteries = $false
        $definition.Settings.StartWhenAvailable = $false
        $definition.Settings.ExecutionTimeLimit = 'PT1H'
        $definition.Settings.Enabled = $true
        $trigger = $definition.Triggers.Create(1) # Time; indefinite hourly repetition
        $trigger.StartBoundary = [DateTime]::Now.AddHours(1).ToString('yyyy-MM-ddTHH:mm:ss', [Globalization.CultureInfo]::InvariantCulture)
        $trigger.Repetition.Interval = 'PT1H'
        $exec = $definition.Actions.Create(0)
        $definition.Actions.Context = 'Updater'
        $exec.Path = '"' + $exe + '"'
        $exec.Arguments = 'update check'
        $exec.WorkingDirectory = $root
        # Create-only prevents a concurrent same-name registration being clobbered.
        # Updates require ownership validation immediately before registration.
        $flags = 2
        if ($null -ne $existing) { $null = Get-OwnedUpdate; $flags = 4 }
        $null = $taskFolder.RegisterTaskDefinition($taskName, $definition, ($flags -bor 16), 'S-1-5-18', $null, 5, $taskSddl)
        $taskFolder.GetTask($taskName).SetSecurityDescriptor($taskSddl, 16)
        $null = Get-OwnedUpdate
        Set-UpdatePreference 1
    }

    function Disable-Updates {
        if ($null -ne (Get-OwnedUpdate)) { $taskFolder.DeleteTask($taskName, 0) }
        Set-UpdatePreference 0
    }

    function Invoke-MonitorCommand([ValidateSet('install', 'uninstall')][string]$Verb) {
        # Explicit CreateProcess-style dispatch: no shell/PATHEXT lookup and no
        # ambient LASTEXITCODE (which can be unset when PowerShell shell-launches).
        $exitCode = -1
        $start = [Diagnostics.ProcessStartInfo]::new()
        $start.FileName = $exe
        $start.Arguments = 'service ' + $Verb
        $start.WorkingDirectory = $root
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        $process = [Diagnostics.Process]::new()
        $process.StartInfo = $start
        try {
            if (-not $process.Start()) { throw 'Cannot launch monitor maintenance.' }
            $output = $process.StandardOutput.ReadToEndAsync()
            $errors = $process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(180000)) { throw 'Monitor maintenance timed out; application retained.' }
            $exitCode = $process.ExitCode
            $null = $output.GetAwaiter().GetResult()
            $errorText = $errors.GetAwaiter().GetResult()
            if ($exitCode -ne 0) { throw "Monitor $Verb failed ($exitCode): $errorText" }
        } finally { $process.Dispose() }
    }

    function Stop-OwnedMonitor {
        if ($null -ne (Get-OwnedMonitor)) {
            $controller = Get-Service -Name SecblitzMonitor
            try {
                if ($controller.Status -ne 'Stopped') {
                    # ServiceController.Stop returns after sending the control;
                    # unlike Stop-Service, it does not wait without our timeout.
                    foreach ($dependent in $controller.DependentServices) {
                        try {
                            if ($dependent.Status -ne 'Stopped') { throw 'An active dependent service prevents monitor removal.' }
                        } finally { $dependent.Dispose() }
                    }
                    $deadline = [DateTime]::UtcNow.AddSeconds(180)
                    $sent = $false
                    while ($controller.Status -ne 'Stopped') {
                        if ([DateTime]::UtcNow -ge $deadline) { throw 'Timed out waiting for monitor stop.' }
                        if (-not $sent -and $controller.Status -in @('Running', 'Paused')) {
                            $controller.Stop()
                            $sent = $true
                        }
                        Start-Sleep -Milliseconds 250
                        $controller.Refresh()
                    }
                }
            } finally { $controller.Dispose() }
            $null = Get-OwnedMonitor
        }
    }

    # ---- Web protection: the SecblitzFilter service and its reconcile task -------
    $reconcileName = 'SecblitzFilterReconcile'

    function Get-OwnedFilter {
        $service = Get-CimInstance Win32_Service -Filter "Name='SecblitzFilter'"
        if ($null -ne $service) {
            if ($service.PathName -cne ('"' + $exe + '" filter run') -or
                $service.StartName -ine 'NT AUTHORITY\LocalService' -or
                $service.ServiceType -ne 'Own Process') {
                throw 'SecblitzFilter is not the expected owned service; refusing to change it.'
            }
        }
        return $service
    }

    function Stop-OwnedFilter {
        if ($null -eq (Get-OwnedFilter)) { return }
        $controller = Get-Service -Name SecblitzFilter
        try {
            $deadline = [DateTime]::UtcNow.AddSeconds(60)
            $sent = $false
            while ($controller.Status -ne 'Stopped') {
                if ([DateTime]::UtcNow -ge $deadline) { throw 'Timed out waiting for the web protection service to stop.' }
                if (-not $sent -and $controller.Status -in @('Running', 'Paused')) {
                    $controller.Stop()
                    $sent = $true
                }
                Start-Sleep -Milliseconds 250
                $controller.Refresh()
            }
        } finally { $controller.Dispose() }
        $null = Get-OwnedFilter
    }

    # Same explicit CreateProcess-style dispatch as the monitor command: no shell,
    # no PATHEXT lookup, no console window. Only two fixed argument strings exist.
    function Invoke-ExeCommand([ValidateSet('filter install', 'filter uninstall')][string]$Arguments) {
        $start = [Diagnostics.ProcessStartInfo]::new()
        $start.FileName = $exe
        $start.Arguments = $Arguments
        $start.WorkingDirectory = $root
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        $process = [Diagnostics.Process]::new()
        $process.StartInfo = $start
        try {
            if (-not $process.Start()) { throw 'Cannot launch web protection maintenance.' }
            $output = $process.StandardOutput.ReadToEndAsync()
            $errors = $process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(180000)) { throw 'Web protection maintenance timed out; application retained.' }
            $exitCode = $process.ExitCode
            $null = $output.GetAwaiter().GetResult()
            $errorText = $errors.GetAwaiter().GetResult()
            if ($exitCode -ne 0) { throw "Command '$Arguments' failed ($exitCode): $errorText" }
        } finally { $process.Dispose() }
    }

    function Assert-OwnedReconcileXml([xml]$xml, [string]$ExePath, [string]$AppRoot) {
        $ns = [Xml.XmlNamespaceManager]::new($xml.NameTable)
        $ns.AddNamespace('t', 'http://schemas.microsoft.com/windows/2004/02/mit/task')
        if ($xml.DocumentElement.LocalName -cne 'Task' -or
            $xml.DocumentElement.NamespaceURI -cne $ns.LookupNamespace('t')) { throw 'Unexpected task XML root.' }
        # Ownership is the account and the one action: SYSTEM running our own
        # executable with the fixed arguments. The triggers only decide when.
        $principals = @($xml.SelectNodes('/t:Task/t:Principals/t:Principal', $ns))
        if ($principals.Count -ne 1) { throw 'Unexpected reconcile principal.' }
        $user = $principals[0].SelectSingleNode('t:UserId', $ns)
        $level = $principals[0].SelectSingleNode('t:RunLevel', $ns)
        if ($null -eq $user -or $user.InnerText -cne 'S-1-5-18' -or
            $null -eq $level -or $level.InnerText -cne 'HighestAvailable') { throw 'Unexpected reconcile principal.' }
        $logon = $principals[0].SelectSingleNode('t:LogonType', $ns)
        if ($null -ne $logon -and $logon.InnerText -cne 'ServiceAccount') { throw 'Unexpected reconcile logon type.' }
        $actions = @($xml.SelectNodes('/t:Task/t:Actions/*', $ns))
        if ($actions.Count -ne 1 -or $actions[0].LocalName -cne 'Exec') { throw 'Unexpected reconcile action.' }
        $exec = $actions[0]
        $command = $exec.SelectSingleNode('t:Command', $ns)
        $arguments = $exec.SelectSingleNode('t:Arguments', $ns)
        $directory = $exec.SelectSingleNode('t:WorkingDirectory', $ns)
        if (@($exec.ChildNodes).Count -ne 3 -or $null -eq $command -or $null -eq $arguments -or $null -eq $directory -or
            ($command.InnerText -cne $ExePath -and $command.InnerText -cne ('"' + $ExePath + '"')) -or
            $arguments.InnerText -cne 'filter reconcile' -or $directory.InnerText -cne $AppRoot) {
            throw 'Unexpected reconcile action.'
        }
    }

    function Get-OwnedReconcile {
        try { $task = $taskFolder.GetTask($reconcileName) }
        catch {
            if ($_.Exception.GetBaseException().HResult -eq -2147024894) { return $null } # ERROR_FILE_NOT_FOUND only
            throw
        }
        [xml]$xml = $task.Xml
        if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'Reconcile task has no protected installed executable.' }
        if ($task.Path -cne '\SecblitzFilterReconcile') {
            throw 'SecblitzFilterReconcile is not the expected owned task; refusing to change it.'
        }
        Assert-OwnedReconcileXml $xml $exe $root
        $sd = [Security.AccessControl.RawSecurityDescriptor]::new($task.GetSecurityDescriptor(7))
        if ($sd.Owner.Value -notin @('S-1-5-18', 'S-1-5-32-544') -or $null -eq $sd.DiscretionaryAcl) {
            throw 'Untrusted reconcile task owner or DACL.'
        }
        foreach ($ace in $sd.DiscretionaryAcl) {
            if ($ace -isnot [Security.AccessControl.CommonAce] -or $ace.AceQualifier -ne 'AccessAllowed' -or
                ($ace.SecurityIdentifier.Value -notin @('S-1-5-18', 'S-1-5-32-544') -and
                  ($ace.AccessMask -band (-bnot 0x1200a9)) -ne 0)) { throw 'Unsafe reconcile task ACL.' }
        }
        foreach ($sid in @('S-1-5-18', 'S-1-5-32-544')) {
            $full = @($sd.DiscretionaryAcl | Where-Object {
                $_.SecurityIdentifier.Value -eq $sid -and ($_.AccessMask -band 0x1f01ff) -eq 0x1f01ff
            })
            if ($full.Count -eq 0) { throw 'Missing privileged reconcile task access.' }
        }
        return $task
    }

    function Enable-Reconcile {
        $existing = Get-OwnedReconcile
        $definition = $scheduler.NewTask(0)
        $definition.RegistrationInfo.Description = 'Secblitz web protection routing check'
        $definition.Principal.Id = 'Reconcile'
        $definition.Principal.UserId = 'S-1-5-18'
        $definition.Principal.LogonType = 5 # ServiceAccount
        $definition.Principal.RunLevel = 1 # Highest
        $definition.Settings.MultipleInstances = 2 # IgnoreNew
        $definition.Settings.DisallowStartIfOnBatteries = $false
        $definition.Settings.StopIfGoingOnBatteries = $false
        $definition.Settings.StartWhenAvailable = $true
        $definition.Settings.ExecutionTimeLimit = 'PT10M'
        $definition.Settings.Enabled = $true
        $boot = $definition.Triggers.Create(8) # at startup
        $boot.Enabled = $true
        $hourly = $definition.Triggers.Create(1) # Time; indefinite hourly repetition
        $hourly.StartBoundary = [DateTime]::Now.AddMinutes(5).ToString('yyyy-MM-ddTHH:mm:ss', [Globalization.CultureInfo]::InvariantCulture)
        $hourly.Repetition.Interval = 'PT1H'
        # The PC joined a network (NetworkProfile event 10000).
        $network = $definition.Triggers.Create(0)
        $network.Subscription = '<QueryList><Query Id="0" Path="Microsoft-Windows-NetworkProfile/Operational"><Select Path="Microsoft-Windows-NetworkProfile/Operational">*[System[Provider[@Name=''Microsoft-Windows-NetworkProfile''] and EventID=10000]]</Select></Query></QueryList>'
        $network.Enabled = $true
        $action = $definition.Actions.Create(0)
        $definition.Actions.Context = 'Reconcile'
        $action.Path = '"' + $exe + '"'
        $action.Arguments = 'filter reconcile'
        $action.WorkingDirectory = $root
        $flags = 2
        if ($null -ne $existing) { $null = Get-OwnedReconcile; $flags = 4 }
        $null = $taskFolder.RegisterTaskDefinition($reconcileName, $definition, ($flags -bor 16), 'S-1-5-18', $null, 5, $taskSddl)
        $taskFolder.GetTask($reconcileName).SetSecurityDescriptor($taskSddl, 16)
        $null = Get-OwnedReconcile
    }

    function Remove-Reconcile {
        if ($null -ne (Get-OwnedReconcile)) { $taskFolder.DeleteTask($reconcileName, 0) }
    }

    # ---- Full cleanup (uninstall) -------------------------------------------------
    # Delete only after the whole tree passed the checks: pinned (no reparse point,
    # no hard link), trusted owner, and nobody untrusted able to write. The web
    # protection service (LocalService) owns and writes some of its own files.
    function Assert-PurgeItem([string]$Path) {
        $pins.Add([SecblitzPaths]::Pin($Path))
        $item = Get-Item -LiteralPath $Path -Force
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point refused: $Path" }
        $acl = Get-Acl -LiteralPath $Path
        $ownerSid = $acl.GetOwner([Security.Principal.SecurityIdentifier]).Value
        if ($ownerSid -notin $trusted -and $ownerSid -ne 'S-1-5-19') { throw "Untrusted owner: $Path" }
        foreach ($rule in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
            if ($rule.AccessControlType -ne 'Allow') { continue }
            if ($rule.IdentityReference.Value -in $trusted) { continue }
            # Inherit-only entries describe children, which are checked themselves.
            if ($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly) { continue }
            $allowed = 0x1200a9 # Read/execute only.
            if ($rule.IdentityReference.Value -eq 'S-1-5-19') { $allowed = 0x1301bf }
            if (([int]$rule.FileSystemRights -band (-bnot $allowed)) -ne 0) { throw "Writable object refused: $Path" }
        }
        $sddl = $acl.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access)
        if ($sddl -match 'NO_ACCESS_CONTROL' -or $sddl -notmatch '\(A;[^)]*;;;(BA|SY)\)') { throw "Missing privileged ACL: $Path" }
    }

    function Assert-PurgeTree([string]$Path) {
        Assert-PurgeItem $Path
        if ((Get-Item -LiteralPath $Path -Force).PSIsContainer) {
            foreach ($child in Get-ChildItem -LiteralPath $Path -Force) {
                Assert-PurgeTree $child.FullName
            }
        }
    }

    function Remove-OwnedTree([string]$Path) {
        if ($null -eq (Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue)) { return }
        Assert-PurgeTree $Path
        # Pins forbid deletion: release them only now that everything passed.
        foreach ($pin in $pins) { $pin.Dispose() }
        $pins.Clear()
        if ((Get-Item -LiteralPath $Path -Force).PSIsContainer) {
            [IO.Directory]::Delete($Path, $true)
        } else {
            [IO.File]::Delete($Path)
        }
    }

    # Remove a logon Run value only when it starts our own executable.
    function Remove-OwnedRunValue([Microsoft.Win32.RegistryKey]$Hive, [string]$SubKey, [string]$Name, [string]$ExePath) {
        $key = $Hive.OpenSubKey($SubKey, $true)
        if ($null -eq $key) { return }
        try {
            $value = $key.GetValue($Name, $null)
            if ($value -is [string] -and $value.StartsWith('"' + $ExePath + '"', [StringComparison]::OrdinalIgnoreCase)) {
                $key.DeleteValue($Name, $false)
            }
        } finally { $key.Dispose() }
    }

    $drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($root))
    if ($drive.DriveType -ne 'Fixed' -or $drive.DriveFormat -ne 'NTFS') { throw 'A fixed NTFS installation volume is required.' }
    # Pin and validate from the volume root down before following any child path.
    $path = [IO.Path]::GetPathRoot($programFiles)
    Assert-SafeItem $path $true
    foreach ($part in $programFiles.Substring($path.Length).Split('\')) {
        if ($part) {
            $path = Join-Path $path $part
            Assert-SafeItem $path $true
        }
    }
    if (Test-Path -LiteralPath $root) { Assert-Tree $root }
    $monitor = Get-OwnedMonitor
    # Fail before replacing files or stopping the service on a name collision.
    $updateTask = Get-OwnedUpdate
    $filterService = Get-OwnedFilter
    $null = Get-OwnedReconcile
    switch ($Action) {
        Prepare {
            if (-not (Test-Path -LiteralPath $root)) {
                $acl = [Security.AccessControl.DirectorySecurity]::new()
                $acl.SetSecurityDescriptorSddlForm('O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;LS)(A;OICI;0x1200a9;;;BU)')
                $null = [IO.Directory]::CreateDirectory($root, $acl)
                # CreateDirectory can return an existing directory after a race.
                Assert-Tree $root
            }
            Protect-Item $root $true
            Stop-OwnedMonitor
            Stop-OwnedFilter
            # Exit 10: the monitor was running, 11: the web protection service
            # was, 12: both. Setup starts them again once the files are replaced.
            $monitorRan = $null -ne $monitor -and $monitor.State -in @('Running', 'Start Pending')
            $filterRan = $null -ne $filterService -and $filterService.State -in @('Running', 'Start Pending')
            if ($monitorRan -and $filterRan) { exit 12 }
            if ($filterRan) { exit 11 }
            if ($monitorRan) { exit 10 }
        }
        Secure {
            Protect-Item $root $true
            Protect-Item $exe $false
        }
        InstallMonitor {
            if ($null -eq $monitor) {
                Invoke-MonitorCommand 'install'
                if ($null -eq (Get-OwnedMonitor)) { throw 'Monitor command succeeded without registering the service.' }
            }
        }
        RemoveMonitor {
            Disable-Updates
            if ($null -ne $monitor) {
                Stop-OwnedMonitor
                Invoke-MonitorCommand 'uninstall'
                if ($null -ne (Get-OwnedMonitor)) { throw 'Service deletion is still pending; close service consoles and retry.' }
            }
        }
        ResumeMonitor {
            if ($null -ne $monitor) {
                $controller = Get-Service -Name SecblitzMonitor
                try {
                    if ($controller.Status -eq 'Stopped') { $controller.Start() }
                    $controller.WaitForStatus([ServiceProcess.ServiceControllerStatus]::Running, [TimeSpan]::FromSeconds(180))
                } finally { $controller.Dispose() }
            }
        }
        InstallFilter {
            # Registers the service disabled; it starts only when a switch goes on.
            Invoke-ExeCommand 'filter install'
            if ($null -eq (Get-OwnedFilter)) { throw 'Filter command succeeded without registering the service.' }
            Enable-Reconcile
        }
        ResumeFilter {
            if ($null -ne $filterService) {
                $controller = Get-Service -Name SecblitzFilter
                try {
                    if ($controller.Status -eq 'Stopped') { $controller.Start() }
                    $controller.WaitForStatus([ServiceProcess.ServiceControllerStatus]::Running, [TimeSpan]::FromSeconds(60))
                } finally { $controller.Dispose() }
            }
        }
        RemoveFilter {
            # The task goes first so it cannot put the routing rule back while the
            # service is being removed. A failing step must not leave the others
            # undone; the first failure is reported after all of them ran.
            $failure = $null
            try { Remove-Reconcile } catch { $failure = $_ }
            if (Test-Path -LiteralPath $exe -PathType Leaf) {
                try { Invoke-ExeCommand 'filter uninstall' } catch { if ($null -eq $failure) { $failure = $_ } }
            }
            if ($null -ne $failure) { throw $failure }
        }
        Purge {
            $failures = [Collections.Generic.List[string]]::new()
            try {
                Remove-OwnedRunValue ([Microsoft.Win32.Registry]::LocalMachine) 'Software\Microsoft\Windows\CurrentVersion\Run' 'SecblitzTray' $exe
            } catch { $failures.Add("Run value: $_") }
            try { Remove-OwnedTree (Join-Path $root 'Monitor') } catch { $failures.Add("Monitor folder: $_") }
            try {
                $programData = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
                $dataRoot = Join-Path $programData 'Secblitz'
                if ($null -ne (Get-Item -LiteralPath $dataRoot -Force -ErrorAction SilentlyContinue)) {
                    $path = [IO.Path]::GetPathRoot($programData)
                    Assert-SafeItem $path $true
                    foreach ($part in $programData.Substring($path.Length).Split('\')) {
                        if ($part) {
                            $path = Join-Path $path $part
                            Assert-SafeItem $path $true ($path -ieq $programData)
                        }
                    }
                    Remove-OwnedTree $dataRoot
                }
            } catch { $failures.Add("Data folder: $_") }
            try {
                [Microsoft.Win32.Registry]::LocalMachine.DeleteSubKeyTree('Software\Secblitz', $false)
            } catch { $failures.Add("Settings key: $_") }
            if ($failures.Count -gt 0) { throw ($failures -join '; ') }
        }
        EnableUpdates { Enable-Updates }
        DisableUpdates { Disable-Updates }
        PreserveUpdates {
            $enabled = [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\Software\Secblitz', 'AutoUpdatesEnabled', $null)
            if ($enabled -eq 1 -or ($null -eq $enabled -and $null -ne $updateTask)) {
                Enable-Updates
            } else {
                Disable-Updates
            }
        }
    }
    exit 0
} catch {
    Write-Error -ErrorRecord $_ -ErrorAction Continue
    exit 1
} finally {
    if (Test-Path variable:pins) { foreach ($pin in $pins) { $pin.Dispose() } }
    if (Test-Path variable:taskFolder) { $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($taskFolder) }
    if (Test-Path variable:scheduler) { $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($scheduler) }
}
