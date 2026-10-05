# Non-mutating fixtures for the extended hardening controls (hardening.ps1).
# Only function definitions from the production files are executed; every
# registry, Defender, firewall, account and Wi-Fi touchpoint is an in-memory
# double. Compatible with Windows PowerShell 5.1 and PowerShell 7.
# Optional: set SECBLITZ_PARITY to the file written by the Rust test
# `export_rule_parity_fixture_for_powershell` (SECBLITZ_PARITY_OUT) to prove the
# PowerShell rules equal the compiled catalog for every spec and candidate value.
param(
    [string]$BackendPath = (Join-Path $PSScriptRoot 'backend.ps1'),
    [string]$HardeningPath = (Join-Path $PSScriptRoot 'hardening.ps1')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
foreach ($path in @($BackendPath, $HardeningPath)) {
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
    foreach ($node in $ast.EndBlock.Statements) {
        if ($node -is [Management.Automation.Language.FunctionDefinitionAst]) { . ([scriptblock]::Create($node.Extent.Text)) }
    }
}
$script:checks = 0
# The script Rust actually runs is the backend definitions plus hardening.ps1:
# it must parse as one unit and must not redefine (shadow) any backend function.
$backendText = [IO.File]::ReadAllText($BackendPath)
$delimiter = "`ntry {`n    switch -CaseSensitive (`$action) {"
$cut = $backendText.IndexOf($delimiter)
if ($cut -lt 0) { throw 'Backend dispatcher boundary changed' }
$combined = $backendText.Substring(0, $cut) + "`n" + [IO.File]::ReadAllText($HardeningPath)
$tokens = $null; $errors = $null
$combinedAst = [Management.Automation.Language.Parser]::ParseInput($combined, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
$names = @($combinedAst.FindAll({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] }, $false) | ForEach-Object { $_.Name.ToLowerInvariant() })
if (@($names | Group-Object | Where-Object { $_.Count -gt 1 }).Count -gt 0) { throw 'Duplicate function definition across backend.ps1 and hardening.ps1' }
$realHRead = ${function:HRead}
function Assert($ok, [string]$message) { if (!$ok) { throw $message }; $script:checks++ }
function Reject([scriptblock]$operation, [string]$message) {
    $caught = $null
    try { & $operation } catch { $caught = $_.Exception.Message }
    Assert ($null -ne $caught -and $caught -like "*$message*") "Expected '$message', received '$caught'"
}
function Load($name) {}
function Start-Sleep($Milliseconds) {}
$hMaps = @('Disabled','Basic','Advanced')
$hPua = @('Disabled','Enabled','AuditMode')
function MakeSpec([string]$json) { $script:spec = ConvertFrom-Json -InputObject $json }

# ---- rule parity with the compiled catalog (when the fixture is available)
if ($env:SECBLITZ_PARITY -and (Test-Path -LiteralPath $env:SECBLITZ_PARITY)) {
    $entries = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($env:SECBLITZ_PARITY))
    Assert (@($entries).Count -ge 20) 'Parity fixture is incomplete'
    foreach ($entry in @($entries)) {
        $script:spec = $entry.spec
        foreach ($case in @($entry.cases)) {
            $def = HDef $case.key
            Assert ((HIsSafe $def $case.value) -eq [bool]$case.safe) "Parity: safe($($entry.id) $($case.key)=$($case.value))"
            $fix = HFixOf $def $case.value
            Assert (HEq $fix $case.fix) "Parity: fix($($entry.id) $($case.key)=$($case.value)) got '$fix' expected '$($case.fix)'"
        }
    }
}

