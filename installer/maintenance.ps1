# Invoked only by elevated Setup/Uninstall. Never invokes apply or a bare exe.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Validate', 'Prepare', 'Secure', 'InstallMonitor', 'RemoveMonitor', 'ResumeMonitor', 'EnableUpdates', 'DisableUpdates', 'PreserveUpdates')][string]$Action,
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
        if ($Action -ne 'RemoveMonitor' -or
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

    function Assert-SafeItem([string]$Path, [bool]$Ancestor = $false) {
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
        if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin $trusted) {
            throw "Untrusted owner: $Path"
        }
        foreach ($rule in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
            if ($rule.AccessControlType -ne 'Allow') { continue }
            if ($rule.IdentityReference.Value -in $trusted) { continue }
            if ($Ancestor -and ($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly)) { continue }
            $allowed = 0x1200a9 # Read/execute, never write/delete/change owner or ACL.
            if ($Ancestor) { $allowed = $allowed -bor 6 } # Windows root/Program Files create-child ACEs.
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
            if ($null -ne $monitor -and $monitor.State -in @('Running', 'Start Pending')) { exit 10 }
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
