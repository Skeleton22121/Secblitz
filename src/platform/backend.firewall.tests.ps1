# Host-only fixtures: real production functions and observe dispatcher, mocked
# Windows probes. No module bootstrap, machine changes, or network access.
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
$clause=@($dispatcher.Clauses | Where-Object { $_.Item1.Value -ceq 'observe' })
if ($clause.Count -ne 1) { throw 'Missing observe dispatcher' }
$dispatch=[scriptblock]::Create($clause[0].Item2.Extent.Text.TrimStart('{').TrimEnd('}'))
$defenderNames=@{}
$env:SystemRoot='C:\Windows'
$script:checks=0
$script:insecureSettingChanges=0
$productionGate=${function:Gate}
function Assert($condition,$message) { if (!$condition) { throw $message }; $script:checks++ }
& {
    $moduleRoot=$PSScriptRoot
    function Import-Module { param($Name,$ErrorAction); return @{unexpected='module output'} }
    Assert (@(Load 'fixture').Count -eq 0) 'Module output leaked into JSON'
}
function Load($name) { if ($name -notin @('CimCmdlets','NetSecurity')) { throw 'Unexpected module' } }
function Set-NetFirewallProfile { $script:insecureSettingChanges++; throw 'Observation called setter' }
function Set-MpPreference { $script:insecureSettingChanges++; throw 'Observation called setter' }
function New-ItemProperty { $script:insecureSettingChanges++; throw 'Observation wrote registry' }
function Remove-ItemProperty { $script:insecureSettingChanges++; throw 'Observation removed registry value' }
function QueryMdmRegistration {
    if ($script:mdmFailure) { throw 'Device is registered with MDM: assessment only' }
    return @{result=[int]0;registered=[int]$script:mdm}
}
function Test-Path { param($LiteralPath,$ErrorAction); return $LiteralPath -in $script:paths }
function Get-ChildItem { param($LiteralPath); return @('configured-account') }
function PolicyValues($path) {
    if ($script:policyFailure) { throw 'Configured management/security policy: assessment only' }
    if ($path -like '*current\device\Firewall') { return $script:currentPolicy }
    return @{}
}
function HasValues($path) { return $script:configuredPolicy -and $path -like '*WindowsFirewall' }
function Get-CimInstance {
    param($ClassName,$Namespace)
    switch ($ClassName) {
        'Win32_OperatingSystem' { return @{ProductType=1;BuildNumber='26100'} }
        'Win32_ComputerSystem' { return @{PartOfDomain=$script:domain} }
        'RSOP_GPO' { return $script:gpos }
        'RSOP_RegistryPolicySetting' { return $script:registryPolicy }
        'RSOP_RegistryValue' { return @() }
        'FirewallProduct' { return $script:providers }
        default { throw "Unexpected CIM probe: $ClassName" }
    }
}
function Get-Service($name) {
    if ($name -notin @('BFE','MpsSvc')) { throw 'Unexpected service' }
    if ($name -eq $script:failedService) { throw 'Service access denied' }
    return @{Status=$(if ($name -eq $script:stoppedService) {'Stopped'} else {'Running'})}
}
function Get-NetFirewallProfile {
    param($PolicyStore,$Name)
    $script:queries+=@($PolicyStore)
    if ($PolicyStore -eq 'RSOP') { return $script:rsop }
    if ($PolicyStore -eq 'PersistentStore') {
        return @{Name=$Name;Enabled=$script:rawEnabled;DefaultInboundAction=$script:rawInbound}
    }
    if ($PolicyStore -ne 'ActiveStore') { throw 'Unexpected firewall store' }
    if ($script:activeFailure) { throw 'ActiveStore unavailable' }
    $profile=@{Name=$(if ($script:wrongProfile) {'Other'} else {$Name});Enabled=$script:activeEnabled;DefaultInboundAction=$script:activeInbound}
    for ($i=0; $i -lt $script:activeCount; $i++) { $profile }
}
function Reset {
    $script:rawInbound='NotConfigured'; $script:rawEnabled='True'
    $script:activeInbound='Block'; $script:activeEnabled='True'
    $script:activeFailure=$false; $script:wrongProfile=$false; $script:activeCount=1
    $script:failedService=''; $script:stoppedService=''
    $script:domain=$false; $script:mdm=0; $script:mdmFailure=$false
    $script:paths=@(); $script:currentPolicy=@{}; $script:configuredPolicy=$false; $script:policyFailure=$false
    $script:gpos=@(); $script:registryPolicy=@(); $script:providers=@(); $script:rsop=@(); $script:queries=@()
}
function Observe([string]$id) {
    $output=@(& $dispatch)
    Assert ($output.Count -eq 1 -and $output[0] -is [string]) 'Observe must emit exactly one JSON string'
    $obs=$output[0] | ConvertFrom-Json
    Assert ($obs.eligible -is [bool] -and $obs.reason -is [string]) 'Untyped observation'
    Assert ($script:queries[0] -ceq 'PersistentStore') 'Raw before-image was not read first'
    Assert (@($obs.PSObject.Properties).Count -eq 5) 'Unexpected firewall observation fields'
    Assert ($obs.authority -cin @('local','managed','unknown')) 'Unsupported authority shape'
    if ($null -ne $obs.effective) {
        Assert (@($obs.effective.PSObject.Properties).Count -eq 2) 'Unexpected effective metadata fields'
        if ($id.EndsWith('.enabled')) {
            Assert ($obs.effective.kind -ceq 'enabled' -and $obs.effective.value -is [bool]) 'Wrong enabled metadata variant'
        } else {
            Assert ($obs.effective.kind -ceq 'inbound' -and $obs.effective.value -cin @('block','allow')) 'Wrong inbound metadata variant'
        }
    }
    Assert ($obs.authority -ceq 'local' -or !$obs.eligible) 'Nonlocal metadata granted eligibility'
    return $obs
}
foreach ($profile in @('domain','private','public')) {
    Reset
    $obs=Observe "firewall.$profile.inbound"
    Assert ($obs.value -ceq 'NotConfigured' -and $obs.effective.value -ceq 'block' -and $obs.authority -ceq 'local' -and $obs.eligible) 'Safe inherited Block was changed or lost eligibility'
    Assert ($script:insecureSettingChanges -eq 0) 'Default firewall observation changed a setting'
    $script:rawInbound='Allow'; $script:activeInbound='Allow'
    $obs=Observe "firewall.$profile.inbound"
    Assert ($obs.value -ceq 'Allow' -and $obs.effective.value -ceq 'allow' -and $obs.eligible) 'Explicit local Allow was not repairable'
    $script:rawInbound='NotConfigured'
    $obs=Observe "firewall.$profile.inbound"
    Assert ($obs.value -ceq 'NotConfigured' -and $obs.effective.value -ceq 'allow' -and $obs.eligible) 'Inherited Allow was not repairable'
    foreach ($enabled in @('True','False')) {
        $script:rawEnabled=$enabled; $script:activeEnabled=$enabled
        $obs=Observe "firewall.$profile.enabled"
        Assert ($obs.value -is [bool] -and $obs.value -eq ($enabled -ceq 'True') -and $obs.eligible) 'Enabled before-image or effective boolean changed'
    }
}
foreach ($pair in @(@('Block','Allow'),@('Allow','Block'))) {
    Reset; $script:rawInbound=$pair[0]; $script:activeInbound=$pair[1]
    $obs=Observe 'firewall.public.inbound'
    Assert (!$obs.eligible -and $obs.authority -ceq 'unknown' -and $obs.reason -ceq 'EffectiveFirewallMismatch' -and $obs.value -ceq $pair[0]) 'Contradictory inbound evidence allowed protection/repair'
}
foreach ($pair in @(@('True','False'),@('False','True'))) {
    Reset; $script:rawEnabled=$pair[0]; $script:activeEnabled=$pair[1]
    $obs=Observe 'firewall.public.enabled'
    Assert (!$obs.eligible -and $obs.authority -ceq 'unknown' -and $obs.reason -ceq 'EffectiveFirewallMismatch') 'Contradictory enabled evidence allowed protection/repair'
}
foreach ($case in @('missing','duplicate','wrong-profile','not-configured','bad-enabled','provider-failure','BFE-stopped','MpsSvc-stopped','service-failure')) {
    Reset
    switch ($case) {
        missing { $script:activeCount=0 }
        duplicate { $script:activeCount=2 }
        wrong-profile { $script:wrongProfile=$true }
        not-configured { $script:activeInbound='NotConfigured' }
        bad-enabled { $script:activeEnabled=1 }
        provider-failure { $script:activeFailure=$true }
        BFE-stopped { $script:stoppedService='BFE' }
        MpsSvc-stopped { $script:stoppedService='MpsSvc' }
        service-failure { $script:failedService='MpsSvc' }
    }
    $obs=Observe 'firewall.public.inbound'
    Assert (!$obs.eligible -and $obs.authority -ceq 'unknown' -and $null -eq $obs.effective -and $obs.value -ceq 'NotConfigured') "Unknown evidence accepted: $case"
}
foreach ($case in @('domain','mdm','cloud','configured-policy','active-provider','gpo','registry-policy','firewall-rsop')) {
    Reset; $script:rawInbound='Allow'
    switch ($case) {
        domain { $script:domain=$true }
        mdm { $script:mdm=1 }
        cloud { $script:paths=@('HKLM:\SYSTEM\CurrentControlSet\Control\CloudDomainJoin\JoinInfo') }
        configured-policy { $script:configuredPolicy=$true }
        active-provider { $script:currentPolicy=@{EnableFirewall=@{kind='DWord';value=1};EnableFirewall_ProviderSet=@{kind='DWord';value=[int]1}} }
        gpo { $script:gpos=@(@{id='nonlocal';enabled=$true;accessDenied=$false;filterAllowed=$true}) }
        registry-policy { $script:registryPolicy=@(@{registryKey='Software\Policies\Microsoft\WindowsFirewall';valueName='EnableFirewall'}) }
        firewall-rsop { $script:rsop=@(@{Name='Public';Enabled='True';DefaultInboundAction='NotConfigured';DefaultOutboundAction='NotConfigured'}) }
    }
    $obs=Observe 'firewall.public.inbound'
    Assert (!$obs.eligible -and $obs.authority -ceq 'managed' -and $obs.effective.value -ceq 'block' -and $obs.value -ceq 'Allow') "Known management authority lost: $case"
}
foreach ($case in @('fake-managed-message','fake-policy-message','missing-metadata','malformed-rsop','unknown-domain','foreign-provider','policy-artifact')) {
    Reset
    switch ($case) {
        fake-managed-message { $script:mdmFailure=$true }
        fake-policy-message { $script:policyFailure=$true }
        missing-metadata { $script:currentPolicy=@{EnableFirewall=@{kind='DWord';value=1}} }
        malformed-rsop { $script:rsop=@(@{Name='Public';Enabled='Unrecognized';DefaultInboundAction='NotConfigured';DefaultOutboundAction='NotConfigured'}) }
        unknown-domain { $script:domain='False' }
        foreign-provider { $script:providers=@(@{instanceGuid='foreign'}) }
        policy-artifact { $script:paths=@([IO.Path]::Combine($env:SystemRoot,'System32\GroupPolicy\gpt.ini')) }
    }
    $obs=Observe 'firewall.public.inbound'
    Assert (!$obs.eligible -and $obs.authority -ceq 'unknown') "Ambiguous evidence classified as managed/local: $case"
}
# Preserve typed authority through native invocation and ErrorRecord wrappers.
foreach ($wrapper in @('inner','record','nested','plain')) {
    Reset
    $script:wrapper=$wrapper
    function Gate($id) {
        try { ThrowGate 'Localized management evidence' } catch {
            $caughtRecord=$_
            switch ($script:wrapper) {
                inner { throw [Reflection.TargetInvocationException]::new($caughtRecord.Exception) }
                record { throw [Management.Automation.RuntimeException]::new('wrapper', [Exception]::new('untagged inner'), $caughtRecord) }
                nested { throw [InvalidOperationException]::new('outer', [Reflection.TargetInvocationException]::new($caughtRecord.Exception)) }
                plain { throw [UnauthorizedAccessException]::new('Localized management evidence') }
            }
        }
    }
    $obs=Observe 'firewall.public.inbound'
    $expected=if ($wrapper -ceq 'plain') {'unknown'} else {'managed'}
    Assert (!$obs.eligible -and $obs.authority -ceq $expected) "Wrapped authority lost: $wrapper ($($obs.authority): $($obs.reason))"
}
${function:Gate}=$productionGate
foreach ($case in @('missing','duplicate','wrong-profile','not-configured','mismatch','managed','service','late-managed','late-mismatch','late-effective','bad-bool')) {
    Reset
    $obs=Observe 'firewall.public.inbound'
    Assert $obs.eligible 'Initial snapshot must be eligible'
    switch ($case) {
        missing { $script:activeCount=0 }
        duplicate { $script:activeCount=2 }
        wrong-profile { $script:wrongProfile=$true }
        not-configured { $script:activeInbound='NotConfigured' }
        mismatch { $script:rawInbound='Allow' }
        managed { $script:domain=$true }
        service { $script:stoppedService='MpsSvc' }
        late-managed {
            function Gate($id) { & $productionGate $id; $script:domain=$true }
        }
        late-mismatch {
            function Gate($id) { & $productionGate $id; $script:rawInbound='Allow' }
        }
        late-effective {
            $script:rawInbound='Block'
            $script:gateCalls=0
            function Gate($id) {
                & $productionGate $id
                $script:gateCalls++
                if ($script:gateCalls -eq 2) { $script:activeInbound='Allow' }
            }
        }
    }
    $caught=$false
    try {
        if ($case -ceq 'bad-bool') { WriteControl 'firewall.public.enabled' 'False' }
        else { WriteControl 'firewall.public.inbound' 'Block' }
    } catch { $caught=$true }
    Assert $caught "Unsafe write accepted: $case"
    Assert ($script:insecureSettingChanges -eq 0) "Setter reached: $case"
    ${function:Gate}=$productionGate
}
Assert ($script:insecureSettingChanges -eq 0) 'Veto fixtures called a setter'
# Successful writes exercise the real preflight too, with an entirely mocked
# setter that updates the fixture stores. No live Windows command is called.
$script:mockWrites=0
function Set-NetFirewallProfile {
    param($PolicyStore,$Name,$Enabled,$DefaultInboundAction)
    Assert ($PolicyStore -ceq 'PersistentStore' -and $Name -ceq 'public') 'Wrong write destination'
    $script:mockWrites++
    if ($PSBoundParameters.ContainsKey('Enabled')) {
        Assert ($Enabled -is [string] -and $Enabled -cin @('True','False')) 'Wrong enum binder token'
        $script:rawEnabled=$Enabled; $script:activeEnabled=$Enabled
    } else {
        $script:rawInbound=$DefaultInboundAction
        $script:activeInbound=if ($DefaultInboundAction -ceq 'NotConfigured') {'Block'} else {$DefaultInboundAction}
    }
    return @{unexpected='setter output'}
}
foreach ($requested in @('Block','Allow','NotConfigured')) {
    Reset
    Assert (@(WriteControl 'firewall.public.inbound' $requested).Count -eq 0) 'Write leaked pipeline output'
    Assert ($script:rawInbound -ceq $requested) 'Raw inbound restoration was changed'
}
foreach ($requested in @($false,$true)) {
    Reset
    Assert (@(WriteControl 'firewall.public.enabled' $requested).Count -eq 0) 'Enabled write leaked pipeline output'
    Assert (($script:rawEnabled -ceq 'True') -eq $requested) 'Enabled restoration was changed'
}
Assert ($script:mockWrites -eq 5) 'Mock apply/restore did not reach setter exactly once'
Reset
function ReadControl($id) { return @{present=$false;value=$null} }
$nonFirewall=ObserveControl 'uac.enabled'
Assert (!$nonFirewall.ContainsKey('effective') -and !$nonFirewall.ContainsKey('authority') -and !$nonFirewall.eligible) 'Non-firewall metadata or eligibility changed'
Assert ($script:insecureSettingChanges -eq 0) 'Read-only firewall fixtures performed insecure setting changes'
Write-Output "Firewall observation fixtures passed: $script:checks checks; insecureSettingChanges=$script:insecureSettingChanges"
