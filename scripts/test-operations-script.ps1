# Cross-platform parser and scan-submission boundary tests. No Windows probe,
# Defender command, servicing executable, registry API or elevation is invoked.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$root = [IO.Path]::GetFullPath([IO.Path]::Combine($PSScriptRoot, '..'))
$source = [IO.File]::ReadAllText([IO.Path]::Combine($root, 'src/operations/probe.ps1'))
$backend = [IO.File]::ReadAllText([IO.Path]::Combine($root, 'src/platform/backend.ps1'))
$delimiter = "`ntry {`n    switch -CaseSensitive (`$action) {"
$parts = $backend.Split(@($delimiter), [StringSplitOptions]::None)
if ($parts.Count -ne 2) { throw 'Policy helper dispatcher boundary changed' }
$tokens = $null; $errors = $null
$null = [Management.Automation.Language.Parser]::ParseInput("`$inputJson=`$null`n$($parts[0])`n$source", [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
$ast = [Management.Automation.Language.Parser]::ParseInput($source, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
$try = @($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.TryStatementAst] })
if ($try.Count -ne 1) { throw 'Ambiguous action body' }
$text = $try[0].Body.Extent.Text
$body = [scriptblock]::Create($text.Substring(1, $text.Length - 2))

function MaintenanceGate { return [pscustomobject]@{} }
function Start-MpScan {
    [CmdletBinding()] param([string]$ScanType)
    if ($ScanType -cne 'QuickScan') { throw 'Scan arguments escaped allowlist' }
    $script:scans++
}
function Emit($value) { $script:acknowledged = $value.acknowledged }

foreach ($case in @('valid','expired','not_yet_valid','wrong_kind','wrong_action')) {
    $now = [uint64]([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())
    $maintenanceKind = 'defender'; $maintenanceAction = 'scan'
    $maintenanceNotBefore = $now - 1; $maintenanceExpiresAt = $now + 60
    $script:scans = 0; $script:acknowledged = $false
    switch ($case) {
        'expired' { $maintenanceExpiresAt = $now }
        'not_yet_valid' { $maintenanceNotBefore = $now + 60 }
        'wrong_kind' { $maintenanceKind = 'servicing' }
        'wrong_action' { $maintenanceAction = "scan'; Start-Process cmd" }
    }
    $failed = $false
    try { & $body } catch { $failed = $true }
    if ($case -eq 'valid') {
        if ($failed -or $script:scans -ne 1 -or !$script:acknowledged) { throw 'Approved exact scan was not submitted once' }
    } elseif (!$failed -or $script:scans -ne 0 -or $script:acknowledged) { throw "Unsafe scan submission: $case" }
}
# Exercise the real gate with isolated providers: absence, managed, unknown and
# alternate source policy must not collapse to the same authorization outcome.
$gate = @($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -ceq 'MaintenanceGate' })
if ($gate.Count -ne 1) { throw 'Missing maintenance gate' }
. ([scriptblock]::Create($gate[0].Extent.Text))
function Load([string]$name) {}
function Get-CimInstance([string]$ClassName) {
    switch ($ClassName) {
        'Win32_OperatingSystem' { [pscustomobject]@{ProductType=1;BuildNumber='22631'} }
        'Win32_ComputerSystem' { [pscustomobject]@{PartOfDomain=$script:domain} }
        default { throw 'Unexpected provider query' }
    }
}
function MdmRegistered { if ($script:unknownMdm) { throw 'Unavailable registration' }; return $script:mdm }
function Test-Path { $false }
function CheckScopedPolicy([string]$id) {}
function CheckRsop([string]$id) {}
function HasValues([string]$path) {
    if ($script:unknownSource) { throw 'Unreadable policy' }
    return ($script:sourcePolicy -and $path -ceq 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Servicing')
}
$maintenanceKind = 'servicing'
foreach ($case in @('clean','domain','unknown_domain','mdm','unknown_mdm','source_policy','unknown_source')) {
    $script:domain = $false; $script:mdm = $false; $script:unknownMdm = $false
    $script:sourcePolicy = $false; $script:unknownSource = $false
    switch ($case) {
        'domain' { $script:domain = $true }
        'unknown_domain' { $script:domain = 'false' }
        'mdm' { $script:mdm = $true }
        'unknown_mdm' { $script:unknownMdm = $true }
        'source_policy' { $script:sourcePolicy = $true }
        'unknown_source' { $script:unknownSource = $true }
    }
    $failed = $false
    try { $null = MaintenanceGate } catch { $failed = $true }
    if ($failed -ne ($case -ne 'clean')) { throw "Incorrect policy authority: $case" }
}
'Operations PowerShell parser, submission and policy-gate tests passed (12 cases).'