$pplJson = '{"id":"lsa.run_as_ppl","source":"Registry","dynamic":false,"reboot":true,"keys":[{"name":"RunAsPPL","path":"HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Lsa","rule":"set","safe":[1,2],"absentSafe":false,"fix":2,"max":2}],"gate":{"areas":[],"pattern":".","tamperExempt":false,"secedit":false,"ownPolicyKey":"","policyValues":[]}}'
$pnpJson = '{"id":"printer.point_and_print","source":"Registry","dynamic":false,"reboot":false,"keys":[{"name":"RestrictDriverInstallationToAdministrators","path":"HKLM:\\P","rule":"set","safe":[1],"absentSafe":true,"fix":null,"max":1},{"name":"NoWarningNoElevationOnInstall","path":"HKLM:\\P","rule":"set","safe":[0],"absentSafe":true,"fix":null,"max":1},{"name":"UpdatePromptSettings","path":"HKLM:\\P","rule":"set","safe":[0,1],"absentSafe":true,"fix":null,"max":2}],"gate":{"areas":[],"pattern":".","tamperExempt":false,"secedit":false,"ownPolicyKey":"","policyValues":[]}}'
$fwJson = '{"id":"net.public_sharing_exposure","source":"FirewallExposure","dynamic":true,"reboot":false,"keys":[{"name":"*","path":"","rule":"exposure","safe":[],"absentSafe":false,"fix":null,"max":15}],"gate":{"areas":[],"pattern":".","tamperExempt":false,"secedit":false,"ownPolicyKey":"","policyValues":[]}}'
$wifiJson = '{"id":"wifi.risky_profiles","source":"WifiProfiles","dynamic":true,"reboot":false,"keys":[{"name":"*","path":"","rule":"set","safe":[0],"absentSafe":false,"fix":0,"max":1}],"gate":{"areas":[],"pattern":".","tamperExempt":false,"secedit":false,"ownPolicyKey":"","policyValues":[]}}'
$asrJson = '{"id":"defender.asr.standard","source":"DefenderAsr","dynamic":false,"reboot":false,"keys":[{"name":"56a863a9-875e-4185-98a7-b882c64b5ce5","path":"","rule":"set","safe":[1,6],"absentSafe":false,"fix":1,"max":6},{"name":"9e6c4e1f-7d60-472f-ba1a-a39ef669e4b2","path":"","rule":"set","safe":[1,6],"absentSafe":false,"fix":1,"max":6}],"gate":{"areas":[],"pattern":".","tamperExempt":true,"secedit":false,"ownPolicyKey":"","policyValues":[]}}'

# ---- rule semantics
MakeSpec $pplJson
$def = HDef 'RunAsPPL'
Assert (!(HIsSafe $def $null) -and !(HIsSafe $def 0) -and (HIsSafe $def 1) -and (HIsSafe $def 2)) 'PPL safety'
Assert ((HFixOf $def $null) -eq 2 -and (HFixOf $def 0) -eq 2 -and (HFixOf $def 1) -eq 1) 'PPL fix keeps the firmware lock'
Reject { HDef 'Other' } 'Unknown hardening item'
MakeSpec $pnpJson
Assert (HIsSafe (HDef 'UpdatePromptSettings') $null) 'Absent policy value is the safe default'
Assert ($null -eq (HFixOf (HDef 'UpdatePromptSettings') 2)) 'Unsafe policy value is removed'
Assert ((HFixOf (HDef 'UpdatePromptSettings') 1) -eq 1) 'Safe value kept'
MakeSpec $fwJson
$exp = HDef 'FPS-X'
Assert ((HFixOf $exp 15) -eq 11 -and (HFixOf $exp 12) -eq 4 -and (HFixOf $exp 7) -eq 7 -and (HFixOf $exp 3) -eq 3) 'Exposure fixes'
foreach ($ok in @('FPS-SMB-In-TCP','NETDIS-LLMNR-In-UDP')) { Assert (HNameOk $ok) "name $ok" }
foreach ($bad in @('RemoteDesktop-In','FPS-','fps-x',"FPS-x'; calc",'FPS-a b','')) { Assert (!(HNameOk $bad)) "name $bad accepted" }
MakeSpec $wifiJson
foreach ($ok in @('Cafe Guest',"John's Home",'Kaffee Straße')) { Assert (HNameOk $ok) "wifi $ok" }
foreach ($bad in @('','a"b'," padded","tab`there",('x' * 65))) { Assert (!(HNameOk $bad)) "wifi name accepted: $bad" }

