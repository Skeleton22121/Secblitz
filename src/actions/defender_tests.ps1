# Host-only mocked gate tests. Never imports Defender or invokes a real setter.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$tokens = $null; $parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'defender.ps1'), [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
$body = $ast.EndBlock.Statements[0].Body.Statements | ForEach-Object { $_.Extent.Text }
$operation = [scriptblock]::Create(($body -join "`n"))
function Load($name) { }
function Test-Path {
    param($LiteralPath)
    if ($script:scenario -eq 'registry-unreadable') { throw 'Registry probe failed' }
    return ($script:scenario -eq 'local-policy' -or
        ($script:scenario -eq 'cloud' -and $LiteralPath -like '*CloudDomainJoin*') -or
        ($script:scenario -eq 'omadm' -and $LiteralPath -like '*OMADM*'))
}
function Get-ChildItem {
    param($LiteralPath)
    if ($script:scenario -in @('cloud','omadm')) { return @('enrollment') }
    return @()
}
function MdmRegistered {
    if ($script:scenario -eq 'mdm-unreadable') { throw 'MDM probe unavailable' }
    return ($script:scenario -eq 'mdm')
}
function CheckScopedPolicy($id) {
    if ($id -cne 'defender.support') { throw 'Wrong policy scope' }
    if ($script:scenario -eq 'policy') { throw 'Managed Defender policy' }
}
function CheckRsop($id) {
    if ($id -cne 'defender.support') { throw 'Wrong RSOP scope' }
    if ($script:scenario -eq 'rsop-unreadable') { throw 'RSOP probe unavailable' }
}
function Get-CimInstance {
    param($ClassName, $Namespace)
    switch ($ClassName) {
        'Win32_OperatingSystem' {
            if ($script:scenario -eq 'os-unreadable') { throw 'OS probe failed' }
            return @{ProductType=$(if ($script:scenario -eq 'server') { 3 } else { 1 });
                BuildNumber=$(if ($script:scenario -eq 'old-os') { '9600' } else { '22631' })}
        }
        'Win32_ComputerSystem' { return @{PartOfDomain=$(if ($script:scenario -eq 'domain-unknown') { $null } else { $script:scenario -eq 'domain' })} }
        'AntiVirusProduct' {
            if ($script:scenario -eq 'provider-unreadable') { throw 'Provider query failed' }
            if ($script:scenario -eq 'provider-missing') { return @() }
            if ($script:scenario -eq 'provider-other') { return @{instanceGuid='{unknown}'} }
            if ($script:scenario -eq 'provider-duplicate') { return @(@{instanceGuid='{D68DDC3A-831F-4fae-9E44-DA132C1ACF46}'}, @{instanceGuid='{D68DDC3A-831F-4fae-9E44-DA132C1ACF46}'}) }
            if ($script:scenario -eq 'provider-malformed') { return @{} }
            return @{instanceGuid='{D68DDC3A-831F-4fae-9E44-DA132C1ACF46}'}
        }
        default { throw 'Unexpected CIM query' }
    }
}
function Get-MpComputerStatus {
    if ($script:scenario -eq 'status-unreadable') { throw 'Status probe failed' }
    return @{AMServiceEnabled=($script:scenario -ne 'service-inactive');
        AntivirusEnabled=$(if ($script:scenario -eq 'status-unknown') { $null } else { $true });
        AMRunningMode=$(if ($script:scenario -eq 'passive') { 'Passive' } else { 'Normal' });
        IsTamperProtected=$true}
}
function Update-MpSignature {
    param($ErrorAction)
    $script:calls.Add('update')
    if ($script:scenario -eq 'command-error') { throw 'Mock command failure' }
}
function Start-MpScan {
    param($ScanType, $ErrorAction)
    if ($ScanType -cne 'QuickScan') { throw 'Wrong scan type' }
    $script:calls.Add('scan')
    if ($script:scenario -eq 'command-error') { throw 'Mock command failure' }
}
function Get-MpThreat {
    param($ErrorAction)
    if ($script:scenario -eq 'threats-unreadable') { throw 'Threat query failed' }
    return @($script:threats)
}
function Remove-MpThreat {
    param($ErrorAction)
    $script:calls.Add('remove')
    if ($script:scenario -eq 'command-error') { throw 'Mock command failure' }
    if ($script:scenario -ne 'stubborn') { $script:threats = @($script:threats | ForEach-Object { @{IsActive=$false; Id=$_.Id} }) }
}
function Start-Sleep { param($Seconds) $script:sleeps++ }
function Emit($value) {
    if ($value.ok -ne $true) { throw 'Invalid acknowledgement' }
    $script:ack = $true
    $script:reply = $value
}
$env:SystemRoot = 'C:\Windows'
$count = 0
foreach ($supportId in @('defender_update', 'defender_quickscan', 'defender_remove_threats', 'defender_update ', 'Defender_update', "defender_update'; exit")) {
    foreach ($script:scenario in @('allowed', 'domain', 'domain-unknown', 'mdm', 'mdm-unreadable', 'local-policy', 'policy', 'rsop-unreadable', 'provider-missing', 'provider-other', 'provider-unreadable', 'service-inactive', 'status-unknown', 'passive', 'command-error', 'cloud', 'omadm', 'registry-unreadable', 'os-unreadable', 'server', 'old-os', 'provider-duplicate', 'provider-malformed', 'status-unreadable', 'threats-unreadable', 'stubborn')) {
        $script:calls = [Collections.Generic.List[string]]::new()
        $script:threats = @(@{IsActive=$true; Id=1}, @{IsActive=$true; Id=2}, @{IsActive=$false; Id=3})
        $script:sleeps = 0
        $script:reply = $null
        $script:ack = $false
        $failed = $false
        try { & $operation } catch { $failed = $true }
        $known = $supportId -cin @('defender_update', 'defender_quickscan', 'defender_remove_threats')
        # 'stubborn' (Defender leaves the threats active) only matters to the threat removal; 'threats-unreadable' only blocks it.
        $isThreat = $supportId -ceq 'defender_remove_threats'
        $allowed = $known -and $script:scenario -in @('allowed', 'stubborn')
        if ($script:scenario -eq 'threats-unreadable') { $allowed = $known -and !$isThreat }
        if ($script:ack -ne $allowed -or $failed -eq $allowed) { throw "Wrong outcome: $supportId / $script:scenario" }
        $expectedCalls = if ($known -and $script:scenario -in @('allowed','command-error','stubborn')) { 1 } else { 0 }
        if ($script:scenario -eq 'threats-unreadable') { $expectedCalls = $(if ($known -and !$isThreat) { 1 } else { 0 }) }
        if ($script:calls.Count -ne $expectedCalls) { throw "Unexpected mutation: $supportId / $script:scenario" }
        if ($expectedCalls -eq 1 -and $script:calls[0] -cne $(if ($supportId -ceq 'defender_update') { 'update' } elseif ($isThreat) { 'remove' } else { 'scan' })) { throw 'Wrong action executed' }
        if ($isThreat -and $script:scenario -eq 'allowed') {
            if ($script:reply.found -ne 2 -or $script:reply.removed -ne 2 -or $script:reply.left -ne 0) { throw 'Wrong threat counts after removal' }
        }
        if ($isThreat -and $script:scenario -eq 'stubborn') {
            if ($script:reply.found -ne 2 -or $script:reply.removed -ne 0 -or $script:reply.left -ne 2 -or $script:sleeps -lt 10) { throw 'A threat Defender could not remove must be reported, not claimed' }
        }
        $count++
    }
}
Write-Output "$count mocked support-action gate cases passed"

