# Read-only task/preference health probe, embedded in the trusted executable.
# Inbox modules only (by absolute path, no autoload); no shell commands,
# updater/Engine locks, task changes or networking.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try {
    $moduleRoot = [IO.Path]::Combine($env:SystemRoot, 'System32\WindowsPowerShell\v1.0\Modules')
    $env:PSModulePath = $moduleRoot
    $PSModuleAutoLoadingPreference = 'None'
    $null = Import-Module ([IO.Path]::Combine($moduleRoot, 'Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1')) -ErrorAction Stop
    $app = [IO.Path]::Combine([Environment]::GetFolderPath('ProgramFiles'), 'Secblitz')
    $exe = [IO.Path]::Combine($app, 'secblitz.exe')
    $trusted = @('S-1-5-18', 'S-1-5-32-544')
    $preference = $null
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('Software\Secblitz')
    if ($null -ne $key) {
        try {
            $acl = $key.GetAccessControl()
            if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin $trusted -or
                $acl.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access) -match 'NO_ACCESS_CONTROL') { throw 'Unsafe preference ACL' }
            foreach ($rule in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
                if ($rule.AccessControlType -eq 'Allow' -and $rule.IdentityReference.Value -notin $trusted -and
                    ([int]$rule.RegistryRights -band (-bnot 0x20019)) -ne 0) { throw 'Writable preference' }
            }
            $preference = $key.GetValue('AutoUpdatesEnabled', $null)
            if ($null -ne $preference -and ($key.GetValueKind('AutoUpdatesEnabled') -ne 'DWord' -or $preference -notin @(0, 1))) { throw 'Invalid preference' }
        } finally { $key.Dispose() }
    }
    $scheduler = New-Object -ComObject Schedule.Service
    $scheduler.Connect()
    $folder = $scheduler.GetFolder('\')
    $task = $null
    try { $task = $folder.GetTask('SecblitzUpdate') }
    catch { if ($_.Exception.GetBaseException().HResult -ne -2147024894) { throw } }
    if ($null -eq $task) {
        if ($preference -eq 1) { throw 'Enabled updater task missing' }
        [Console]::WriteLine('absent')
        exit 0
    }
    if ($preference -eq 0 -or $task.Path -cne '\SecblitzUpdate' -or -not $task.Enabled -or $task.State -notin @(3, 4)) { throw 'Updater task unavailable' }
    $definition = $task.Definition
    if ($definition.Actions.Count -ne 1) { throw 'Unexpected actions' }
    $action = $definition.Actions.Item(1)
    if ($action.Type -ne 0 -or $action.Path -cnotin @($exe, ('"' + $exe + '"')) -or
        $action.Arguments -cne 'update check' -or $action.WorkingDirectory -cne $app) { throw 'Unexpected updater action' }
    # Task Scheduler may return the canonical SYSTEM account name instead of
    # its SID, including for tasks registered by our installer.
    if ($definition.Principal.UserId -notin @('S-1-5-18', 'SYSTEM') -or $definition.Principal.LogonType -ne 5 -or
        $definition.Principal.RunLevel -ne 1 -or -not $definition.Settings.Enabled -or
        $definition.Settings.MultipleInstances -ne 2) { throw 'Unexpected updater principal/settings' }
    if ($definition.Triggers.Count -ne 1) { throw 'Unexpected triggers' }
    $trigger = $definition.Triggers.Item(1)
    if ($trigger.Type -ne 1 -or -not $trigger.Enabled -or $trigger.Repetition.Interval -cne 'PT1H') { throw 'Unexpected update cadence' }
    $sd = [Security.AccessControl.RawSecurityDescriptor]::new($task.GetSecurityDescriptor(7))
    if ($sd.Owner.Value -notin $trusted -or $null -eq $sd.DiscretionaryAcl) { throw 'Unsafe task descriptor' }
    foreach ($ace in $sd.DiscretionaryAcl) {
        if ($ace -isnot [Security.AccessControl.CommonAce] -or $ace.AceQualifier -ne 'AccessAllowed' -or
            ($ace.SecurityIdentifier.Value -notin $trusted -and ($ace.AccessMask -band (-bnot 0x1200a9)) -ne 0)) { throw 'Writable task' }
    }
    foreach ($sid in $trusted) {
        $full = @($sd.DiscretionaryAcl | Where-Object { $_.SecurityIdentifier.Value -eq $sid -and ($_.AccessMask -band 0x1f01ff) -eq 0x1f01ff })
        if ($full.Count -eq 0) { throw 'Missing task access' }
    }
    # LastTaskResult may refer to the currently running update (or no prior run);
    # it is not evidence of this installation's health. Do not assert otherwise.
    [Console]::WriteLine('ready')
    exit 0
} catch {
    [Console]::Error.WriteLine('Update task health verification failed')
    exit 1
}