# ---- input parsing
MakeSpec $pplJson
function Parse($json) { HParseInput (ConvertFrom-Json -InputObject $json) }
Assert ((Parse '{"items":{"RunAsPPL":2}}')['RunAsPPL'] -eq 2) 'parse'
Assert ($null -eq (Parse '{"items":{"RunAsPPL":null}}')['RunAsPPL']) 'parse null'
foreach ($bad in @('{"items":{"RunAsPPL":3}}','{"items":{"RunAsPPL":-1}}','{"items":{"RunAsPPL":"1"}}','{"items":{"RunAsPPL":1.5}}','{"items":{"X":1}}','{"items":{}}','{"items":{"RunAsPPL":1},"extra":1}','{"present":true,"value":1}')) {
    $caught = $false; try { $null = Parse $bad } catch { $caught = $true }
    Assert $caught "accepted $bad"
}

# ---- write transitions (state double behind HRead / HSet)
function HRead { return $script:state.Clone() }
function HGate { $script:gates++; if ($script:blocked) { throw 'management gate' } }
function HPreflight { $script:preflights++; if ($script:preflightFails) { throw 'Not offered: fixture' } }
function HSet([string]$name, $v) {
    $script:sets += ,@($name, $v)
    if ($script:failOn -ceq $name) { throw 'fixture write failure' }
    if (!$script:ignoreWrites) { $script:state[$name] = $v }
}
function Reset([string]$json, $state) {
    MakeSpec $json
    $script:state = $state; $script:sets = @(); $script:blocked = $false; $script:gates = 0
    $script:preflights = 0; $script:preflightFails = $false; $script:failOn = ''; $script:ignoreWrites = $false
}
function Input($json) { ConvertFrom-Json -InputObject $json }

Reset $pplJson @{ RunAsPPL = $null }
HWrite (Input '{"items":{"RunAsPPL":2}}')
Assert ($script:sets.Count -eq 1 -and $script:state['RunAsPPL'] -eq 2 -and $script:preflights -eq 1 -and $script:gates -ge 1) 'repair'
# Undo is gated but never preflighted (a failing preflight must not trap the person).
$script:preflightFails = $true
HWrite (Input '{"items":{"RunAsPPL":null}}')
Assert ($null -eq $script:state['RunAsPPL'] -and $script:preflights -eq 1) 'undo skips preflight'
Reset $pplJson @{ RunAsPPL = $null }; $script:preflightFails = $true
Reject { HWrite (Input '{"items":{"RunAsPPL":2}}') } 'Not offered'
Assert ($script:sets.Count -eq 0) 'preflight failure wrote'
Reset $pplJson @{ RunAsPPL = $null }; $script:blocked = $true
Reject { HWrite (Input '{"items":{"RunAsPPL":2}}') } 'management gate'
Assert ($script:sets.Count -eq 0) 'gate bypassed'
# Drift: any state that is neither the recorded original nor its fix stops the write.
foreach ($drift in @(0, 1)) {
    Reset $pplJson @{ RunAsPPL = $drift }
    Reject { HWrite (Input '{"items":{"RunAsPPL":null}}') } 'changed before the write'
    Assert ($script:sets.Count -eq 0) "undo overwrote drift $drift"
}
Reset $pplJson @{ RunAsPPL = 1 }
Reject { HWrite (Input '{"items":{"RunAsPPL":2}}') } 'changed before the write'
Assert ($script:sets.Count -eq 0) 'repair overwrote the firmware lock'
Reset $pplJson @{ RunAsPPL = $null }; $script:ignoreWrites = $true
Reject { HWrite (Input '{"items":{"RunAsPPL":2}}') } 'Readback did not match'

