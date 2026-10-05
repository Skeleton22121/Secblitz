# Private host-only fixtures: import production functions/one dispatcher clause,
# never backend initialization. No Windows service or registry is touched.
param([string]$BackendPath = (Join-Path $PSScriptRoot 'backend.ps1'))
$ErrorActionPreference='Stop'
Set-StrictMode -Version 2
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($BackendPath,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($node in $ast.EndBlock.Statements) {
    if ($node -is [Management.Automation.Language.FunctionDefinitionAst]) { . ([scriptblock]::Create($node.Extent.Text)) }
}
$dispatcher=@($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.TryStatementAst] })[0].Body.Statements[0]
$gateClause=@($dispatcher.Clauses | Where-Object { $_.Item1.Value -ceq 'permission_gate' })
if ($gateClause.Count -ne 1) { throw 'Missing exact permission_gate dispatcher' }
$dispatchGate=[scriptblock]::Create($gateClause[0].Item2.Extent.Text.TrimStart('{').TrimEnd('}'))
$script:checks=0
function Assert($ok,$message) { if (!$ok) { throw $message }; $script:checks++ }
function Reject([scriptblock]$operation,$message) {
    $caught=$null
    try { & $operation } catch { $caught=$_.Exception.Message }
    Assert ($null -ne $caught -and $caught -like "*$message*") "Expected '$message', got '$caught'"
}
function Load($name) { Assert ($name -ceq 'CimCmdlets') 'Service gate loaded unrelated native security module' }
function QueryMdmRegistration {
    $script:mdmCalls++
    if ($script:mdmFailed) { throw 'MDM API unavailable' }
    return $script:mdm
}
function Test-Path {
    param($LiteralPath,$ErrorAction)
    if ($script:deniedPath -ceq $LiteralPath) { throw 'policy path access denied' }
    return ($LiteralPath -in $script:paths -or $LiteralPath -ceq 'HKLM:\SOFTWARE\Microsoft\PolicyManager\providers')
}
function Get-ChildItem {
    param($LiteralPath)
    if ($LiteralPath -ceq 'HKLM:\SOFTWARE\Microsoft\PolicyManager\providers') { return @{PSPath=$PSScriptRoot} }
    return @('actual-enrollment')
}
function PolicyValues($path) {
    $script:policyQueries+=@($path)
    if ($script:policyFailed) { throw 'policy read unavailable' }
    if ($script:policyMap.ContainsKey($path)) { return $script:policyMap[$path] }
    return @{}
}
function HasValues($path) { return $path -in $script:configuredPaths }
function Get-CimInstance {
    param($ClassName,$Namespace)
    $script:cimQueries+=@($ClassName)
    switch -CaseSensitive ($ClassName) {
        'Win32_OperatingSystem' { return $script:os }
        'Win32_ComputerSystem' { return @{PartOfDomain=$script:domain} }
        'RSOP_GPO' { if ($null -ne $script:rsopException) { throw $script:rsopException }; return $script:gpos }
        'RSOP_PolicySetting' { if ($null -ne $script:settingException) { throw $script:settingException }; return $script:settings }
        default { throw "Unexpected service gate probe: $ClassName" }
    }
}
function Reset {
    $script:os=@{ProductType=1;BuildNumber='26100'}; $script:domain=$false
    $script:mdm=@{result=[int]0;registered=[int]0}; $script:mdmFailed=$false; $script:mdmCalls=0
    $script:paths=@(); $script:deniedPath=''; $script:configuredPaths=@()
    $script:policyMap=@{}; $script:policyFailed=$false; $script:policyQueries=@(); $script:cimQueries=@()
    $script:gpos=@(@{id='LocalGPO';enabled=$true;accessDenied=$false;filterAllowed=$true})
    $script:settings=@(); $script:rsopException=$null; $script:settingException=$null
}
$env:SystemRoot='C:\Windows'
foreach ($id in @('permissions.service.bits','permissions.service.wuauserv')) {
    Reset
    $value=$null
    Assert ((& $dispatchGate) -ceq '{"ok":true}') 'Dispatcher did not acknowledge actual gate'
    Assert ($script:mdmCalls -eq 1 -and $script:cimQueries -contains 'RSOP_PolicySetting') 'Gate skipped native authority probes'
    Assert (@($script:policyQueries | Where-Object { $_ -like '*Firewall*' }).Count -eq 0) 'Service fell through to Firewall policy'
    PermissionGate $id $null | Out-Null
    Assert ($script:mdmCalls -eq 2) 'Gate cached prior success'
    $script:mdm.registered=[int]1
    Reject { & $dispatchGate } 'registered with MDM'
    $script:mdm.registered=[int]0; $script:mdmFailed=$true
    Reject { PermissionGate $id $null } 'MDM API unavailable'
    $script:mdmFailed=$false
    foreach ($probe in @(@{result=[int]1;registered=[int]0},@{result=[int]0;registered='0'},@{result=[int]0;registered=[int]2})) {
        $script:mdm=$probe
        Reject { PermissionGate $id $null } 'MDM registration state is unknown'
    }
    Reset; $script:domain=$true
    Reject { PermissionGate $id $null } 'Domain-managed'
    foreach ($domain in @($null,'False')) {
        $script:domain=$domain
        Reject { PermissionGate $id $null } 'membership is not readable'
    }
    Reset
    foreach ($os in @(@{ProductType=3;BuildNumber='26100'},@{ProductType=1;BuildNumber='9600'},@{ProductType=1;BuildNumber=$null})) {
        $script:os=$os
        Reject { PermissionGate $id $null } 'Unsupported Windows client'
    }
    Reset
    foreach ($path in @('HKLM:\SOFTWARE\Microsoft\Provisioning\OMADM\Accounts','HKLM:\SYSTEM\CurrentControlSet\Control\CloudDomainJoin\JoinInfo')) {
        $script:paths=@($path)
        Reject { PermissionGate $id $null } 'cloud-management evidence'
    }
    Reset
    foreach ($artifact in @('System32\GroupPolicy\Machine\Registry.pol','System32\GroupPolicy\gpt.ini','System32\GroupPolicy\Machine\Microsoft\Windows NT\SecEdit\GptTmpl.inf','System32\GroupPolicy\Machine\Preferences\Registry\Registry.xml','System32\GroupPolicy\Machine\Preferences\Services\Services.xml')) {
        $path=[IO.Path]::Combine($env:SystemRoot,$artifact)
        $script:paths=@($path)
        Reject { PermissionGate $id $null } 'policy artifacts'
        $script:paths=@(); $script:deniedPath=$path
        Reject { PermissionGate $id $null } 'policy path access denied'
        $script:deniedPath=''
    }
    Reset
    # Enumerating the documented base catches any derived system-service /
    # security setting, including a local setting and an unknown class instance.
    foreach ($setting in @(@{GPOID='LocalGPO';name='BITS';securityDescriptor='configured'},@{GPOID='LocalGPO';name='wuauserv';startupMode=3},@{GPOID='LocalGPO';name='Unrelated actual policy'},@{})) {
        $script:settings=@($setting)
        Reject { PermissionGate $id $null } 'Applied computer policy settings'
    }
    Reset
    foreach ($gpo in @(@{id='domain-gpo';enabled=$true;filterAllowed=$true;accessDenied=$false},@{id='domain-gpo';enabled=$false;filterAllowed=$false;accessDenied=$false},@{id='LocalGPO';enabled=$true;filterAllowed=$true;accessDenied=$true},@{id='';enabled=$true;filterAllowed=$true;accessDenied=$false},@{id='LocalGPO';enabled='True';filterAllowed=$true;accessDenied=$false})) {
        $script:gpos=@($gpo)
        Reject { PermissionGate $id $null } 'Group Policy'
    }
    Reset; $script:gpos=@()
    PermissionGate $id $null | Out-Null
    $script:settings=@(@{GPOID='orphaned';name='BITS'})
    Reject { PermissionGate $id $null } 'Applied computer policy settings'
    Reset
    $script:rsopException=[Runtime.InteropServices.COMException]::new('missing namespace',0x8004100E)
    PermissionGate $id $null | Out-Null
    Assert ($script:cimQueries -notcontains 'RSOP_PolicySetting') 'Missing namespace did not use established exception'
    $script:paths=@([IO.Path]::Combine($env:SystemRoot,'System32\GroupPolicy\Machine\Microsoft\Windows NT\SecEdit\GptTmpl.inf'))
    Reject { PermissionGate $id $null } 'policy artifacts'
    foreach ($code in @(0x80041003,0x80041010,0x80041004,0x80070005)) {
        Reset; $script:rsopException=[Runtime.InteropServices.COMException]::new('unknown authority',$code)
        Reject { PermissionGate $id $null } 'unknown authority'
        Reset; $script:settingException=[Runtime.InteropServices.COMException]::new('unknown setting authority',$code)
        Reject { PermissionGate $id $null } 'unknown setting authority'
    }
    # Namespace disappearing after a successful GPO read is NOT clean absence.
    Reset; $script:settingException=[Runtime.InteropServices.COMException]::new('namespace disappeared',0x8004100E)
    Reject { PermissionGate $id $null } 'namespace disappeared'
    Reset
    $areas=if ($id.EndsWith('.bits')) {@('SystemServices','ADMX_BITS')} else {@('SystemServices','Update','ADMX_WindowsUpdate')}
    foreach ($area in $areas) {
        foreach ($path in @("HKLM:\SOFTWARE\Microsoft\PolicyManager\current\device\$area", (Join-Path $PSScriptRoot "default\device\$area"))) {
            $script:policyMap=@{$path=@{SomeServicePolicy=@{kind='DWord';value=0}}}
            Reject { PermissionGate $id $null } 'Relevant policy'
        }
    }
    Reset; $script:policyFailed=$true
    Reject { PermissionGate $id $null } 'policy read unavailable'
    Reset
    $policyPath=if ($id.EndsWith('.bits')) {'HKLM:\SOFTWARE\Policies\Microsoft\Windows\BITS'} else {'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate'}
    $script:configuredPaths=@($policyPath)
    Reject { PermissionGate $id $null } 'Configured management/security policy'
    Reset; $script:configuredPaths=@('HKLM:\SOFTWARE\Policies\Microsoft\WindowsFirewall')
    PermissionGate $id $null | Out-Null
    foreach ($payload in @($true,1,'BITS',@{ok=$true})) {
        Reject { PermissionGate $id $payload } 'Invalid service permission gate request'
    }
    Reject { ReadControl $id } 'native wrapper'
    Reject { WriteControl $id @{present=$true;value=1} } 'native wrapper'
}
foreach ($id in @('permissions.service.BITS','Permissions.service.bits','permissions.service.spooler','permissions.service.bits;exit','permissions.service.bits ', 'uac.enabled','', 'BITS')) {
    Reset
    Reject { PermissionGate $id $null } 'permission'
    Assert ($script:mdmCalls -eq 0 -and $script:cimQueries.Count -eq 0) 'Invalid service ID reached native probes'
}
Write-Output "Permission gate fixtures passed: $script:checks checks"
