# Windows-only, non-mutating integration regression for the native PS5.1
# NetSecurity parameter binder. Every native setter call is forced to -WhatIf.
param([string]$BackendPath = (Join-Path $PSScriptRoot 'backend.ps1'))
$ErrorActionPreference = 'Stop'
$global:ProgressPreference = 'SilentlyContinue'
Set-StrictMode -Version 2
Import-Module (Join-Path ([Environment]::GetFolderPath('Windows')) 'System32\WindowsPowerShell\v1.0\Modules\NetSecurity\NetSecurity.psd1') -ErrorAction Stop
$nativeSetter = Get-Command NetSecurity\Set-NetFirewallProfile -ErrorAction Stop
$nativeReader = Get-Command NetSecurity\Get-NetFirewallProfile -ErrorAction Stop
function Snapshot {
    foreach ($store in @('PersistentStore','ActiveStore')) {
        & $nativeReader -PolicyStore $store | Sort-Object Name | Select-Object Name,Enabled,DefaultInboundAction,DefaultOutboundAction
    }
}
$before = @(Snapshot) | ConvertTo-Json -Depth 4 -Compress
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($BackendPath,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
$node=@($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -eq 'WriteControl' })
if ($node.Count -ne 1) { throw 'Production WriteControl not found' }
. ([scriptblock]::Create($node[0].Extent.Text))
$specNode=@($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -eq 'PrivilegeRegistrySpec' })
if ($specNode.Count -ne 1) { throw 'Production PrivilegeRegistrySpec not found' }
. ([scriptblock]::Create($specNode[0].Extent.Text))
$permissionNode=@($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -eq 'PermissionService' })
if ($permissionNode.Count -ne 1) { throw 'Production PermissionService not found' }
. ([scriptblock]::Create($permissionNode[0].Extent.Text))
$defenderNames=@{}
$script:calls=0
# Only eligibility/readback is simulated. The arguments assembled by the real
# WriteControl cross the actual inbox cmdlet binder, always with WhatIf forced.
function Gate($id) {}
function ReadControl($id) { return $script:requested }
function ObserveControl($id) { return @{eligible=$true} }
function ReadEffectiveFirewall($id) { return @{kind='enabled';value=$script:requested} }
function Get-NetFirewallProfile { param($PolicyStore,$Name); return @{Enabled=[string]$script:requested} }
function Set-NetFirewallProfile {
    param($PolicyStore,$Name,$Enabled)
    if ($Enabled -isnot [string] -or $Enabled -cnotin @('True','False')) { throw 'Production Enabled argument is not an enum-name token' }
    & $nativeSetter @PSBoundParameters -WhatIf -ErrorAction Stop
    $script:calls++
}
foreach ($requested in @($true,$false)) {
    $script:requested=$requested
    WriteControl 'firewall.public.enabled' $requested
}
$after = @(Snapshot) | ConvertTo-Json -Depth 4 -Compress
if ($script:calls -ne 2 -or $before -cne $after) { throw 'Native binder test did not complete unchanged' }
Write-Output 'Native NetSecurity binder passed: True/False via production WriteControl, WhatIf only; all profile snapshots unchanged'