# Partial progress is rolled back so the journal still matches the machine.
Reset $pnpJson @{ RestrictDriverInstallationToAdministrators = 0; NoWarningNoElevationOnInstall = 1; UpdatePromptSettings = 2 }
$script:failOn = 'UpdatePromptSettings'
Reject { HWrite (Input '{"items":{"RestrictDriverInstallationToAdministrators":null,"NoWarningNoElevationOnInstall":null,"UpdatePromptSettings":null}}') } 'fixture write failure'
$script:failOn = ''
Assert ($script:state['RestrictDriverInstallationToAdministrators'] -eq 0 -and $script:state['NoWarningNoElevationOnInstall'] -eq 1 -and $script:state['UpdatePromptSettings'] -eq 2) 'rollback restored the originals'
Reset $pnpJson @{ RestrictDriverInstallationToAdministrators = 1; NoWarningNoElevationOnInstall = 1; UpdatePromptSettings = 1 }
HWrite (Input '{"items":{"RestrictDriverInstallationToAdministrators":1,"NoWarningNoElevationOnInstall":null,"UpdatePromptSettings":1}}')
Assert ($script:sets.Count -eq 1 -and $script:sets[0][0] -ceq 'NoWarningNoElevationOnInstall') 'only unsafe keys move'

# Dynamic items must still exist; new ones are left alone.
Reset $fwJson @{ 'FPS-A' = 15; 'FPS-B' = 12; 'NETDIS-New' = 15 }
HWrite (Input '{"items":{"FPS-A":11,"FPS-B":4}}')
Assert ($script:state['FPS-A'] -eq 11 -and $script:state['FPS-B'] -eq 4 -and $script:state['NETDIS-New'] -eq 15) 'dynamic repair'
HWrite (Input '{"items":{"FPS-A":15,"FPS-B":12}}')
Assert ($script:state['FPS-A'] -eq 15 -and $script:state['FPS-B'] -eq 12) 'dynamic undo'
Reset $fwJson @{ 'FPS-A' = 15 }
Reject { HWrite (Input '{"items":{"FPS-GONE":11}}') } 'no longer exists'
Reject { HWrite (Input '{"items":{"RemoteDesktop-In":11}}') } 'Unknown hardening item'
Reset $wifiJson @{ 'Cafe' = 1; 'Home' = 0 }
HWrite (Input '{"items":{"Cafe":0}}')
Assert ($script:state['Cafe'] -eq 0 -and $script:state['Home'] -eq 0 -and $script:sets.Count -eq 1) 'wifi repair'
HWrite (Input '{"items":{"Cafe":1}}')
Assert ($script:state['Cafe'] -eq 1) 'wifi undo'

# ---- observation: preflight only when something is unsafe; gate text is the reason
Reset $pplJson @{ RunAsPPL = $null }
$o = HObserve
Assert ($o.eligible -and $o.value.items['RunAsPPL'] -eq $null -and $script:preflights -eq 1) 'observe unsafe'
Reset $pplJson @{ RunAsPPL = 2 }; $script:preflightFails = $true
$o = HObserve
Assert ($o.eligible -and $script:preflights -eq 0) 'safe state is never gated by preflight'
Reset $pplJson @{ RunAsPPL = $null }; $script:preflightFails = $true
$o = HObserve
Assert (!$o.eligible -and $o.reason -ceq 'Not offered: fixture') 'preflight reason surfaces'
Reset $pplJson @{ RunAsPPL = $null }; $script:blocked = $true
$o = HObserve
Assert (!$o.eligible -and $o.reason -ceq 'management gate' -and $o.value.items['RunAsPPL'] -eq $null) 'managed device is observed but not eligible'
Assert ((ConvertTo-Json -InputObject $o -Depth 8 -Compress) -like '*"RunAsPPL":null*') 'absent serializes as null'