# Exercise the actual policy-area selection, not just the scope passed by the
# support dispatcher. Default catalogs are not configured policy; Defender's
# provider/current areas are. Unrelated firewall state must not be queried.
$backend = Join-Path $PSScriptRoot '../platform/backend.ps1'
$ast = [System.Management.Automation.Language.Parser]::ParseFile($backend, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
foreach ($node in $ast.EndBlock.Statements) {
    if ($node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -in @('CheckScopedPolicy','CheckPolicyValues','PrivilegeRegistrySpec','PermissionService')) {
        . ([scriptblock]::Create($node.Extent.Text))
    }
}
function Test-Path { param($LiteralPath) return ($LiteralPath -ceq 'HKLM:\SOFTWARE\Microsoft\PolicyManager\providers') }
function Get-ChildItem { param($LiteralPath) return @{PSPath='FixtureProvider'} }
function HasValues($path) {
    if ($path -cne 'HKLM:\SOFTWARE\Policies\Microsoft\Windows Defender') { throw 'Wrong policy tree' }
    return $false
}
function PolicyValues($path) {
    $script:paths.Add($path)
    if ($path -like '*Firewall*' -or $path -like '*PolicyManager\default*') { throw 'Incorrect policy scope' }
    if (($script:policyCase -eq 'current' -and $path -like '*current*\Defender') -or
        ($script:policyCase -eq 'provider' -and $path -like 'FixtureProvider*\Defender')) {
        return @{AllowRealtimeMonitoring=@{kind='DWord';value=1}}
    }
    return @{}
}
function Join-Path($Path, $ChildPath) { return "$Path\$ChildPath" }
foreach ($script:policyCase in @('empty', 'current', 'provider')) {
    $script:paths = [Collections.Generic.List[string]]::new()
    $failed = $false
    try { CheckScopedPolicy 'defender.support' } catch { $failed = $true }
    if ($failed -ne ($script:policyCase -ne 'empty')) { throw "Policy scope outcome: $script:policyCase" }
    if ($script:policyCase -eq 'empty' -and $script:paths.Count -ne 4) { throw 'Missing Defender policy areas' }
}
Write-Output '3 production policy-scope cases passed'
