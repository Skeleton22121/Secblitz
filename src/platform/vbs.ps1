# Read-only facts for memory integrity and kernel stack protection, emitted as one JSON object
# that src/vbs.rs decodes. Loads no driver, writes nothing, starts no child process.
function VDword([string]$path, [string]$name) {
    if (!(Test-Path -LiteralPath $path)) { return $null }
    $key = Get-Item -LiteralPath $path
    if ($key.GetValueNames() -notcontains $name) { return $null }
    if ($key.GetValueKind($name) -ne [Microsoft.Win32.RegistryValueKind]::DWord) { return $null }
    $n = [int64]$key.GetValue($name)
    if ($n -lt 0) { $n += 4294967296 }
    return $n
}
function VNumbers($value) {
    if ($null -eq $value) { return @() }
    return @(@($value) | ForEach-Object { [int]$_ })
}
function VFlag($object, [string]$name) {
    $p = $object.PSObject.Properties[$name]
    if ($null -ne $p -and $p.Value -is [bool]) { return [bool]$p.Value }
    return $null
}

function VbsFacts {
    $dgPath = 'HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard'
    $hvciPath = "$dgPath\Scenarios\HypervisorEnforcedCodeIntegrity"
    $stackPath = "$dgPath\Scenarios\KernelShadowStacks"
    Load 'CimCmdlets'
    $guard = @(Get-CimInstance -Namespace 'root\Microsoft\Windows\DeviceGuard' -ClassName Win32_DeviceGuard -ErrorAction Stop)
    if ($guard.Count -ne 1) { throw 'Protection support could not be read' }
    $g = $guard[0]
    $os = @(Get-CimInstance -ClassName Win32_OperatingSystem)[0]
    $virt = $null
    foreach ($cpu in @(Get-CimInstance -ClassName Win32_Processor)) {
        $f = VFlag $cpu 'VirtualizationFirmwareEnabled'
        if ($f -eq $true) { $virt = $true } elseif ($f -eq $false -and $null -eq $virt) { $virt = $false }
    }
    $hvPresent = $null
    $cs = @(Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction SilentlyContinue)
    if ($cs.Count -ge 1) { $hvPresent = VFlag $cs[0] 'HypervisorPresent' }
    $boot = $null
    if ($os.LastBootUpTime -is [datetime]) { $boot = [DateTimeOffset]::new($os.LastBootUpTime).ToUnixTimeSeconds() }
    $build = $null
    $parsed = 0
    if ([int]::TryParse([string]$os.BuildNumber, [ref]$parsed)) { $build = $parsed }
    $vbsStatus = $null
    if ($null -ne $g.VirtualizationBasedSecurityStatus) { $vbsStatus = [int]$g.VirtualizationBasedSecurityStatus }
    $enabledHvci = VDword $hvciPath 'Enabled'
    $enabledStack = VDword $stackPath 'Enabled'

    # Drivers Windows refused to load since the last start (only worth asking
    # when the setting is on). Any trouble reading the log means "none known".
    $blocked = @()
    if (($enabledHvci -eq 1 -or $enabledStack -eq 1) -and $null -ne $boot) {
        try {
            Load 'Microsoft.PowerShell.Diagnostics'
            $start = [DateTimeOffset]::FromUnixTimeSeconds($boot).LocalDateTime
            $events = @(Get-WinEvent -FilterHashtable @{ LogName = 'Microsoft-Windows-CodeIntegrity/Operational'; Id = 3111, 3074; StartTime = $start } -MaxEvents 25 -ErrorAction Stop)
            foreach ($e in $events) {
                foreach ($m in [regex]::Matches([string]$e.Message, '[A-Za-z0-9_.\-]{1,64}\.sys')) { $blocked += $m.Value.ToLowerInvariant() }
            }
        } catch { }
    }

    return @{
        available = @(VNumbers $g.AvailableSecurityProperties)
        required = @(VNumbers $g.RequiredSecurityProperties)
        configured = @(VNumbers $g.SecurityServicesConfigured)
        running = @(VNumbers $g.SecurityServicesRunning)
        virtFirmware = $virt
        hypervisorPresent = $hvPresent
        vbsStatus = $vbsStatus
        build = $build
        mandatory = (VDword $dgPath 'Mandatory')
        enableVbs = (VDword $dgPath 'EnableVirtualizationBasedSecurity')
        requirePlatform = (VDword $dgPath 'RequirePlatformSecurityFeatures')
        lockVbs = (VDword $dgPath 'Locked')
        lockHvci = (VDword $hvciPath 'Locked')
        lockStack = (VDword $stackPath 'Locked')
        enabledHvci = $enabledHvci
        enabledStack = $enabledStack
        bootUnix = $boot
        blocked = @($blocked | Select-Object -Unique | Select-Object -First 8)
    }
}

try { Emit (VbsFacts) } catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