# ---- readers
${function:HRead} = $realHRead
function Get-MpPreference { return $script:mp }
MakeSpec '{"id":"defender.cloud_protection","source":"DefenderPref","dynamic":false,"reboot":false,"keys":[{"name":"MAPSReporting","path":"","rule":"set","safe":[1,2],"absentSafe":false,"fix":2,"max":2},{"name":"DisableBlockAtFirstSeen","path":"","rule":"set","safe":[0],"absentSafe":false,"fix":0,"max":1}],"gate":{"areas":[],"pattern":".","tamperExempt":false,"secedit":false,"ownPolicyKey":"","policyValues":[]}}'
$script:mp = [pscustomobject]@{ MAPSReporting = 'Advanced'; DisableBlockAtFirstSeen = $false }
$r = HRead
Assert ($r['MAPSReporting'] -eq 2 -and $r['DisableBlockAtFirstSeen'] -eq 0) 'defender enum names'
$script:mp = [pscustomobject]@{ MAPSReporting = [byte]0; DisableBlockAtFirstSeen = $true }
$r = HRead
Assert ($r['MAPSReporting'] -eq 0 -and $r['DisableBlockAtFirstSeen'] -eq 1) 'defender numeric'
$script:mp = [pscustomobject]@{ MAPSReporting = $null; DisableBlockAtFirstSeen = $false }
Reject { HRead } 'not readable'
MakeSpec '{"id":"defender.script_nis","source":"DefenderPref","dynamic":false,"reboot":false,"keys":[{"name":"DisableScriptScanning","path":"","rule":"set","safe":[0],"absentSafe":true,"fix":0,"max":1},{"name":"DisableIntrusionPreventionSystem","path":"","rule":"set","safe":[0],"absentSafe":true,"fix":0,"max":1}],"gate":{"areas":[],"pattern":".","tamperExempt":true,"secedit":false,"ownPolicyKey":"","policyValues":[]}}'
$script:mp = [pscustomobject]@{ DisableScriptScanning = $false; DisableIntrusionPreventionSystem = $null }
$r = HRead
Assert ($r['DisableIntrusionPreventionSystem'] -eq 0 -and !(HAnyUnsafe $r)) 'unset prevention preference means on'
$script:mp = [pscustomobject]@{ DisableScriptScanning = 'yes'; DisableIntrusionPreventionSystem = $false }
Reject { HRead } 'readable boolean'

MakeSpec $asrJson
$script:mp = [pscustomobject]@{
    AttackSurfaceReductionRules_Ids = @('56A863A9-875E-4185-98A7-B882C64B5CE5', 'aaaaaaaa-0000-0000-0000-000000000000')
    AttackSurfaceReductionRules_Actions = @([byte]2, [byte]1)
}
$r = HRead
Assert ($r['56a863a9-875e-4185-98a7-b882c64b5ce5'] -eq 2 -and $null -eq $r['9e6c4e1f-7d60-472f-ba1a-a39ef669e4b2'] -and $r.Count -eq 2) 'asr read (case-insensitive, other rules ignored)'
Assert (HAnyUnsafe $r) 'audit mode and unconfigured rules are not protection'
$script:mp = [pscustomobject]@{ AttackSurfaceReductionRules_Ids = $null; AttackSurfaceReductionRules_Actions = $null }
Assert ($null -eq (HRead)['56a863a9-875e-4185-98a7-b882c64b5ce5']) 'empty asr lists'
$script:mp = [pscustomobject]@{ AttackSurfaceReductionRules_Ids = @('a', 'b'); AttackSurfaceReductionRules_Actions = @([byte]1) }
Reject { HRead } 'inconsistent'

