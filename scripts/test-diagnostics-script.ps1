# Synthetic provider/serialization tests. Runs on pwsh and Windows PowerShell
# 5.1; never queries Windows, profiles, registry, COM or a network endpoint.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$root = [IO.Path]::GetFullPath([IO.Path]::Combine($PSScriptRoot, '..'))
function Parse([string]$relative) {
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseInput([IO.File]::ReadAllText([IO.Path]::Combine($root,$relative)), [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
    return $ast
}
$common = Parse 'src/diagnostics/common.ps1'
# Install definitions only; do not run the Windows module bootstrap.
foreach ($definition in $common.EndBlock.Statements) {
    if ($definition -is [Management.Automation.Language.FunctionDefinitionAst]) {
        . ([scriptblock]::Create($definition.Extent.Text))
    }
}
$ast = Parse 'src/diagnostics/probes.ps1'
$null = Parse 'src/diagnostics/browsers.ps1'
$try = @($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.TryStatementAst] })
if ($try.Count -ne 1) { throw 'Ambiguous probe dispatcher' }
$text = $try[0].Body.Extent.Text
$body = [scriptblock]::Create($text.Substring(1,$text.Length-2))
function Load([string]$name) {}
function Cim([string]$class, [string]$namespace) {
    if ($script:failClass -ceq $class) { throw 'Synthetic provider failure' }
    switch ($class) {
        'Win32_ComputerSystem' { [pscustomobject]@{PartOfDomain=$false} }
        'Win32_DeviceGuard' { [pscustomobject]@{VirtualizationBasedSecurityStatus=[uint32]2;SecurityServicesConfigured=[uint32[]]@(2);SecurityServicesRunning=[uint32[]]$script:services} }
        'AntiVirusProduct' { [pscustomobject]@{displayName='Synthetic AV';instanceGuid='{D68DDC3A-831F-4FAE-9E44-DA132C1ACF46}';productState=[uint32]397568} }
        'FirewallProduct' { }
        default { throw 'Unmocked CIM query' }
    }
}
function PolicyValues([string]$path) { $false }
function ChildIndicator([string]$path) { $false }
function MdmRegistered { $false }
function Get-MpPreference {
    [pscustomobject]@{AttackSurfaceReductionRules_Ids=[string[]]$script:asrIds;AttackSurfaceReductionRules_Actions=[byte[]]$script:asrActions;EnableControlledFolderAccess=[uint32]1}
}
function Get-DnsClientServerAddress { [pscustomobject]@{AddressFamily=[uint16]2;ServerAddresses=[string[]]@('192.0.2.1')} }
function Probe([string]$id) {
    $raw = & { param($probe) & $body } $id
    if ($raw -isnot [string]) { throw "Pipeline contamination in $id" }
    return ConvertFrom-Json -InputObject $raw
}
function Assert([bool]$ok, [string]$message) { if (!$ok) { throw $message } }
$script:failClass = ''
$script:services = @()
$script:asrIds = @(); $script:asrActions = @()
foreach ($count in @(0,1,2)) {
    $script:services = @(@(1,2) | Select-Object -First $count)
    $v = Probe 'Vbs'
    Assert ($v.running_services.value -is [array] -and $v.running_services.value.Count -eq $count) 'CIM array cardinality changed in JSON'
    $script:asrIds = @(@('d4f940ab-401b-4efc-aadc-ad5f3c50688a','3b576869-a4ec-4529-8536-b80a7769e899') | Select-Object -First $count)
    $script:asrActions = @(@(1,2) | Select-Object -First $count)
    $v = Probe 'DefenderPolicy'
    Assert ($v.asr.value.items -is [array] -and $v.asr.value.items.Count -eq $count) 'ASR array cardinality changed in JSON'
    Assert ($v.cfa_mode.state -ceq 'Known' -and $v.cfa_mode.value -eq 1) 'CFA enum was not numeric'
}
$script:asrActions = @()
$v = Probe 'DefenderPolicy'
Assert ($v.asr.state -ceq 'Unknown' -and $v.cfa_mode.state -ceq 'Known') 'Invalid ASR arrays erased sibling CFA evidence'
$v = Probe 'SecurityProviders'
Assert ($v.antivirus.value.items -is [array] -and $v.antivirus.value.items.Count -eq 1) 'Singleton provider lost its array'
Assert ($v.firewall.value.items -is [array] -and $v.firewall.value.items.Count -eq 0) 'Empty successful provider query became null'
$script:failClass = 'AntiVirusProduct'
$v = Probe 'SecurityProviders'
Assert ($v.antivirus.state -ceq 'Unknown' -and $v.firewall.state -ceq 'Known') 'Provider failure erased unrelated evidence'
$v = Probe 'Management'
Assert ($v.domain_joined.value -is [bool] -and !$v.domain_joined.value) 'False membership was lost or coerced'
$script:failClass = 'Win32_ComputerSystem'
$v = Probe 'Management'
Assert ($v.domain_joined.state -ceq 'Unknown' -and $v.mdm_registered.state -ceq 'Known') 'Failed domain query became unmanaged'
$v = Probe 'Dns'
Assert ($v.interfaces.value.items[0].server_count -eq 1) 'DNS count incorrect'
Assert (!(($v | ConvertTo-Json -Depth 16) -match '192\.0\.2\.1')) 'DNS endpoint leaked into report'
$v = (Prop ([pscustomobject]@{Present=$false}) 'Present') | ConvertTo-Json -Compress | ConvertFrom-Json
Assert ($v.value -is [bool] -and !$v.value) 'False property became missing'
$v = (Prop ([pscustomobject]@{}) 'Missing') | ConvertTo-Json -Compress | ConvertFrom-Json
Assert ($v.state -ceq 'Unknown') 'Missing property became known'
$disk = [pscustomobject]@{HealthStatus='Healthy';CimInstanceProperties=@{HealthStatus=[pscustomobject]@{Value=[uint16]0}}}
$v = Code $disk 'HealthStatus'
Assert ($v.state -ceq 'Known' -and $v.value -eq 0) 'Adapted CIM enum ignored its raw numeric value'
$disk.CimInstanceProperties.HealthStatus.Value='0'
Assert ((Code $disk 'HealthStatus').state -ceq 'Unknown') 'CIM string was coerced to numeric evidence'
'Diagnostics parser, provider isolation and JSON cardinality tests passed.'
