# Private, non-mutating fixtures for src/platform/vbs.ps1. Management data, the
# registry and the event log are in-memory doubles; the production function is
# read from the script by its syntax tree and the closing Emit call never runs.
param([string]$VbsPath = (Join-Path $PSScriptRoot 'vbs.ps1'))
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($VbsPath, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($node in $ast.EndBlock.Statements) {
    if ($node -is [Management.Automation.Language.FunctionDefinitionAst]) { . ([scriptblock]::Create($node.Extent.Text)) }
}
$script:checks = 0
function Assert($ok, $message) { if (!$ok) { throw $message }; $script:checks++ }

function Load([string]$name) { }
$script:guard = $null
$script:registry = @{}
$script:events = @()
$script:eventFilter = $null
$script:readDenied = $false

function Get-CimInstance {
    param($Namespace, $ClassName, $ErrorAction)
    switch ($ClassName) {
        'Win32_DeviceGuard' {
            if ($null -eq $script:guard) { throw 'Invalid class' }
            return $script:guard
        }
        'Win32_OperatingSystem' { return [pscustomobject]@{ BuildNumber = '26100'; LastBootUpTime = [datetime]'2026-01-02T03:04:05' } }
        'Win32_Processor' { return $script:processors }
        default { throw "Unexpected class $ClassName" }
    }
}
function Test-Path { param($LiteralPath); return $script:registry.ContainsKey($LiteralPath) }
function Get-Item {
    param($LiteralPath)
    if ($script:readDenied) { throw 'registry access denied' }
    $values = $script:registry[$LiteralPath]
    $key = [pscustomobject]@{}
    $key | Add-Member ScriptMethod GetValueNames { return @($values.Keys) }.GetNewClosure()
    $key | Add-Member ScriptMethod GetValueKind { param($name); return $values[$name].kind }.GetNewClosure()
    $key | Add-Member ScriptMethod GetValue { param($name); return $values[$name].value }.GetNewClosure()
    return $key
}
function Get-WinEvent {
    param($FilterHashtable, $MaxEvents, $ErrorAction)
    $script:eventFilter = $FilterHashtable
    if ($null -eq $script:events) { throw 'No events were found' }
    return $script:events
}
function Dword($value) { return @{ kind = [Microsoft.Win32.RegistryValueKind]::DWord; value = $value } }
function Reset {
    $script:guard = [pscustomobject]@{
        AvailableSecurityProperties = @(1, 2, 3)
        RequiredSecurityProperties = @(1)
        SecurityServicesConfigured = @(2)
        SecurityServicesRunning = @()
        VirtualizationBasedSecurityStatus = 0
    }
    $script:processors = @([pscustomobject]@{ VirtualizationFirmwareEnabled = $true })
    $script:registry = @{}
    $script:events = @()
    $script:eventFilter = $null
    $script:readDenied = $false
}
$dg = 'HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard'
$hvci = "$dg\Scenarios\HypervisorEnforcedCodeIntegrity"
$stack = "$dg\Scenarios\KernelShadowStacks"

Reset
$f = VbsFacts
Assert (@($f.available).Count -eq 3 -and $f.available[2] -eq 3) 'Available properties changed'
Assert (@($f.running).Count -eq 0 -and $f.running -is [array]) 'Empty running list must stay an array'
Assert ($f.virtFirmware -eq $true -and $f.build -eq 26100 -and $f.vbsStatus -eq 0) 'Firmware, build or status not read'
Assert ($null -eq $f.mandatory -and $null -eq $f.enableVbs -and $null -eq $f.requirePlatform) 'Absent values must be null'
Assert ($null -eq $f.enabledHvci -and $null -eq $f.lockVbs) 'Absent registry values must be null'
Assert ($null -eq $script:eventFilter) 'The log must not be read while nothing is turned on'
Assert (@($f.blocked).Count -eq 0) 'No blocked drivers expected'
$json = ConvertTo-Json -InputObject $f -Depth 4 -Compress
Assert ($json -match '"available":\[1,2,3\]' -and $json -match '"running":\[\]') 'JSON shape changed'

Reset
$script:guard.AvailableSecurityProperties = 2
$script:guard.SecurityServicesRunning = @(2)
$f = VbsFacts
Assert (@($f.available).Count -eq 1 -and $f.available[0] -eq 2) 'A single value must become a list'
Assert (@($f.running).Count -eq 1 -and $f.running[0] -eq 2) 'One running service must stay a list'
$json = ConvertTo-Json -InputObject $f -Depth 4 -Compress
Assert ($json -match '"available":\[2\]') 'A one element list must serialize as a list'

# Registry values: only DWords count, and the sign is undone.
Reset
$script:registry[$dg] = @{ Mandatory = (Dword 1); EnableVirtualizationBasedSecurity = (Dword 0); RequirePlatformSecurityFeatures = (Dword 3); Locked = @{ kind = [Microsoft.Win32.RegistryValueKind]::String; value = '1' } }
$script:registry[$hvci] = @{ Enabled = (Dword 1); Locked = (Dword 1) }
$script:registry[$stack] = @{ Enabled = (Dword -1) }
$f = VbsFacts
Assert ($f.mandatory -eq 1 -and $f.enableVbs -eq 0 -and $f.requirePlatform -eq 3) 'Policy values not read'
Assert ($null -eq $f.lockVbs) 'A non-DWord value must read as absent'
Assert ($f.enabledHvci -eq 1 -and $f.lockHvci -eq 1 -and $f.enabledStack -eq 4294967295) 'Scenario values not read'

# Blocked drivers: names are lower-cased, de-duplicated, and limited to file names.
Reset
$script:registry[$hvci] = @{ Enabled = (Dword 1) }
$script:events = @(
    [pscustomobject]@{ Message = 'Code Integrity prevented \Device\HarddiskVolume3\Windows\System32\drivers\Bad.SYS from loading' },
    [pscustomobject]@{ Message = 'Again bad.sys and Other_1.sys' }
)
$f = VbsFacts
Assert ($f.blocked -is [array] -and ($f.blocked -join ',') -ceq 'bad.sys,other_1.sys') 'Blocked names must be lower case and unique'
Assert ($script:eventFilter.LogName -ceq 'Microsoft-Windows-CodeIntegrity/Operational') 'Wrong event log'
Assert (@($script:eventFilter.Id) -contains 3111 -and @($script:eventFilter.Id) -contains 3074 -and $script:eventFilter.Id.Count -eq 2) 'Wrong event ids'
Assert ($script:eventFilter.StartTime -is [datetime]) 'Events must be limited to the time since start-up'

# A log that cannot be read means "none known", never a failure.
Reset
$script:registry[$stack] = @{ Enabled = (Dword 1) }
$script:events = $null
$f = VbsFacts
Assert (@($f.blocked).Count -eq 0) 'An unreadable log must report no names'

Reset
$script:registry[$hvci] = @{ Enabled = (Dword 1) }
$script:events = @([pscustomobject]@{ Message = (1..12 | ForEach-Object { "d$_.sys" }) -join ' ' })
$f = VbsFacts
Assert (@($f.blocked).Count -eq 8) 'More than eight names returned'

# No DeviceGuard class (a very old or stripped PC): the function fails, the
# script's closing handler turns that into exit code 1.
Reset
$script:guard = $null
$caught = $null
try { VbsFacts | Out-Null } catch { $caught = $_.Exception.Message }
Assert ($null -ne $caught) 'A missing DeviceGuard class must fail'
Reset
$script:guard = @([pscustomobject]@{}, [pscustomobject]@{})
$caught = $null
try { VbsFacts | Out-Null } catch { $caught = $_.Exception.Message }
Assert ($caught -like '*could not be read*') 'Two DeviceGuard rows must fail'
$text = Get-Content -LiteralPath $VbsPath -Raw
Assert ($text -match 'exit 1') 'The script must exit 1 when facts cannot be read'

# Nothing in the script may write or start anything.
foreach ($word in @('Set-ItemProperty', 'New-ItemProperty', 'Remove-ItemProperty', 'Start-Process', 'Invoke-Expression', 'Set-CimInstance')) {
    Assert ($text -notmatch [regex]::Escape($word)) "The read-only script uses $word"
}
"vbs facts: $script:checks checks passed"
