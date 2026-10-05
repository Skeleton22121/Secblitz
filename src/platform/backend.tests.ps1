# Non-mutating fixtures: execute only function definitions from the production
# AST. No module imports, registry access, or production dispatcher is executed.
# Compatible with Windows PowerShell 5.1 and PowerShell 7 (Linux).
param([string]$BackendPath = (Join-Path $PSScriptRoot 'backend.ps1'))
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($BackendPath, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($node in $ast.EndBlock.Statements) {
    if ($node -is [System.Management.Automation.Language.FunctionDefinitionAst]) {
        . ([scriptblock]::Create($node.Extent.Text))
    }
}
$productionGate = ${function:Gate}
$defenderNames = @{ 'defender.realtime'='DisableRealtimeMonitoring'; 'defender.behavior'='DisableBehaviorMonitoring'; 'defender.ioav'='DisableIOAVProtection'; 'defender.archive'='DisableArchiveScanning' }
$uacPath = 'unused-fixture-path'
$script:checks = 0
function Assert($ok, [string]$message) {
    if (!$ok) { throw $message }
    $script:checks++
}
function Reject([scriptblock]$operation, [string]$message) {
    $caught = $null
    try { & $operation } catch { $caught = $_.Exception.Message }
    Assert ($null -ne $caught -and $caught -like "*$message*") "Expected '$message', received '$caught'"
}
function Load($name) {}
function Start-Sleep($Milliseconds) {}
function Gate($id) { if ($script:blocked) { throw 'fixture management gate' } }
function ReadControl($id) { return $script:current }
function Set-MpPreference {
    param($DisableRealtimeMonitoring, $DisableBehaviorMonitoring, $DisableIOAVProtection, $DisableArchiveScanning)
    $script:writes++
    if (!$script:ignoreSetter) { $script:current = $script:requested }
}
function Get-MpComputerStatus { return $script:status }
function New-ItemProperty { param($LiteralPath,$Name,$PropertyType,$Value,[switch]$Force); $script:writes++; $script:current=@{present=$true;value=$Value} }
function Remove-ItemProperty { param($LiteralPath,$Name,$ErrorAction); $script:writes++; $script:current=@{present=$false;value=$null} }
function Reset {
    $script:writes=0; $script:blocked=$false; $script:ignoreSetter=$false
    $script:status=@{IsTamperProtected=$false;AMServiceEnabled=$true;AntivirusEnabled=$true;AMRunningMode='Normal';RealTimeProtectionEnabled=$true;BehaviorMonitorEnabled=$true;IoavProtectionEnabled=$true}
}
foreach ($id in @('uac.enabled','uac.consent')) {
    $target = if ($id -eq 'uac.enabled') { 1 } else { 5 }
    Reset; $script:current=@{present=$true;value=0}
    WriteControl $id @{present=$true;value=$target}
    Assert ($script:writes -eq 1 -and $script:current.value -eq $target) 'UAC repair failed'
    WriteControl $id @{present=$true;value=0}
    Assert ($script:writes -eq 2 -and $script:current.value -eq 0) 'UAC restore failed'
    foreach ($drift in @(@{present=$false;value=$null}, @{present=$true;value=0}, @{present=$true;value=2})) {
        Reset; $script:current=$drift
        Reject { WriteControl $id @{present=$true;value=0} } 'changed before restore'
        Assert ($script:writes -eq 0) 'UAC restore overwrote drift'
    }
    Reset; $script:current=@{present=$true;value=$target}
    WriteControl $id @{present=$false;value=$null}
    Assert (!$script:current.present -and $script:writes -eq 1) 'UAC absent before image was not restored'
    foreach ($preserved in @(@{present=$false;value=$null}, @{present=$true;value=$target})) {
        Reset; $script:current=$preserved
        Reject { WriteControl $id @{present=$true;value=$target} } 'explicitly disabled'
        Assert ($script:writes -eq 0) 'UAC repair overwrote preserved setting'
    }
    Reset; $script:blocked=$true; $script:current=@{present=$true;value=$target}
    Reject { WriteControl $id @{present=$true;value=0} } 'management gate'
    Assert ($script:writes -eq 0) 'UAC restore bypassed management gate'
}
foreach ($id in $defenderNames.Keys) {
    foreach ($requested in @($false,$true)) {
        Reset; $script:requested=$requested; $script:current=!$requested
        foreach ($name in @('RealTimeProtectionEnabled','BehaviorMonitorEnabled','IoavProtectionEnabled')) { $script:status[$name]=!$requested }
        WriteControl $id $requested
        Assert ($script:writes -eq 1 -and $script:current -eq $requested) 'Defender apply/restore failed'
        $script:current=!$requested; $script:ignoreSetter=$true
        Reject { WriteControl $id $requested } 'readback did not match'
    }
    Reset; $script:requested=$false; $script:current=$true; $script:status.IsTamperProtected=$true
    Reject { WriteControl $id $false } 'tamper state changed'
    Reset; $script:requested=$false; $script:current=$true; $script:status.AMRunningMode='Passive'
    Reject { WriteControl $id $false } 'became unavailable or passive'
    foreach ($field in @('AMServiceEnabled','AntivirusEnabled')) {
        foreach ($bad in @($null,'True','False',1)) {
            Reset; $script:requested=$false; $script:current=$true; $script:status[$field]=$bad
            Reject { WriteControl $id $false } 'became unavailable or passive'
        }
    }
    Reset; $script:blocked=$true
    Reject { WriteControl $id $false } 'management gate'
    Assert ($script:writes -eq 0) 'Defender bypassed management gate'
}
foreach ($pair in @(@('defender.realtime','RealTimeProtectionEnabled'),@('defender.behavior','BehaviorMonitorEnabled'),@('defender.ioav','IoavProtectionEnabled'))) {
    foreach ($bad in @($false, $null, 'True')) {
        Reset; $script:requested=$false; $script:current=$true; $script:status[$pair[1]]=$bad
        Reject { WriteControl $pair[0] $false } 'readback did not match'
    }
}
# Exercise the real gate independently, including fail-closed tamper handling.
function QueryMdmRegistration { if ($script:mdmFailed) { throw 'fixture MDM API unavailable' }; return $script:mdmProbe }
function Test-Path { param($LiteralPath); return ($LiteralPath -in @('HKLM:\SOFTWARE\Microsoft\Enrollments','HKLM:\SOFTWARE\Microsoft\PolicyManager\providers') -or $LiteralPath -eq $script:registryEvidence) }
function Get-ChildItem { param($LiteralPath); if ($LiteralPath -eq 'HKLM:\SOFTWARE\Microsoft\Enrollments') { return @(1..33) }; if ($LiteralPath -like '*PolicyManager\providers') { return @{PSPath=$PSScriptRoot} }; return @('active-id') }
function HasValues($path) { return ($script:policy -and $path -like '*Windows Defender') }
function PolicyValues($path) {
    if ($script:policyReadFailed) { throw 'fixture relevant policy access denied' }
    if ($script:policyMap.ContainsKey($path)) { return $script:policyMap[$path] }
    if ($script:providerArea -ne '' -and $path -like "*default*device*$script:providerArea") { return $script:providerData }
    return @{}
}
function Get-CimInstance {
    param($ClassName,$Namespace)
    switch ($ClassName) {
        'Win32_OperatingSystem' { return $script:os }
        'Win32_ComputerSystem' { return @{PartOfDomain=$script:domain} }
        'RSOP_GPO' { if ($script:rsopFailed) { throw 'fixture RSOP unavailable' }; if ($null -ne $script:rsopException) { throw $script:rsopException }; return $script:gpos }
        'RSOP_RegistryPolicySetting' { return $script:rsopSettings }
        'RSOP_RegistryValue' { return $script:rsopRegistryValues }
        'AntiVirusProduct' { return $script:providers }
        'FirewallProduct' { return }
        default { throw "Unexpected probe $ClassName" }
    }
}
function Get-MpPreference { return @{} }
$env:SystemRoot = 'C:\Windows'
Reset; $script:domain=$false; $script:policy=$false; $script:rsopFailed=$false
$script:policyMap=@{}; $script:providerArea=''; $script:providerData=@{}; $script:policyReadFailed=$false
$script:gpos=@(@{id='LocalGPO';enabled=$true;accessDenied=$false;filterAllowed=$true;version=0})
$script:rsopSettings=@(); $script:rsopRegistryValues=@(); $script:rsopException=$null
$script:mdmFailed=$false; $script:mdmProbe=@{result=[int]0;registered=[int]0}; $script:registryEvidence=''
$script:providers=@(@{instanceGuid='{D68DDC3A-831F-4fae-9E44-DA132C1ACF46}'})
foreach ($build in @('10240','19045','22631','26100')) {
    $script:os=@{ProductType=1;BuildNumber=$build}
    & $productionGate 'defender.realtime'
}
# Built-in enrollment templates do not establish active MDM. The API is
# authoritative for that check; actual OMADM accounts and Entra join evidence
# remain independent vetoes, even if the API reports no current MDM enrollment.
& $productionGate 'uac.enabled'
Assert $true 'Clean client with enrollment templates should pass'
foreach ($probe in @(@{result=[int]0;registered=[int]1},@{result=[int]-2147467259;registered=[int]0},@{result=[int]1;registered=[int]0},@{result=[int]0;registered=[int]2},@{result=[int]0;registered='0'},@{result=$null;registered=[int]0})) {
    $script:mdmProbe=$probe
    Reject { & $productionGate 'uac.enabled' } $(if ($probe.result -eq 0 -and $probe.registered -eq 1) {'registered with MDM'} else {'MDM registration state is unknown'})
}
$script:mdmProbe=@{result=[int]0;registered=[int]0}; $script:mdmFailed=$true
Reject { & $productionGate 'uac.enabled' } 'MDM API unavailable'
$script:mdmFailed=$false
foreach ($path in @('HKLM:\SOFTWARE\Microsoft\Provisioning\OMADM\Accounts','HKLM:\SYSTEM\CurrentControlSet\Control\CloudDomainJoin\JoinInfo')) {
    $script:registryEvidence=$path
    Reject { & $productionGate 'uac.enabled' } 'cloud-management evidence'
}
$script:registryEvidence=''
foreach ($tamper in @($true,$null,'False')) {
    $script:status.IsTamperProtected=$tamper
    Reject { & $productionGate 'defender.realtime' } $(if ($tamper -eq $true) {'tamper protected'} else {'state is unknown'})
}
foreach ($field in @('AMServiceEnabled','AntivirusEnabled')) {
    foreach ($bad in @($null,'True','False',1)) {
        Reset; $script:status[$field]=$bad
        Reject { & $productionGate 'defender.realtime' } 'unavailable, passive, or tamper protected'
    }
}
Reset; $script:domain=$true
Reject { & $productionGate 'uac.enabled' } 'Domain-managed'
$script:domain=$null
Reject { & $productionGate 'uac.enabled' } 'membership is not readable'
$script:domain=$false
foreach ($os in @(@{ProductType=3;BuildNumber='26100'},@{ProductType=1;BuildNumber='9600'},@{ProductType=1;BuildNumber=$null})) {
    $script:os=$os
    Reject { & $productionGate 'uac.enabled' } 'Unsupported Windows client'
}
$script:os=@{ProductType=1;BuildNumber='26100'}
$script:policy=$true
Reject { & $productionGate 'defender.realtime' } 'Configured management/security policy'
& $productionGate 'uac.enabled'
Assert $true 'Unrelated Defender policy must not veto UAC'
$script:policy=$false; $script:rsopFailed=$true
Reject { & $productionGate 'uac.enabled' } 'RSOP unavailable'
$script:rsopFailed=$false
$script:providers=@()
Reject { & $productionGate 'defender.realtime' } 'registration cannot be established'
$script:providers=@(@{instanceGuid='third-party'},@{instanceGuid='{D68DDC3A-831F-4fae-9E44-DA132C1ACF46}'})
Reject { & $productionGate 'defender.realtime' } 'unrecognized security provider'
foreach ($probe in @({ 'partial output'; throw 'provider failed' }, { 'unexpected provider output'; @{title='fixture';status='ok';detail='valid'} }, { @{title='fixture';status='invalid';detail='bad'} })) {
    $findings = @(Finding 'fixture' $probe)
    Assert ($findings.Count -eq 1 -and $findings[0].title -eq 'fixture' -and $findings[0].status -eq 'unknown') 'A failed/malformed probe contaminated the findings array'
}
$findings = @(Finding 'fixture' { @{title='fixture';status='ok';detail='valid'} })
Assert ($findings.Count -eq 1 -and $findings[0].status -eq 'ok') 'Valid finding was rejected'
# Policy authority, not provider-container presence/default values.
$script:providerArea='knobs'; $script:providerData=@{'Power/Controls/EnergyEstimationEnabled'=@{kind='DWord';value=1}}
& $productionGate 'uac.enabled'
Assert $true 'Inbox power provider incorrectly classified as UAC management'
$uacArea='HKLM:\SOFTWARE\Microsoft\PolicyManager\current\device\LocalPoliciesSecurityOptions'
$policyName='UserAccountControl_RunAllAdministratorsInAdminApprovalMode'
$script:policyMap[$uacArea]=@{$policyName=@{kind='DWord';value=1};($policyName+'_ProviderSet')=@{kind='DWord';value=[int]0}}
& $productionGate 'uac.enabled'
Assert $true 'Explicitly inactive current metadata rejected'
foreach ($flag in @([int]1,'0',[int]2)) {
    $script:policyMap[$uacArea][$policyName+'_ProviderSet'].value=$flag
    Reject { & $productionGate 'uac.enabled' } 'Relevant policy'
}
$script:policyMap[$uacArea][$policyName+'_ProviderSet'].value=[int]0
$script:policyMap[$uacArea][$policyName+'_WinningProvider']=@{kind='String';value='enforcing-provider'}
Reject { & $productionGate 'uac.enabled' } 'Relevant policy'
$script:policyMap[$uacArea]=@{$policyName=@{kind='DWord';value=1}}
Reject { & $productionGate 'uac.enabled' } 'Relevant policy'
$script:policyMap=@{}; $script:providerArea='LocalPoliciesSecurityOptions'; $script:providerData=@{$policyName=@{kind='DWord';value=1}}
Reject { & $productionGate 'uac.enabled' } 'Relevant policy'
$script:providerArea=''
$script:policyReadFailed=$true
Reject { & $productionGate 'uac.enabled' } 'policy access denied'
$script:policyReadFailed=$false
# Exact absence is accepted; access/provider/class failures are not absence.
$script:rsopException=[Runtime.InteropServices.COMException]::new('fixture missing namespace',0x8004100E)
& $productionGate 'uac.enabled'
Assert $true 'Absent RSOP namespace rejected on otherwise unconfigured machine'
$script:registryEvidence=[IO.Path]::Combine($env:SystemRoot,'System32\GroupPolicy\Machine\Registry.pol')
Reject { & $productionGate 'uac.enabled' } 'Local computer policy artifacts'
$script:registryEvidence=''
foreach ($code in @(0x80041003,0x80041010,0x80041004,0x80070005)) {
    $script:rsopException=[Runtime.InteropServices.COMException]::new('fixture unknown authority',$code)
    Reject { & $productionGate 'uac.enabled' } 'unknown authority'
}
$script:rsopException=$null
$script:rsopSettings=@(@{registryKey='Software\Policies\Microsoft\Windows Defender';valueName='DisableRealtimeMonitoring'})
& $productionGate 'uac.enabled'
Assert $true 'Unrelated Defender RSoP must not veto UAC'
$script:rsopRegistryValues=@(@{Path='MACHINE\Software\Microsoft\Windows\CurrentVersion\Policies\System\EnableLUA'})
Reject { & $productionGate 'uac.enabled' } 'Relevant resultant Group Policy'
$script:rsopRegistryValues=@(); $script:rsopSettings=@()
$script:gpos[0].accessDenied=$true
Reject { & $productionGate 'uac.enabled' } 'authority is unknown'
$script:gpos[0].accessDenied=$false; $script:gpos[0].id='nonlocal-gpo'
Reject { & $productionGate 'uac.enabled' } 'Applied computer Group Policy'
$script:gpos[0].id='LocalGPO'
# Native firewall RSOP empty-store enumeration and effective readback.
function Get-Service($name) { return @{Status='Running'} }
function Get-NetFirewallProfile {
    param($PolicyStore,$Name)
    if ($PolicyStore -eq 'RSOP') {
        Assert ($null -eq $Name) 'RSOP used a -Name filter that misclassifies an empty store'
        if ($script:firewallRsopFailed) { throw 'fixture firewall policy access denied' }
        return $script:firewallRsop
    }
    return @{Name='Public';Enabled=$script:firewallEnabled;DefaultInboundAction=$script:firewallInbound;DefaultOutboundAction='Allow'}
}
$script:firewallRsop=@(); $script:firewallRsopFailed=$false; $script:firewallEnabled='True'; $script:firewallInbound='Block'
& $productionGate 'firewall.public.enabled'
$script:policy=$true
& $productionGate 'firewall.public.enabled'
Assert $true 'Unrelated Defender policy must not veto firewall'
$script:policy=$false; $script:firewallRsopFailed=$true
Reject { & $productionGate 'firewall.public.enabled' } 'policy access denied'
$script:firewallRsopFailed=$false; $script:firewallRsop=@(@{Name='Public';Enabled='False';DefaultInboundAction='NotConfigured';DefaultOutboundAction='NotConfigured'})
Reject { & $productionGate 'firewall.public.enabled' } 'resultant Group Policy'
$script:firewallRsop=@()
function Set-NetFirewallProfile {
    param($PolicyStore,$Name,$Enabled,$DefaultInboundAction)
    if ($PSBoundParameters.ContainsKey('Enabled') -and ($Enabled -isnot [string] -or $Enabled -cnotin @('True','False'))) { throw 'Enabled must be a GpoBoolean enum-name token' }
    $script:writes++
    if (!$script:ignoreSetter) { $script:current=$script:requested }
}
# Setter/readback unit fixtures isolate preflight; real fresh preflight and
# zero-write vetoes are exercised in backend.firewall.tests.ps1.
function ObserveControl($id) { return @{eligible=$true} }
foreach ($request in @(@{id='firewall.public.enabled';value=$true;effective='True'},@{id='firewall.public.enabled';value=$false;effective='False'},@{id='firewall.public.inbound';value='Block';effective='Block'},@{id='firewall.public.inbound';value='Allow';effective='Allow'},@{id='firewall.public.inbound';value='NotConfigured';effective='Block'},@{id='firewall.public.inbound';value='NotConfigured';effective='Allow'})) {
    Reset; $script:requested=$request.value; $script:current='before'
    if ($request.id.EndsWith('enabled')) { $script:firewallEnabled=$request.effective } else { $script:firewallInbound=$request.effective }
    WriteControl $request.id $request.value
    Assert ($script:writes -eq 1 -and $script:current -eq $request.value) 'Firewall write/restore failed'
    $script:ignoreSetter=$true; $script:current='before'
    Reject { WriteControl $request.id $request.value } 'effective readback did not match'
}
Reset; $script:requested='Block'; $script:firewallInbound='Allow'
Reject { WriteControl 'firewall.public.inbound' 'Block' } 'effective readback did not match'
Reset; $script:requested='NotConfigured'; $script:firewallInbound='NotConfigured'
Reject { WriteControl 'firewall.public.inbound' 'NotConfigured' } 'effective readback did not match'
# The expanded controls use exactly the same native gate, including restores.
$script:policyMap=@{}; $script:providerArea=''; $script:registryEvidence=''
foreach ($id in @('installer.always_install_elevated','lsa.restrict_anonymous_sam','lsa.limit_blank_password_use','wdigest.use_logon_credential')) {
    $spec=PrivilegeRegistrySpec $id
    & $productionGate $id
    $script:mdmFailed=$true
    Reject { & $productionGate $id } 'MDM API unavailable'
    $script:mdmFailed=$false; $script:domain=$true
    Reject { & $productionGate $id } 'Domain-managed'
    $script:domain=$false; $script:rsopFailed=$true
    Reject { & $productionGate $id } 'RSOP unavailable'
    $script:rsopFailed=$false
    foreach ($artifact in @('System32\GroupPolicy\Machine\Registry.pol','System32\GroupPolicy\gpt.ini','System32\GroupPolicy\Machine\Microsoft\Windows NT\SecEdit\GptTmpl.inf','System32\GroupPolicy\Machine\Preferences\Registry\Registry.xml')) {
        $script:registryEvidence=[IO.Path]::Combine($env:SystemRoot,$artifact)
        Reject { & $productionGate $id } 'Local computer policy artifacts'
    }
    $script:registryEvidence=''
    $script:rsopSettings=@(@{registryKey=$spec.path.Substring(6);valueName=$spec.name})
    Reject { & $productionGate $id } 'Relevant resultant Group Policy'
    $script:rsopSettings=@(@{registryKey=$spec.path.Substring(6);valueName='UnrelatedValue'})
    & $productionGate $id
    $script:rsopSettings=@(); $script:rsopRegistryValues=@(@{Path=('MACHINE\'+$spec.path.Substring(6)+'\'+$spec.name)})
    Reject { & $productionGate $id } 'Relevant resultant Group Policy'
    $script:rsopRegistryValues=@()
    $policyName=switch ($id) {
        'installer.always_install_elevated' {'AlwaysInstallElevated'}
        'lsa.restrict_anonymous_sam' {'NetworkAccess_DoNotAllowAnonymousEnumerationOfSAMAccounts'}
        'lsa.limit_blank_password_use' {'Accounts_LimitLocalAccountUseOfBlankPasswordsToConsoleLogonOnly'}
        'wdigest.use_logon_credential' {'WDigestAuthentication'}
    }
    $area='HKLM:\SOFTWARE\Microsoft\PolicyManager\current\device\'+$spec.areas[0]
    $script:policyMap[$area]=@{$policyName=@{kind='DWord';value=0}}
    Reject { & $productionGate $id } 'Relevant policy'
    $script:policyMap=@{}; $script:providerArea=$spec.areas[0]; $script:providerData=@{$policyName=@{kind='DWord';value=0}}
    Reject { & $productionGate $id } 'Relevant policy'
    $script:providerArea=''; $script:policyReadFailed=$true
    Reject { & $productionGate $id } 'policy access denied'
    $script:policyReadFailed=$false
}
& (Join-Path $PSScriptRoot 'backend.privilege.tests.ps1') -BackendPath $BackendPath
& (Join-Path $PSScriptRoot 'backend.permission.tests.ps1') -BackendPath $BackendPath
& (Join-Path $PSScriptRoot 'backend.firewall.tests.ps1') -BackendPath $BackendPath
& (Join-Path $PSScriptRoot 'backend.hardening.tests.ps1') -BackendPath $BackendPath
Write-Output "Platform PowerShell fixtures passed: $script:checks checks"
