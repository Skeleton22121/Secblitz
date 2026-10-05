# Invoked only through a compiled script and an allowlisted action. No journal
# strings, paths, commands, update IDs, package IDs, or credentials enter here.
function MaintenanceGate {
    Load 'CimCmdlets'
    $os = Get-CimInstance Win32_OperatingSystem
    if ($os.ProductType -ne 1 -or [int]$os.BuildNumber -lt 10240 -or ![Environment]::Is64BitProcess) { throw 'Unsupported Windows client' }
    $cs = Get-CimInstance Win32_ComputerSystem
    if ($cs.PartOfDomain -isnot [bool] -or $cs.PartOfDomain) { throw 'Managed or unknown ownership' }
    if (MdmRegistered) { throw 'Managed device' }
    foreach ($p in @('HKLM:\SOFTWARE\Microsoft\Provisioning\OMADM\Accounts','HKLM:\SYSTEM\CurrentControlSet\Control\CloudDomainJoin\JoinInfo')) {
        if ((Test-Path -LiteralPath $p) -and @(Get-ChildItem -LiteralPath $p).Count -gt 0) { throw 'Enrollment evidence' }
    }
    foreach ($p in @('System32\GroupPolicy\Machine\Registry.pol','System32\GroupPolicy\gpt.ini','System32\GroupPolicy\Machine\Microsoft\Windows NT\SecEdit\GptTmpl.inf','System32\GroupPolicy\Machine\Preferences\Registry\Registry.xml')) {
        if (Test-Path -LiteralPath ([IO.Path]::Combine($env:SystemRoot,$p))) { throw 'Local policy evidence' }
    }
    CheckScopedPolicy 'permissions.service.wuauserv'
    CheckRsop 'permissions.service.wuauserv'
    # /LimitAccess blocks Windows Update, not a configured alternate repair
    # source (which can be a network share). Do not execute under such policy.
    foreach ($p in @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Servicing','HKLM:\SOFTWARE\Policies\Microsoft\Windows\Servicing')) {
        if (HasValues $p) { throw 'Configured servicing source/policy' }
    }
    if ($maintenanceKind -ceq 'defender') {
        CheckScopedPolicy 'defender.support'
        CheckRsop 'defender.support'
        $providers = @(Get-CimInstance -Namespace root\SecurityCenter2 -ClassName AntiVirusProduct)
        if ($providers.Count -ne 1 -or $providers[0].instanceGuid -ne '{D68DDC3A-831F-4fae-9E44-DA132C1ACF46}') { throw 'Defender provider unknown' }
        Load 'Defender'
        $s = Get-MpComputerStatus
        if ($s.AMServiceEnabled -isnot [bool] -or $s.AntivirusEnabled -isnot [bool] -or !$s.AMServiceEnabled -or !$s.AntivirusEnabled -or $s.AMRunningMode -cne 'Normal') { throw 'Defender not active' }
        # A supported scan works with tamper protection enabled. Never disable it.
    }
    return $os
}
function Epoch($date) {
    if ($date -isnot [DateTime] -or $date.Year -lt 1970) { return [uint64]0 }
    return [uint64]([DateTimeOffset]::new($date.ToUniversalTime()).ToUnixTimeSeconds())
}
try {
    if ($maintenanceKind -cnotin @('servicing','defender') -or $maintenanceAction -cnotin @('probe','scan','verify')) { throw 'Unknown maintenance action' }
    $os = MaintenanceGate
    if ($maintenanceAction -ceq 'scan') {
        if ($maintenanceKind -cne 'defender') { throw 'Invalid scan kind' }
        # Module loading and policy queries can be slow. The numeric, engine-
        # supplied deadline binds actual scan submission, not PowerShell startup.
        $now = [uint64]([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())
        if ($now -lt $maintenanceNotBefore -or $now -ge $maintenanceExpiresAt) { throw 'Scan approval/readiness expired' }
        $null = Start-MpScan -ScanType QuickScan -ErrorAction Stop
        Emit @{ acknowledged=$true }
    } else {
        $pending = $false
        foreach ($p in @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending','HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired')) {
            if (Test-Path -LiteralPath $p) { $pending = $true }
        }
        $session = Get-Item -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager'
        if ($null -ne $session.GetValue('PendingFileRenameOperations')) { $pending = $true }
        $start = $null; $end = $null
        if ($maintenanceKind -ceq 'defender') {
            $status = Get-MpComputerStatus
            $start = Epoch $status.QuickScanStartTime
            $end = Epoch $status.QuickScanEndTime
        }
        # Defender may use its existing cloud protection. Failure/offline/unknown
        # costs are not invented as unmetered; only this soft gate is exceptable.
        $unmetered = $null
        if ($maintenanceKind -ceq 'defender') {
            try {
                $profile = [Windows.Networking.Connectivity.NetworkInformation, Windows.Networking.Connectivity, ContentType=WindowsRuntime]::GetInternetConnectionProfile()
                if ($null -ne $profile) {
                    $cost = $profile.GetConnectionCost()
                    if ($null -ne $cost -and $cost.Roaming -is [bool] -and $cost.OverDataLimit -is [bool]) {
                        switch ([string]$cost.NetworkCostType) {
                            'Unrestricted' { $unmetered = (!$cost.Roaming -and !$cost.OverDataLimit) }
                            'Fixed' { $unmetered = $false }
                            'Variable' { $unmetered = $false }
                        }
                    }
                }
            } catch { $unmetered = $null }
        }
        Emit @{ boot_time=(Epoch $os.LastBootUpTime); reboot_pending=$pending; quick_start=$start; quick_end=$end; unmetered=$unmetered }
    }
} catch { [Console]::Error.WriteLine('Maintenance probe/scan failed'); exit 1 }