# Setters send fixed, typed arguments only.
$script:calls = @()
function Add-MpPreference { param($AttackSurfaceReductionRules_Ids, $AttackSurfaceReductionRules_Actions); $script:calls += ,@('add', $AttackSurfaceReductionRules_Ids, $AttackSurfaceReductionRules_Actions) }
function Remove-MpPreference { param($AttackSurfaceReductionRules_Ids); $script:calls += ,@('remove', $AttackSurfaceReductionRules_Ids) }
function Set-MpPreference { param($MAPSReporting, $PUAProtection, $DisableBlockAtFirstSeen, $DisableScriptScanning, $DisableIntrusionPreventionSystem); $script:calls += ,@('set', $MAPSReporting, $PUAProtection, $DisableBlockAtFirstSeen, $DisableScriptScanning, $DisableIntrusionPreventionSystem) }
$asrDef = HDef '56a863a9-875e-4185-98a7-b882c64b5ce5'
HSetAsr $asrDef 1; HSetAsr $asrDef 6; HSetAsr $asrDef 2; HSetAsr $asrDef 0; HSetAsr $asrDef $null
Assert (($script:calls | ForEach-Object { $_[0] + ':' + $_[2] }) -join ',' -ceq 'add:Enabled,add:Warn,add:AuditMode,add:Disabled,remove:') 'asr setter arguments'
Reject { HSetAsr $asrDef 3 } 'Invalid attack surface rule action'
$script:calls = @()
MakeSpec '{"id":"defender.cloud_protection","source":"DefenderPref","dynamic":false,"reboot":false,"keys":[{"name":"MAPSReporting","path":"","rule":"set","safe":[1,2],"absentSafe":false,"fix":2,"max":2},{"name":"DisableBlockAtFirstSeen","path":"","rule":"set","safe":[0],"absentSafe":false,"fix":0,"max":1}],"gate":{"areas":[],"pattern":".","tamperExempt":false,"secedit":false,"ownPolicyKey":"","policyValues":[]}}'
HSetDefenderPref (HDef 'MAPSReporting') 2; HSetDefenderPref (HDef 'DisableBlockAtFirstSeen') 0; HSetDefenderPref (HDef 'MAPSReporting') 0
Assert ($script:calls[0][1] -ceq 'Advanced' -and $script:calls[1][3] -eq $false -and $script:calls[2][1] -ceq 'Disabled') 'defender setter arguments'

# Firewall rule reader keeps only inbound allow rules with safe names.
MakeSpec $fwJson
function Get-NetFirewallRule {
    param($PolicyStore, $Name, $ErrorAction)
    $all = @(
        [pscustomobject]@{ Name = 'FPS-SMB-In-TCP'; Direction = 'Inbound'; Action = 'Allow'; Profile = 7; Enabled = 'True' }
        [pscustomobject]@{ Name = 'FPS-Any'; Direction = 'Inbound'; Action = 'Allow'; Profile = 0; Enabled = 'True' }
        [pscustomobject]@{ Name = 'FPS-PublicOnly'; Direction = 'Inbound'; Action = 'Allow'; Profile = 4; Enabled = 'False' }
        [pscustomobject]@{ Name = 'FPS-Out'; Direction = 'Outbound'; Action = 'Allow'; Profile = 7; Enabled = 'True' }
        [pscustomobject]@{ Name = 'FPS-Block'; Direction = 'Inbound'; Action = 'Block'; Profile = 7; Enabled = 'True' }
        [pscustomobject]@{ Name = 'NETDIS-LLMNR-In-UDP'; Direction = 'Inbound'; Action = 'Allow'; Profile = 3; Enabled = 'True' }
        [pscustomobject]@{ Name = 'FPS-Odd Name'; Direction = 'Inbound'; Action = 'Allow'; Profile = 7; Enabled = 'True' }
    )
    $hits = @($all | Where-Object { $_.Name -like $Name })
    if ($hits.Count -eq 0) {
        $e = [Management.Automation.ErrorRecord]::new([Exception]::new('none'), 'none', [Management.Automation.ErrorCategory]::ObjectNotFound, $Name)
        throw $e
    }
    return $hits
}
$r = HReadFirewall
Assert ($r.Count -eq 4 -and $r['FPS-SMB-In-TCP'] -eq 15 -and $r['FPS-Any'] -eq 15 -and $r['FPS-PublicOnly'] -eq 4 -and $r['NETDIS-LLMNR-In-UDP'] -eq 11) 'firewall slice'
$script:calls = @()
function Set-NetFirewallRule { param($PolicyStore, $Name, $Profile, $Enabled, $ErrorAction); $script:calls += ,@($Name, ($Profile -join '+'), $Enabled) }
HSetFirewall 'FPS-A' 11; HSetFirewall 'FPS-B' 4; HSetFirewall 'FPS-C' 15; HSetFirewall 'FPS-D' 12
Assert (($script:calls | ForEach-Object { $_ -join '/' }) -join ',' -ceq 'FPS-A/Domain+Private/True,FPS-B/Public/False,FPS-C/Any/True,FPS-D/Public/True') 'firewall setter arguments'

