# One-way Secure Boot certificate renewal. Load and Emit come from the compiled backend.
$sbKey = 'SYSTEM\CurrentControlSet\Control\SecureBoot'
$sbTaskPath = '\Microsoft\Windows\PI\'
$sbTaskName = 'Secure-Boot-Update'
function RegValue([string]$relative, [string]$name) {
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($relative, $false)
    if ($null -eq $key) { return $null }
    try { return $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } finally { $key.Dispose() }
}
function WriteAvailableUpdates([int]$value) {
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($sbKey, $true)
    if ($null -eq $key) { throw 'Secure Boot settings are unavailable' }
    try { $key.SetValue('AvailableUpdates', $value, [Microsoft.Win32.RegistryValueKind]::DWord) } finally { $key.Dispose() }
}
function HasSystemEvent([int[]]$ids) {
    try {
        $rows = @(Get-WinEvent -FilterHashtable @{LogName='System';Id=$ids;StartTime=[DateTime]::Now.AddDays(-400)} -MaxEvents 1)
        return ($rows.Count -gt 0)
    } catch {
        if ($_.FullyQualifiedErrorId -like 'NoMatchingEventsFound,*') { return $false }
        throw
    }
}
function RenewalRefusal {
    # Every condition the tip uses to offer the renewal is read again here, from the PC itself.
    try {
        $uefi = $false
        try { $uefi = [bool](Confirm-SecureBootUEFI) }
        catch {
            if ($_.Exception -is [PlatformNotSupportedException]) { return 'not_uefi' }
            return 'unreadable'
        }
        if (!$uefi) { return 'secure_boot_off' }
        $status = RegValue "$sbKey\Servicing" 'UEFICA2023Status'
        if ($status -is [string] -and $status -ceq 'Updated') { return 'already_updated' }
        $available = RegValue $sbKey 'AvailableUpdates'
        if ($null -ne $available -and ($available -isnot [int] -or $available -ne 0)) { return 'already_started' }
        if ($status -is [string] -and $status -ceq 'InProgress') { return 'already_started' }
        $computer = @(Get-CimInstance Win32_ComputerSystem)
        if ($computer.Count -ne 1) { return 'unreadable' }
        if (SbIsVirtualMachine ([string]$computer[0].Manufacturer) ([string]$computer[0].Model)) { return 'virtual_machine' }
        if (HasSystemEvent @(1032,1795,1796,1802,1803)) { return 'maker_blocked' }
        $task = @(Get-ScheduledTask -TaskPath $sbTaskPath -TaskName $sbTaskName -ErrorAction SilentlyContinue)
        if ($task.Count -eq 0) { return 'task_missing' }
        if ([string]$task[0].State -ceq 'Disabled') { return 'task_disabled' }
        if (SbHasOtherBootLoader (SbFirmwareBootLines)) { return 'other_system' }
        return $null
    } catch { return 'unreadable' }
}
try {
    if ($supportId -cne 'secureboot_renewal') { throw 'Unknown support operation' }
    Load 'CimCmdlets'
    Load 'SecureBoot'
    Load 'ScheduledTasks'
    Load 'Microsoft.PowerShell.Diagnostics'
    $os = Get-CimInstance Win32_OperatingSystem
    if ($os.ProductType -ne 1 -or [int]$os.BuildNumber -lt 10240 -or ![Environment]::Is64BitProcess) { throw 'Unsupported Windows client capability' }
    $reason = RenewalRefusal
    if ($null -ne $reason) {
        Emit @{ok=$true; result='refused'; reason=[string]$reason}
    } else {
        # Only AvailableUpdates is written. The opt-out and Microsoft opt-in values are never touched.
        WriteAvailableUpdates 0x5944
        $taskStarted = $false
        try { Start-ScheduledTask -TaskPath $sbTaskPath -TaskName $sbTaskName -ErrorAction Stop; $taskStarted = $true } catch { $taskStarted = $false }
        $confirmed = $false
        $now = 0x5944
        for ($attempt = 0; $taskStarted -and $attempt -lt 30; $attempt++) {
            $value = RegValue $sbKey 'AvailableUpdates'
            $status = RegValue "$sbKey\Servicing" 'UEFICA2023Status'
            if ($value -is [int]) { $now = $value }
            if (($value -is [int] -and $value -ne 0x5944) -or ($status -is [string] -and $status -ceq 'InProgress')) { $confirmed = $true; break }
            Start-Sleep -Seconds 2
        }
        Emit @{ok=$true; result='started'; confirmed=[bool]$confirmed; task_started=[bool]$taskStarted; available_updates=[int]$now}
    }
} catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