# ---- management and policy vetoes (registry / RSOP doubles)
$script:fakeFs = $false; $script:fakePaths = @(); $script:fakeKey = $null
function Test-Path { param($LiteralPath, $ErrorAction); if ($script:fakeFs) { return ($script:fakePaths -contains $LiteralPath) }; return (Microsoft.PowerShell.Management\Test-Path -LiteralPath $LiteralPath) }
function Get-Item { param($LiteralPath, $ErrorAction); if ($script:fakeFs) { return $script:fakeKey }; return (Microsoft.PowerShell.Management\Get-Item -LiteralPath $LiteralPath) }
function FakeKey($names, [int]$subkeys = 0) {
    $k = [pscustomobject]@{ SubKeyCount = $subkeys; Names = @($names) }
    $k | Add-Member ScriptMethod GetValueNames { return @($this.Names) }
    return $k
}
$own = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\Printers\PointAndPrint'
$gateJson = '{"id":"printer.point_and_print","source":"Registry","dynamic":false,"reboot":false,"keys":[{"name":"UpdatePromptSettings","path":"' + $own.Replace('\', '\\') + '","rule":"set","safe":[0,1],"absentSafe":true,"fix":null,"max":2}],"gate":{"areas":[],"pattern":".","tamperExempt":false,"secedit":true,"ownPolicyKey":"' + $own.Replace('\', '\\') + '","policyValues":[{"path":"HKLM:\\SOFTWARE\\Policies\\Other","name":"Managed"}]}}'
MakeSpec $gateJson
$script:fakeFs = $true
$script:fakePaths = @($own); $script:fakeKey = FakeKey @('UpdatePromptSettings')
HGatePolicy
Assert $true 'only our own value in the policy key is acceptable'
$script:fakeKey = FakeKey @('UpdatePromptSettings', 'TrustedServers')
Reject { HGatePolicy } 'Relevant policy is configured'
$script:fakeKey = FakeKey @('UpdatePromptSettings') 1
Reject { HGatePolicy } 'Relevant policy is configured'
$script:fakePaths = @($own, 'HKLM:\SOFTWARE\Policies\Other'); $script:fakeKey = FakeKey @('Managed')
Reject { HGatePolicy } 'Relevant policy is configured'
$script:fakeKey = FakeKey @('UpdatePromptSettings')
$script:fakePaths = @($own, [IO.Path]::Combine($env:SystemRoot, 'System32\GroupPolicy\Machine\Microsoft\Windows NT\SecEdit\GptTmpl.inf'))
Reject { HGatePolicy } 'Local computer policy artifacts'
$script:fakePaths = @($own, [IO.Path]::Combine($env:SystemRoot, 'System32\GroupPolicy\Machine\Registry.pol'))
Reject { HGatePolicy } 'Local computer policy artifacts'
$script:fakePaths = @(); $script:fakeFs = $false
function Get-CimInstance { param($ClassName, $Namespace); switch ($ClassName) { 'RSOP_GPO' { return $script:gpos } 'RSOP_RegistryPolicySetting' { return $script:rsopSettings } default { throw "Unexpected probe $ClassName" } } }
$script:gpos = @(@{ id = 'LocalGPO'; enabled = $true; accessDenied = $false; filterAllowed = $true })
$script:rsopSettings = @(@{ registryKey = 'SOFTWARE\Policies\Microsoft\Windows NT\Printers\PointAndPrint'; valueName = 'Other' })
HRsop
$script:rsopSettings = @(@{ registryKey = 'SOFTWARE\Policies\Microsoft\Windows NT\Printers\PointAndPrint'; valueName = 'UpdatePromptSettings' })
Reject { HRsop } 'Relevant resultant Group Policy'
$script:rsopSettings = @()
$script:gpos = @(@{ id = '{31B2F340-016D-11D2-945F-00C04FB984F9}'; enabled = $true; accessDenied = $false; filterAllowed = $true })
Reject { HRsop } 'Applied computer Group Policy'
$script:gpos = @(@{ id = 'x'; enabled = 'yes'; accessDenied = $false; filterAllowed = $true })
Reject { HRsop } 'authority is unknown'

# Wi-Fi profile classification reads files only and flags weak/open networks.
$root = Join-Path ([IO.Path]::GetTempPath()) ('secblitz-wlan-' + [Guid]::NewGuid().ToString('N'))
$iface = '{' + [Guid]::NewGuid().ToString() + '}'
$null = New-Item -ItemType Directory -Path (Join-Path $root $iface) -Force
function HWlanRoot { return $root }
function Profile([string]$name, [string]$mode, [string]$auth, [string]$enc, [string]$extra = '') {
    "<?xml version=`"1.0`"?><WLANProfile xmlns=`"http://www.microsoft.com/networking/WLAN/profile/v1`"><name>$name</name><connectionType>ESS</connectionType><connectionMode>$mode</connectionMode><MSM><security><authEncryption><authentication>$auth</authentication><encryption>$enc</encryption><useOneX>false</useOneX></authEncryption>$extra</security></MSM></WLANProfile>"
}
try {
    $i = 0
    foreach ($p in @(
        (Profile 'OpenCafe' 'auto' 'open' 'none'), (Profile 'WepOld' 'manual' 'open' 'WEP'), (Profile 'TkipBox' 'auto' 'WPAPSK' 'TKIP'),
        (Profile 'Good2' 'auto' 'WPA2PSK' 'AES'), (Profile 'Good3' 'auto' 'WPA3SAE' 'AES'), (Profile 'Odd"Name' 'auto' 'open' 'none'),
        (Profile 'Bad Mode' 'weird' 'open' 'none')
    )) { [IO.File]::WriteAllText((Join-Path (Join-Path $root $iface) ("p$i.xml")), $p); $i++ }
    [IO.File]::WriteAllText((Join-Path (Join-Path $root $iface) 'enterprise.xml'), ((Profile 'Corp' 'auto' 'open' 'none') -replace '<useOneX>false', '<useOneX>true'))
    MakeSpec $wifiJson
    $r = HReadWifi
    Assert ($r.Count -eq 3 -and $r['OpenCafe'] -eq 1 -and $r['WepOld'] -eq 0 -and $r['TkipBox'] -eq 1) "wifi slice: $($r.Keys -join ',')"
    # A duplicate name (two adapters) is ambiguous and therefore left alone.
    [IO.File]::WriteAllText((Join-Path (Join-Path $root $iface) 'dupe.xml'), (Profile 'OpenCafe' 'auto' 'open' 'none'))
    Assert ((HReadWifi).Count -eq 2) 'duplicate names excluded'
    # DTDs are refused rather than resolved.
    [IO.File]::WriteAllText((Join-Path (Join-Path $root $iface) 'xxe.xml'), '<?xml version="1.0"?><!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/passwd">]><WLANProfile><name>&e;</name></WLANProfile>')
    Reject { HReadWifi } 'DTD'
} finally { Remove-Item -LiteralPath $root -Recurse -Force }

Write-Output "Hardening PowerShell fixtures passed: $script:checks checks"
