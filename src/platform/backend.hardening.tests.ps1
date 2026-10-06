# Non-mutating fixtures for the extended hardening controls (hardening.ps1).
# Only function definitions from the production files are executed; every
# registry, Defender, firewall, account and Wi-Fi touchpoint is an in-memory
# double. Compatible with Windows PowerShell 5.1 and PowerShell 7.
# Optional: set SECBLITZ_PARITY to the file written by the Rust test
# `export_rule_parity_fixture_for_powershell` (SECBLITZ_PARITY_OUT) to prove the
# PowerShell rules equal the compiled catalog for every spec and candidate value.
param(
    [string]$BackendPath = (Join-Path $PSScriptRoot 'backend.ps1'),
    [string]$HardeningPath = (Join-Path $PSScriptRoot 'hardening.ps1'),
    [string]$HandledPath = (Join-Path $PSScriptRoot 'hardening.handled.ps1')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
foreach ($path in @($BackendPath, $HandledPath, $HardeningPath)) {
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
    foreach ($node in $ast.EndBlock.Statements) {
        if ($node -is [Management.Automation.Language.FunctionDefinitionAst]) { . ([scriptblock]::Create($node.Extent.Text)) }
        # Module-level constants of hardening.ps1 ($hName = literal), so new
        # constants never need copying into this file by hand.
        elseif ($path -ne $BackendPath -and $node -is [Management.Automation.Language.AssignmentStatementAst] -and
                $node.Left -is [Management.Automation.Language.VariableExpressionAst] -and
                $node.Left.VariablePath.UserPath -cmatch '^h[A-Z]' -and
                $node.Right.Extent.Text -notmatch '\$(?!true|false|null)') { . ([scriptblock]::Create($node.Extent.Text)) }
    }
}
$script:checks = 0
# backend.tests.ps1 runs this file as a child script, so its doubles for these
# cmdlets are visible here; start from the real ones so this suite is hermetic.
function Get-ChildItem { Microsoft.PowerShell.Management\Get-ChildItem @args }
function Test-Path { Microsoft.PowerShell.Management\Test-Path @args }
function Get-Item { Microsoft.PowerShell.Management\Get-Item @args }
function Get-ItemProperty { Microsoft.PowerShell.Management\Get-ItemProperty @args }
# The script Rust actually runs is the backend definitions plus hardening.ps1:
# it must parse as one unit and must not redefine (shadow) any backend function.
$backendText = [IO.File]::ReadAllText($BackendPath)
$delimiter = "`ntry {`n    switch -CaseSensitive (`$action) {"
$cut = $backendText.IndexOf($delimiter)
if ($cut -lt 0) { throw 'Backend dispatcher boundary changed' }
$combined = $backendText.Substring(0, $cut) + "`n" + [IO.File]::ReadAllText($HandledPath) + "`n" + [IO.File]::ReadAllText($HardeningPath)
$tokens = $null; $errors = $null
$combinedAst = [Management.Automation.Language.Parser]::ParseInput($combined, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
$names = @($combinedAst.FindAll({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] }, $false) | ForEach-Object { $_.Name.ToLowerInvariant() })
if (@($names | Group-Object | Where-Object { $_.Count -gt 1 }).Count -gt 0) { throw 'Duplicate function definition across backend.ps1, hardening.handled.ps1 and hardening.ps1' }
$realHRead = ${function:HRead}
$realHPreflight = ${function:HPreflight}
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

# ---- network and Defender extension controls: specs
$gateTail = '"gate":{"areas":[],"pattern":".","tamperExempt":false,"secedit":false,"ownPolicyKey":"","policyValues":[]}'
$gateExempt = $gateTail.Replace('"tamperExempt":false', '"tamperExempt":true')
$tlsJson = '{"id":"tls.legacy_protocols","source":"Registry","dynamic":false,"reboot":true,"keys":[{"name":"ssl3.client.enabled","path":"HKLM:\\T","valueName":"Enabled","rule":"set","safe":[0],"absentSafe":false,"fix":0,"max":4294967295},{"name":"ssl3.client.default_off","path":"HKLM:\\T","valueName":"DisabledByDefault","rule":"set","safe":[1],"absentSafe":false,"fix":1,"max":1}],' + $gateTail + '}'
$stackJson = '{"id":"net.stack_hardening","source":"Registry","dynamic":false,"reboot":true,"keys":[{"name":"DisableIPSourceRouting","path":"HKLM:\\T4","valueName":"DisableIPSourceRouting","rule":"set","safe":[2],"absentSafe":false,"fix":2,"max":2},{"name":"DisableIPSourceRouting6","path":"HKLM:\\T6","valueName":"DisableIPSourceRouting","rule":"set","safe":[2],"absentSafe":false,"fix":2,"max":2}],' + $gateTail + '}'
$nbJson = '{"id":"net.netbios","source":"NetbiosAdapters","dynamic":true,"reboot":false,"keys":[{"name":"*","path":"","valueName":"*","rule":"set","safe":[2],"absentSafe":false,"fix":2,"max":2}],' + $gateTail + '}'
$obJson = '{"id":"firewall.outbound_smb_internet","source":"FirewallOutbound","dynamic":false,"reboot":false,"keys":[{"name":"RulePresent","path":"","valueName":"RulePresent","rule":"set","safe":[1],"absentSafe":false,"fix":1,"max":1}],' + $gateTail + '}'
$npJson = '{"id":"defender.network_protection","source":"DefenderPref","dynamic":false,"reboot":false,"keys":[{"name":"EnableNetworkProtection","path":"","valueName":"EnableNetworkProtection","rule":"set","safe":[1],"absentSafe":false,"fix":1,"max":2}],' + $gateExempt + '}'
$cblJson = '{"id":"defender.cloud_block_level","source":"DefenderPref","dynamic":false,"reboot":false,"keys":[{"name":"CloudBlockLevel","path":"","valueName":"CloudBlockLevel","rule":"set","safe":[2,4,6],"absentSafe":false,"fix":2,"max":6},{"name":"CloudExtendedTimeout","path":"","valueName":"CloudExtendedTimeout","rule":"set","safe":[20,25,50],"absentSafe":false,"fix":20,"max":50}],' + $gateExempt + '}'
$officeJson = '{"id":"defender.asr.office","source":"DefenderAsr","dynamic":false,"reboot":false,"keys":[{"name":"75668c1f-73b5-4cf0-bb93-3ecf5cb7cc84","path":"","valueName":"75668c1f-73b5-4cf0-bb93-3ecf5cb7cc84","rule":"set","safe":[1],"absentSafe":false,"fix":1,"max":6}],' + $gateExempt + '}'

# DWORD 0xFFFFFFFF ("enabled" in many guides) is a legal original and is restored exactly.
MakeSpec $tlsJson
Assert (!(HIsSafe (HDef 'ssl3.client.enabled') 4294967295) -and !(HIsSafe (HDef 'ssl3.client.enabled') $null) -and (HIsSafe (HDef 'ssl3.client.enabled') 0)) 'tls enabled safety'
Assert ((HFixOf (HDef 'ssl3.client.enabled') 4294967295) -eq 0 -and (HFixOf (HDef 'ssl3.client.default_off') $null) -eq 1) 'tls fixes'
Assert ((Parse '{"items":{"ssl3.client.enabled":4294967295,"ssl3.client.default_off":1}}')['ssl3.client.enabled'] -eq 4294967295) 'parse 0xFFFFFFFF'
Reject { $null = Parse '{"items":{"ssl3.client.enabled":4294967296,"ssl3.client.default_off":1}}' } 'out of range'
Reset $tlsJson @{ 'ssl3.client.enabled' = 4294967295; 'ssl3.client.default_off' = $null }
HWrite (Input '{"items":{"ssl3.client.enabled":0,"ssl3.client.default_off":1}}')
Assert ($script:sets.Count -eq 2 -and $script:state['ssl3.client.enabled'] -eq 0 -and $script:state['ssl3.client.default_off'] -eq 1 -and $script:preflights -eq 1) 'tls repair'
HWrite (Input '{"items":{"ssl3.client.enabled":4294967295,"ssl3.client.default_off":null}}')
Assert ($script:state['ssl3.client.enabled'] -eq 4294967295 -and $null -eq $script:state['ssl3.client.default_off'] -and $script:preflights -eq 1) 'tls undo restores the exact original'
Reset $tlsJson @{ 'ssl3.client.enabled' = 1; 'ssl3.client.default_off' = 1 }
Reject { HWrite (Input '{"items":{"ssl3.client.enabled":4294967295,"ssl3.client.default_off":1}}') } 'changed before the write'
Assert ($script:sets.Count -eq 0) 'tls drift'

# NetBIOS is restored adapter by adapter; adapters that appear later are left alone.
Reset $nbJson @{ '{11111111-1111-1111-1111-111111111111}' = 0; '{22222222-2222-2222-2222-222222222222}' = 1; '{33333333-3333-3333-3333-333333333333}' = 2 }
HWrite (Input '{"items":{"{11111111-1111-1111-1111-111111111111}":2,"{22222222-2222-2222-2222-222222222222}":2,"{33333333-3333-3333-3333-333333333333}":2}}')
Assert ($script:sets.Count -eq 2 -and $script:preflights -eq 1) 'netbios repair only moves unsafe adapters'
HWrite (Input '{"items":{"{11111111-1111-1111-1111-111111111111}":0,"{22222222-2222-2222-2222-222222222222}":1,"{33333333-3333-3333-3333-333333333333}":2}}')
Assert ($script:state['{11111111-1111-1111-1111-111111111111}'] -eq 0 -and $script:state['{22222222-2222-2222-2222-222222222222}'] -eq 1 -and $script:state['{33333333-3333-3333-3333-333333333333}'] -eq 2) 'netbios undo per adapter'
Reject { HWrite (Input '{"items":{"{99999999-9999-9999-9999-999999999999}":2}}') } 'no longer exists'
Reject { HWrite (Input '{"items":{"Ethernet":2}}') } 'Unknown hardening item'

# The firewall rule is present (1) or absent (0); undo removes it.
Reset $obJson @{ RulePresent = 0 }
HWrite (Input '{"items":{"RulePresent":1}}')
Assert ($script:state['RulePresent'] -eq 1 -and $script:preflights -eq 1) 'outbound rule added'
HWrite (Input '{"items":{"RulePresent":0}}')
Assert ($script:state['RulePresent'] -eq 0) 'outbound rule removed on undo'

# Defender failures mention tamper protection.
Reset $npJson @{ EnableNetworkProtection = 0 }; $script:ignoreWrites = $true
Reject { HWrite (Input '{"items":{"EnableNetworkProtection":1}}') } 'tamper protection'

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
$script:mp = [pscustomobject]@{ DisableScriptScanning = $false }
$r = HRead
Assert ($r['DisableIntrusionPreventionSystem'] -eq 0 -and !(HAnyUnsafe $r)) 'retired prevention preference (missing property) means on'
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


# ---- network and Defender extension controls: readers, setters, preflights
${function:HPreflight} = $realHPreflight
function FakeDwordKey([hashtable]$values) {
    $k = [pscustomobject]@{ Values = $values; SubKeyCount = 0 }
    $k | Add-Member ScriptMethod GetValueNames { return @($this.Values.Keys) }
    $k | Add-Member ScriptMethod GetValueKind { param($n) return [Microsoft.Win32.RegistryValueKind]::DWord }
    $k | Add-Member ScriptMethod GetValue { param($n) return $this.Values[$n] }
    return $k
}
# Registry: unsigned DWORDs, and one value name under several keys.
MakeSpec $tlsJson
$script:fakeFs = $true; $script:fakePaths = @('HKLM:\T'); $script:fakeKey = FakeDwordKey @{ Enabled = [int]-1 }
Assert ((HReadRegistry (HDef 'ssl3.client.enabled')) -eq 4294967295) '0xFFFFFFFF reads back unsigned'
Assert ($null -eq (HReadRegistry (HDef 'ssl3.client.default_off'))) 'absent value reads as null'
$script:fakeKey = FakeDwordKey @{ Enabled = 0; DisabledByDefault = 1 }
Assert ((HReadRegistry (HDef 'ssl3.client.enabled')) -eq 0 -and (HReadRegistry (HDef 'ssl3.client.default_off')) -eq 1) 'tls read'
$script:calls = @()
function New-ItemProperty { param($LiteralPath, $Name, $PropertyType, $Value, [switch]$Force, $ErrorAction); $script:calls += ,@($Name, $Value) }
function Remove-ItemProperty { param($LiteralPath, $Name, $ErrorAction); $script:calls += ,@($Name, 'removed') }
HSetRegistry (HDef 'ssl3.client.enabled') 4294967295
HSetRegistry (HDef 'ssl3.client.enabled') 0
HSetRegistry (HDef 'ssl3.client.default_off') $null
Assert (($script:calls | ForEach-Object { "$($_[0])=$($_[1])" }) -join ',' -ceq 'Enabled=-1,Enabled=0,DisabledByDefault=removed') 'registry setter keeps the unsigned bits and value name'
MakeSpec $stackJson
Assert ((HValueName (HDef 'DisableIPSourceRouting6')) -ceq 'DisableIPSourceRouting' -and (HDef 'DisableIPSourceRouting6').path -ceq 'HKLM:\T6') 'same value name under another key'
$script:fakePaths = @(); $script:fakeFs = $false

# NetBIOS adapters.
MakeSpec $nbJson
foreach ($ok in @('{11111111-1111-1111-1111-111111111111}', '{abcdefAB-1111-2222-3333-444444444444}')) { Assert (HNameOk $ok) "adapter $ok" }
foreach ($bad in @('', 'Ethernet', '{1111}', '11111111-1111-1111-1111-111111111111', "{11111111-1111-1111-1111-111111111111}'; calc", '{1111111g-1111-1111-1111-111111111111}')) { Assert (!(HNameOk $bad)) "adapter name accepted: $bad" }
function Get-CimInstance {
    param($ClassName, $Namespace, $Filter)
    switch ($ClassName) {
        'Win32_NetworkAdapterConfiguration' {
            if ($Filter -match "SettingID = '(.+)'") { $id = $Matches[1]; return @($script:nics | Where-Object { $_.SettingID -ceq $id }) }
            return $script:nics
        }
        'Win32_OperatingSystem' { return [pscustomobject]@{ OperatingSystemSKU = $script:sku } }
        default { throw "Unexpected probe $ClassName" }
    }
}
$script:nics = @(
    [pscustomobject]@{ SettingID = '{11111111-1111-1111-1111-111111111111}'; TcpipNetbiosOptions = [uint32]0 }
    [pscustomobject]@{ SettingID = '{22222222-2222-2222-2222-222222222222}'; TcpipNetbiosOptions = [uint32]2 }
    [pscustomobject]@{ SettingID = '{33333333-3333-3333-3333-333333333333}'; TcpipNetbiosOptions = $null }
    [pscustomobject]@{ SettingID = 'not-a-guid'; TcpipNetbiosOptions = [uint32]1 }
    [pscustomobject]@{ SettingID = '{44444444-4444-4444-4444-444444444444}'; TcpipNetbiosOptions = [uint32]1 }
    [pscustomobject]@{ SettingID = '{44444444-4444-4444-4444-444444444444}'; TcpipNetbiosOptions = [uint32]2 }
)
$r = HReadNetbios
Assert ($r.Count -eq 2 -and $r['{11111111-1111-1111-1111-111111111111}'] -eq 0 -and $r['{22222222-2222-2222-2222-222222222222}'] -eq 2) 'netbios slice skips unreadable, invalid and duplicate adapters'
$script:calls = @(); $script:rv = 0
function Invoke-CimMethod { param($InputObject, $MethodName, $Arguments); $script:calls += ,@($InputObject.SettingID, $MethodName, $Arguments.TcpipNetbiosOptions); return [pscustomobject]@{ ReturnValue = $script:rv } }
HSetNetbios '{11111111-1111-1111-1111-111111111111}' 2
HSetNetbios '{22222222-2222-2222-2222-222222222222}' 0
Assert (($script:calls | ForEach-Object { $_ -join '/' }) -join ',' -ceq '{11111111-1111-1111-1111-111111111111}/SetTcpipNetbios/2,{22222222-2222-2222-2222-222222222222}/SetTcpipNetbios/0') 'netbios setter arguments'
$script:rv = 1; HSetNetbios '{11111111-1111-1111-1111-111111111111}' 2
$script:rv = 84
Reject { HSetNetbios '{11111111-1111-1111-1111-111111111111}' 2 } 'refused'
Reject { HSetNetbios '{44444444-4444-4444-4444-444444444444}' 2 } 'exactly once'
Reject { HSetNetbios '{11111111-1111-1111-1111-111111111111}' 3 } 'Invalid NetBIOS'
Reject { HSetNetbios "x'; calc" 2 } 'Invalid NetBIOS'

# NetBIOS gate: SMB1 off and no bare-name shares.
Assert ((HRemoteHost '\\NAS\media') -ceq 'NAS' -and (HRemoteHost '\\nas.local\x') -ceq 'nas.local' -and (HRemoteHost 'C:\x') -ceq '') 'remote host parsing'
Assert ((HBareName 'NAS') -and !(HBareName 'nas.local') -and !(HBareName '192.168.1.5') -and !(HBareName 'fe80::1') -and !(HBareName '')) 'bare name detection'
$script:smb = [pscustomobject]@{ EnableSMB1Protocol = $false }
$script:maps = @(); $script:conns = @(); $script:hives = @(); $script:drivePath = ''
function Get-SmbServerConfiguration { return $script:smb }
function Get-SmbMapping { param($ErrorAction); return $script:maps }
function Get-SmbConnection { param($ErrorAction); return $script:conns }
function Get-ChildItem { param($LiteralPath); if ($LiteralPath -ceq 'Registry::HKEY_USERS') { return $script:hives }; return @([pscustomobject]@{ PSPath = 'drive' }) }
function Get-ItemProperty { param($LiteralPath); return [pscustomobject]@{ RemotePath = $script:drivePath } }
$script:fakeFs = $true; $script:fakePaths = @(); $script:fakeKey = $null
MakeSpec $nbJson
HNetbiosPreflight
Assert $true 'clean machine passes the NetBIOS gate'
$script:smb = [pscustomobject]@{ EnableSMB1Protocol = $true }
Reject { HNetbiosPreflight } 'SMB1'
$script:smb = [pscustomobject]@{ EnableSMB1Protocol = $null }
Reject { HNetbiosPreflight } 'could not be checked'
$script:smb = [pscustomobject]@{ EnableSMB1Protocol = $false }
$script:fakePaths = @('HKLM:\SYSTEM\CurrentControlSet\Services\mrxsmb10'); $script:fakeKey = FakeDwordKey @{ Start = 3 }
Reject { HNetbiosPreflight } 'SMB1'
$script:fakeKey = FakeDwordKey @{ Start = 4 }
HNetbiosPreflight
$script:fakePaths = @(); $script:fakeKey = $null
$script:maps = @([pscustomobject]@{ RemotePath = '\\NAS\media' })
Reject { HNetbiosPreflight } 'old name service'
$script:maps = @([pscustomobject]@{ RemotePath = '\\nas.local\media' }, [pscustomobject]@{ RemotePath = '\\192.168.1.9\share' })
HNetbiosPreflight
$script:maps = @(); $script:conns = @([pscustomobject]@{ ServerName = 'FILESRV' })
Reject { HNetbiosPreflight } 'old name service'
$script:conns = @()
$script:hives = @([pscustomobject]@{ PSChildName = 'S-1-5-21-1-2-3-1001' }, [pscustomobject]@{ PSChildName = 'S-1-5-21-1-2-3-1001_Classes' })
$script:fakePaths = @('Registry::HKEY_USERS\S-1-5-21-1-2-3-1001\Network'); $script:drivePath = '\\OLDBOX\data'
Reject { HNetbiosPreflight } 'old name service'
$script:drivePath = '\\box.example.com\data'
HNetbiosPreflight
$script:fakePaths = @(); $script:fakeFs = $false

# Outbound SMB firewall rule.
MakeSpec $obJson
Assert ((HReadOutbound)['RulePresent'] -eq 0) 'rule absent reads as 0 (ObjectNotFound is not an error)'
$script:rule = [pscustomobject]@{ Name = 'Secblitz-Block-Outbound-SMB-Internet'; Direction = 'Outbound'; Action = 'Block'; Enabled = 'True' }
$script:portFilter = [pscustomobject]@{ Protocol = 'TCP'; RemotePort = @('445', '139') }
$script:addrFilter = [pscustomobject]@{ RemoteAddress = 'Internet' }
function Get-NetFirewallRule { param($PolicyStore, $Name, $ErrorAction); if ($Name -cne 'Secblitz-Block-Outbound-SMB-Internet') { throw 'wrong name' }; return $script:rules }
function Get-NetFirewallPortFilter { param([Parameter(ValueFromPipeline = $true)]$InputObject); process { $script:portFilter } }
function Get-NetFirewallAddressFilter { param([Parameter(ValueFromPipeline = $true)]$InputObject); process { $script:addrFilter } }
$script:rules = @($script:rule)
Assert ((HReadOutbound)['RulePresent'] -eq 1) 'exact rule reads as 1'
$script:rules = @($script:rule, $script:rule)
Reject { HReadOutbound } 'more than once'
foreach ($variant in @(
    { $script:rule = [pscustomobject]@{ Name = 'x'; Direction = 'Outbound'; Action = 'Allow'; Enabled = 'True' } },
    { $script:rule = [pscustomobject]@{ Name = 'x'; Direction = 'Outbound'; Action = 'Block'; Enabled = 'False' } },
    { $script:portFilter = [pscustomobject]@{ Protocol = 'TCP'; RemotePort = @('445') } },
    { $script:portFilter = [pscustomobject]@{ Protocol = 'UDP'; RemotePort = @('445', '139') } },
    { $script:addrFilter = [pscustomobject]@{ RemoteAddress = 'Any' } }
)) {
    $script:rule = [pscustomobject]@{ Name = 'x'; Direction = 'Outbound'; Action = 'Block'; Enabled = 'True' }
    $script:portFilter = [pscustomobject]@{ Protocol = 'TCP'; RemotePort = @('445', '139') }
    $script:addrFilter = [pscustomobject]@{ RemoteAddress = 'Internet' }
    & $variant
    $script:rules = @($script:rule)
    Reject { HReadOutbound } 'exists but is different'
}
$script:calls = @()
function New-NetFirewallRule { param($PolicyStore, $Name, $DisplayName, $Description, $Direction, $Action, $Protocol, $RemotePort, $RemoteAddress, $Profile, $Enabled, $ErrorAction); $script:calls += ,@('new', $PolicyStore, $Name, $DisplayName, $Direction, $Action, $Protocol, ($RemotePort -join '+'), $RemoteAddress, $Profile, $Enabled) }
function Remove-NetFirewallRule { param($PolicyStore, $Name, $ErrorAction); $script:calls += ,@('remove', $PolicyStore, $Name) }
HSetOutbound 1; HSetOutbound 0
Assert (($script:calls | ForEach-Object { $_ -join '/' }) -join ',' -ceq 'new/PersistentStore/Secblitz-Block-Outbound-SMB-Internet/Secblitz: block outbound file sharing to the internet/Outbound/Block/TCP/445+139/Internet/Any/True,remove/PersistentStore/Secblitz-Block-Outbound-SMB-Internet') 'outbound rule setter arguments'

# Defender preference readers and setters for the new keys.
MakeSpec $npJson
$script:mp = [pscustomobject]@{ EnableNetworkProtection = $null }
Assert ((HRead)['EnableNetworkProtection'] -eq 0) 'unreported network protection counts as off'
$script:mp = [pscustomobject]@{ EnableNetworkProtection = 'Enabled' }
Assert ((HRead)['EnableNetworkProtection'] -eq 1) 'network protection name'
$script:mp = [pscustomobject]@{ EnableNetworkProtection = [byte]2 }
Assert ((HRead)['EnableNetworkProtection'] -eq 2 -and (HAnyUnsafe (HRead))) 'audit mode is not protection'
$script:mp = [pscustomobject]@{ EnableNetworkProtection = 'Bogus' }
Reject { HRead } 'not readable'
MakeSpec $cblJson
$script:mp = [pscustomobject]@{ CloudBlockLevel = 'High'; CloudExtendedTimeout = [uint32]20 }
$r = HRead
Assert ($r['CloudBlockLevel'] -eq 2 -and $r['CloudExtendedTimeout'] -eq 20 -and !(HAnyUnsafe $r)) 'cloud block level names'
$script:mp = [pscustomobject]@{ CloudBlockLevel = [byte]6; CloudExtendedTimeout = [uint32]25 }
$r = HRead
Assert ($r['CloudBlockLevel'] -eq 6 -and !(HAnyUnsafe $r)) 'zero tolerance is preserved, never written'
Assert ((HFixOf (HDef 'CloudBlockLevel') 0) -eq 2 -and (HFixOf (HDef 'CloudBlockLevel') 1) -eq 2 -and (HFixOf (HDef 'CloudExtendedTimeout') 0) -eq 20) 'cloud block fixes are High and 20 seconds'
$script:mp = [pscustomobject]@{ CloudBlockLevel = 'Default'; CloudExtendedTimeout = [uint32]0 }
$r = HRead
Assert ($r['CloudBlockLevel'] -eq 0 -and $r['CloudExtendedTimeout'] -eq 0 -and (HAnyUnsafe $r)) 'defaults are not protected'
$script:mp = [pscustomobject]@{ CloudBlockLevel = [byte]3; CloudExtendedTimeout = [uint32]0 }
Reject { HRead } 'not readable'
$script:mp = [pscustomobject]@{ CloudBlockLevel = 'High'; CloudExtendedTimeout = $null }
Reject { HRead } 'not readable'
$script:calls = @()
function Set-MpPreference { param($EnableNetworkProtection, $CloudBlockLevel, $CloudExtendedTimeout); $script:calls += ,@($EnableNetworkProtection, $CloudBlockLevel, $CloudExtendedTimeout) }
HSetDefenderPref (HDef 'CloudBlockLevel') 2; HSetDefenderPref (HDef 'CloudBlockLevel') 0; HSetDefenderPref (HDef 'CloudBlockLevel') 4; HSetDefenderPref (HDef 'CloudExtendedTimeout') 20
Reject { HSetDefenderPref (HDef 'CloudBlockLevel') 3 } 'Invalid cloud block level'
MakeSpec $npJson
HSetDefenderPref (HDef 'EnableNetworkProtection') 1; HSetDefenderPref (HDef 'EnableNetworkProtection') 0; HSetDefenderPref (HDef 'EnableNetworkProtection') 2
Assert (($script:calls | ForEach-Object { "$($_[0])|$($_[1])|$($_[2])" }) -join ',' -ceq '|High|,|Default|,|HighPlus|,||20,Enabled||,Disabled||,AuditMode||') 'new Defender setter arguments'

# Preflights: each condition has its own calm reason; undo is never preflighted.
$script:fakeFs = $true; $script:fakePaths = @(); $script:fakeKey = $null
$script:status = [pscustomobject]@{ RealTimeProtectionEnabled = $true; BehaviorMonitorEnabled = $true }
function Get-MpComputerStatus { return $script:status }
$script:mp = [pscustomobject]@{ MAPSReporting = 'Advanced' }
$script:sku = 48
MakeSpec $npJson
HPreflight
$script:sku = 101
Reject { HPreflight } 'does not include it'
$script:sku = 0
Reject { HPreflight } 'does not include it'
$script:sku = 48
$script:status = [pscustomobject]@{ RealTimeProtectionEnabled = $true; BehaviorMonitorEnabled = $false }
Reject { HPreflight } 'behavior monitoring'
$script:status = [pscustomobject]@{ RealTimeProtectionEnabled = $false; BehaviorMonitorEnabled = $true }
Reject { HPreflight } 'real-time protection is off'
$script:status = [pscustomobject]@{ RealTimeProtectionEnabled = $true; BehaviorMonitorEnabled = $true }
$script:mp = [pscustomobject]@{ MAPSReporting = 'Disabled' }
Reject { HPreflight } 'cloud protection is off'
$script:mp = [pscustomobject]@{ MAPSReporting = 'Advanced' }
$script:fakePaths = @('HKLM:\SOFTWARE\Microsoft\CCM')
Reject { HPreflight } 'Configuration Manager'
$script:fakePaths = @()
MakeSpec $cblJson
HPreflight
$script:mp = [pscustomobject]@{ MAPSReporting = 'Disabled' }
Reject { HPreflight } 'cloud protection is off'
$script:mp = [pscustomobject]@{ MAPSReporting = 'Advanced' }
# Office rules are only offered when Office is installed.
MakeSpec $officeJson
Assert (!(HOfficeInstalled)) 'no Office on a clean machine'
Reject { HPreflight } 'Office was not found'
$script:fakePaths = @('HKLM:\SOFTWARE\Microsoft\Office\ClickToRun\Configuration')
Assert (HOfficeInstalled) 'Click-to-Run Office detected'
HPreflight
if ($env:ProgramFiles) {
    $script:fakePaths = @([IO.Path]::Combine($env:ProgramFiles, 'Microsoft Office\root\Office16\WINWORD.EXE'))
    Assert (HOfficeInstalled) 'Office found by program file'
}
$script:fakePaths = @(); $script:fakeFs = $false
$script:sku = 48; Assert (HEditionHasNetworkProtection) 'Pro supports network protection'
foreach ($homeSku in @(98, 99, 100, 101, 0, 999)) { $script:sku = $homeSku; Assert (!(HEditionHasNetworkProtection)) "sku $homeSku must not be offered" }
# ==================================================================
# System area: OS / credentials / update / privacy controls
# ==================================================================
$noGate = '"gate":{"areas":[],"pattern":".","tamperExempt":false,"secedit":false,"ownPolicyKey":"","policyValues":[]}'
$exclJson = '{"id":"defender.exclusions_risky","source":"DefenderExclusions","dynamic":true,"reboot":false,"keys":[{"name":"*","path":"","rule":"set","safe":[0],"absentSafe":false,"fix":0,"max":1}],' + $noGate + '}'
$svcJson = '{"id":"services.legacy_remote","source":"LegacyServices","dynamic":true,"reboot":false,"keys":[{"name":"*","path":"","rule":"set","safe":[3,4],"absentSafe":false,"fix":4,"max":13}],' + $noGate + '}'
$pauseKeys = @('PauseUpdatesExpiryTime', 'PauseFeatureUpdatesEndTime', 'PauseQualityUpdatesEndTime', 'PauseFeatureUpdatesStartTime', 'PauseQualityUpdatesStartTime') | ForEach-Object {
    '{"name":"' + $_ + '","path":"HKLM:\\SOFTWARE\\Microsoft\\WindowsUpdate\\UX\\Settings","rule":"set","safe":[0],"absentSafe":false,"fix":0,"max":2000000000}'
}
$pauseJson = '{"id":"update.paused","source":"UpdatePause","dynamic":false,"reboot":false,"keys":[' + ($pauseKeys -join ',') + '],' + $noGate + '}'
$ssJson = '{"id":"smartscreen.apps","source":"SmartScreen","dynamic":false,"reboot":false,"keys":[{"name":"SmartScreenEnabled","path":"HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer","rule":"set","safe":[1,2,3],"absentSafe":true,"fix":1,"max":3},{"name":"EnableSmartScreen","path":"HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\System","rule":"set","safe":[1],"absentSafe":true,"fix":null,"max":1}],' + $noGate + '}'
function ValueKey($map) {
    $k = [pscustomobject]@{ Map = $map }
    $k | Add-Member ScriptMethod GetValueNames { return @($this.Map.Keys) }
    $k | Add-Member ScriptMethod GetValueKind {
        param($n)
        if ($this.Map[$n] -is [string]) { return [Microsoft.Win32.RegistryValueKind]::String }
        else { return [Microsoft.Win32.RegistryValueKind]::DWord }
    }
    $k | Add-Member ScriptMethod GetValue { param($n) return $this.Map[$n] }
    return $k
}
function CallLog() { return (($script:calls | ForEach-Object { $_ -join ':' }) -join ',') }

# ---- exclusions: only the risky list, never anything else
foreach ($risky in @('C:\', 'c:', 'C:\Windows', 'C:\Windows\', 'C:\Windows\Temp', 'C:\Users', 'C:\Users\Bob', 'C:\Users\Bob\Downloads', 'C:\Users\*\Downloads', '%USERPROFILE%\Downloads', '%TEMP%', 'D:\*', 'C:\Users\Bob\AppData\Local\Temp', 'C:\Program Files')) {
    Assert (HExclusionRisky 'path' $risky) "risky path: $risky"
}
foreach ($fine in @('C:\Games\Steam', 'D:\Projects\app', 'C:\Program Files\Foo', 'C:\Users\Bob\Documents\Work', 'C:\Users\Bob\Downloads\keep')) {
    Assert (!(HExclusionRisky 'path' $fine)) "ordinary path left alone: $fine"
}
foreach ($risky in @('exe', '.DLL', 'ps1', 'bat', 'js', 'vbs', 'scr')) { Assert (HExclusionRisky 'ext' $risky) "risky extension: $risky" }
foreach ($fine in @('log', 'txt', 'iso', 'pdf')) { Assert (!(HExclusionRisky 'ext' $fine)) "ordinary extension left alone: $fine" }
foreach ($risky in @('powershell.exe', 'C:\Windows\System32\cmd.exe', 'mshta', 'wscript.exe', 'pwsh.exe')) { Assert (HExclusionRisky 'proc' $risky) "risky process: $risky" }
foreach ($fine in @('steam.exe', 'C:\Games\game.exe')) { Assert (!(HExclusionRisky 'proc' $fine)) "ordinary process left alone: $fine" }
Assert (!(HExclusionRisky 'path' '') -and !(HExclusionRisky 'other' 'exe')) 'empty and unknown kinds are never risky'
MakeSpec $exclJson
foreach ($ok in @('path:C:\Users\Bob\Downloads', 'ext:exe', 'proc:cmd')) { Assert (HNameOk $ok) "exclusion name $ok" }
foreach ($bad in @('path:D:\Games', 'ext:log', 'file:C:\x', 'ext:', 'PATH:C:\Windows', ' path:C:\Windows', "ext:ex`te", 'path:C:\"x', '')) { Assert (!(HNameOk $bad)) "exclusion name accepted: $bad" }

# ---- exclusions: reader keeps only risky entries; removed ones read as 0
function Load($name) {}
$script:hWanted = @{}
$script:mp = [pscustomobject]@{ ExclusionPath = @('C:\Users\Bob\Downloads', 'D:\Games'); ExclusionExtension = @('exe', 'log'); ExclusionProcess = $null }
function Get-MpPreference { return $script:mp }
$r = HReadExclusions
Assert ($r.Count -eq 2 -and $r['path:C:\Users\Bob\Downloads'] -eq 1 -and $r['ext:exe'] -eq 1) 'only risky exclusions are listed'
$script:hWanted = @{ 'ext:dll' = 1; 'ext:exe' = 0 }
$r = HReadExclusions
Assert ($r['ext:dll'] -eq 0 -and $r['ext:exe'] -eq 1) 'a removed exclusion reads as 0'
$script:mp = [pscustomobject]@{ ExclusionPath = 'N/A: Must be an administrator to view exclusions'; ExclusionExtension = $null; ExclusionProcess = $null }
Reject { HReadExclusions } 'not readable'
$script:hWanted = @{}
# Setters touch only risky entries and use the typed parameters.
$script:exPaths = @('C:\Users\Bob\Downloads', 'D:\Games')
function Add-MpPreference { param($ExclusionPath, $ExclusionExtension, $ExclusionProcess); if ($ExclusionPath) { $script:exPaths = @($script:exPaths) + @($ExclusionPath) } }
function Remove-MpPreference { param($ExclusionPath, $ExclusionExtension, $ExclusionProcess); if ($ExclusionPath) { $script:exPaths = @($script:exPaths | Where-Object { $_ -ne $ExclusionPath }) } }
HSetExclusion 'path:C:\Users\Bob\Downloads' 0
Assert (@($script:exPaths).Count -eq 1 -and $script:exPaths[0] -ceq 'D:\Games') 'risky exclusion removed, the other one untouched'
HSetExclusion 'path:C:\Users\Bob\Downloads' 1
Assert (@($script:exPaths) -contains 'C:\Users\Bob\Downloads') 'undo adds the exclusion back'
Reject { HSetExclusion 'path:D:\Games' 0 } 'Unknown hardening item'
Reject { HSetExclusion 'path:C:\Windows' $null } 'Invalid exclusion state'
# Full write: remove then undo, through the real reader (hint keeps removed names visible).
${function:HRead} = $realHRead
function HSet([string]$name, $v) { HSetExclusion $name $v }
$script:preflightFails = $false; $script:blocked = $false
$script:mp = $null
function Get-MpPreference { return [pscustomobject]@{ ExclusionPath = $script:exPaths; ExclusionExtension = $null; ExclusionProcess = $null } }
$script:exPaths = @('C:\Users\Bob\Downloads', 'D:\Games')
HWrite (Input '{"items":{"path:C:\\Users\\Bob\\Downloads":0}}')
Assert (@($script:exPaths).Count -eq 1 -and $script:exPaths[0] -ceq 'D:\Games') 'exclusion fix verified through the reader'
HWrite (Input '{"items":{"path:C:\\Users\\Bob\\Downloads":1}}')
Assert (@($script:exPaths) -contains 'C:\Users\Bob\Downloads' -and @($script:exPaths).Count -eq 2) 'exclusion undo verified through the reader'
Reject { HWrite (Input '{"items":{"path:D:\\Games":0}}') } 'Unknown hardening item'

# ---- legacy services
MakeSpec $svcJson
foreach ($ok in @('RemoteRegistry', 'WinRM', 'sshd', 'TlntSvr', 'FTPSVC', 'W3SVC', 'SNMP')) { Assert (HNameOk $ok) "service $ok" }
foreach ($bad in @('Spooler', 'winrm', 'WinRM ', "WinRM'; calc", '')) { Assert (!(HNameOk $bad)) "service name accepted: $bad" }
Reject { HServiceKey 'Spooler' } 'Unknown hardening item'
$script:calls = @(); $script:svcStatus = 'Running'
function Get-Service { param($Name, $ErrorAction); $o = [pscustomobject]@{ Status = $script:svcStatus }; $o | Add-Member ScriptMethod WaitForStatus { param($s, $t) }; $o | Add-Member ScriptMethod Refresh { }; return $o }
function Stop-Service { param($Name, $ErrorAction); $script:calls += ,@('stop', $Name); $script:svcStatus = 'Stopped' }
function Start-Service { param($Name, $ErrorAction); $script:calls += ,@('start', $Name); $script:svcStatus = 'Running' }
function Set-Service { param($Name, $StartupType, $ErrorAction); $script:calls += ,@('type', $Name, $StartupType) }
function New-ItemProperty { param($LiteralPath, $Name, $PropertyType, $Value, [switch]$Force, $ErrorAction); $script:calls += ,@('reg', $Name, $Value) }
HSetService 'WinRM' 4
Assert ((CallLog) -ceq 'stop:WinRM,type:WinRM:Disabled') "service stop and disable: $(CallLog)"
$script:calls = @(); $script:svcStatus = 'Stopped'
HSetService 'WinRM' 10
Assert ((CallLog) -ceq 'type:WinRM:Automatic,reg:DelayedAutostart:0,start:WinRM') "service restore automatic running: $(CallLog)"
$script:calls = @(); $script:svcStatus = 'Stopped'
HSetService 'sshd' 13
Assert ((CallLog) -ceq 'type:sshd:Automatic,reg:DelayedAutostart:1,start:sshd') "service restore delayed running: $(CallLog)"
$script:calls = @(); $script:svcStatus = 'Stopped'
HSetService 'SNMP' 3
Assert ((CallLog) -ceq 'type:SNMP:Manual') "service restore manual stopped: $(CallLog)"
Reject { HSetService 'Spooler' 4 } 'Unknown hardening item'
Reject { HSetService 'WinRM' 1 } 'Invalid service start type'

# ---- exploit protection states
Assert ((HMitigationValue 'ON') -eq 1 -and (HMitigationValue 'off') -eq 0 -and (HMitigationValue 'NOTSET') -eq 2) 'mitigation words'
Reject { HMitigationValue 'maybe' } 'not readable'
function Get-ProcessMitigation { param([switch]$System); return [pscustomobject]@{ DEP = [pscustomobject]@{ Enable = 'ON' }; SEHOP = [pscustomobject]@{ Enable = 'NOTSET' }; ASLR = [pscustomobject]@{ BottomUp = 'OFF'; HighEntropy = 'ON' }; CFG = [pscustomobject]@{ Enable = 'ON' } } }
$r = HReadMitigations
Assert ($r['DEP'] -eq 1 -and $r['SEHOP'] -eq 2 -and $r['BottomUp'] -eq 0 -and $r['HighEntropy'] -eq 1 -and $r['CFG'] -eq 1) 'mitigation slice'
$script:calls = @()
function Set-ProcessMitigation { param([switch]$System, $Enable, $Disable, $ErrorAction); if ($Enable) { $script:calls += ,@('on', $Enable) } else { $script:calls += ,@('off', $Disable) } }
HSetMitigation 'BottomUp' 1; HSetMitigation 'CFG' 0
Assert ((CallLog) -ceq 'on:BottomUp,off:CFG') "mitigation setters: $(CallLog)"
Reject { HSetMitigation 'ForceRelocateImages' 1 } 'Unknown hardening item'
Reject { HSetMitigation 'DEP' 2 } 'Invalid exploit protection state'

# ---- Windows optional features (a missing feature counts as removed)
Assert ((HV2Value 'Enabled') -eq 1 -and (HV2Value 'EnablePending') -eq 1 -and (HV2Value 'Disabled') -eq 0 -and (HV2Value 'DisabledWithPayloadRemoved') -eq 0 -and (HV2Value 'Missing') -eq 0) 'feature state words'
Reject { HV2Value 'Weird' } 'not readable'
$script:features = @(1..6 | ForEach-Object { [pscustomobject]@{ FeatureName = "Filler$_"; State = 'Enabled' } }) + @([pscustomobject]@{ FeatureName = 'MicrosoftWindowsPowerShellV2Root'; State = 'Enabled' })
function Get-CimInstance {
    param($ClassName, $Filter, $OperationTimeoutSec)
    if ($ClassName -cne 'Win32_OptionalFeature') { throw "Unexpected probe $ClassName" }
    $rows = @($script:features | ForEach-Object { [pscustomobject]@{ Name = $_.FeatureName; InstallState = [uint32]$(switch ($_.State) { 'Enabled' { 1 } 'Disabled' { 2 } default { 3 } }) } })
    if (!$Filter) { return $rows }
    if ($Filter -cnotmatch "^Name='([A-Za-z0-9-]+)'$") { throw 'Unexpected feature filter' }
    $name = $Matches[1]
    return @($rows | Where-Object { $_.Name -ceq $name })
}
Assert ((HFeatureState 'MicrosoftWindowsPowerShellV2Root') -ceq 'Enabled') 'feature state'
Assert ((HFeatureState 'Recall') -ceq 'Missing') 'unknown feature in a healthy list is missing'
Assert ((HReadPowerShellV2)['Enabled'] -eq 1) 'root enabled'
$script:features = @(1..6 | ForEach-Object { [pscustomobject]@{ FeatureName = "Filler$_"; State = 'Enabled' } })
Assert ((HReadPowerShellV2)['Enabled'] -eq 0) 'missing feature is protected'
$script:features = @()
Reject { HReadPowerShellV2 } 'not readable'
$script:features = @(1..6 | ForEach-Object { [pscustomobject]@{ FeatureName = "Filler$_"; State = 'Enabled' } }) + @([pscustomobject]@{ FeatureName = 'MicrosoftWindowsPowerShellV2Root'; State = 'Disabled' })
Assert ((HReadPowerShellV2)['Enabled'] -eq 0) 'disabled root'
Reject { HFeatureState "x' or Name like '%" } 'Invalid feature name'
$script:features = @(1..6 | ForEach-Object { [pscustomobject]@{ FeatureName = "Filler$_"; State = 'Enabled' } }) + @([pscustomobject]@{ FeatureName = 'MicrosoftWindowsPowerShellV2Root'; State = 'DisabledWithPayloadRemoved' })
Assert ((HReadPowerShellV2)['Enabled'] -eq 0) 'payload removed is off'
$script:features = @(1..6 | ForEach-Object { [pscustomobject]@{ FeatureName = "Filler$_"; State = 'Enabled' } }) + @([pscustomobject]@{ FeatureName = 'MicrosoftWindowsPowerShellV2Root'; State = 'Disabled' })
$script:calls = @()
function Disable-WindowsOptionalFeature { param([switch]$Online, $FeatureName, [switch]$NoRestart, $ErrorAction); $script:calls += ,@('disable', $FeatureName) }
function Enable-WindowsOptionalFeature { param([switch]$Online, $FeatureName, [switch]$All, [switch]$NoRestart, $ErrorAction); $script:calls += ,@('enable', $FeatureName) }
HSetPowerShellV2 0
Assert ((CallLog) -ceq '') 'nothing to disable when already disabled'
HSetPowerShellV2 1
Assert ((CallLog) -ceq 'enable:MicrosoftWindowsPowerShellV2Root,enable:MicrosoftWindowsPowerShellV2') "feature undo: $(CallLog)"

# ---- sign-in on wake (power WMI provider)
$planId = '381b4222-f694-41f0-9685-ff5bb260df2e'
$script:power = @{ ac = 0; dc = 0; hasDc = $true }
function Get-CimInstance {
    param($ClassName, $Namespace)
    switch ($ClassName) {
        'Win32_PowerPlan' { return @([pscustomobject]@{ IsActive = $false; InstanceID = 'Microsoft:PowerPlan\{aaaaaaaa-0000-0000-0000-000000000000}' }, [pscustomobject]@{ IsActive = $true; InstanceID = "Microsoft:PowerPlan\{$planId}" }) }
        'Win32_PowerSettingDataIndex' {
            $list = @([pscustomobject]@{ InstanceID = "Microsoft:PowerSettingDataIndex\{$planId}\AC\{11111111-1111-1111-1111-111111111111}"; SettingIndexValue = [uint32]7 })
            $list += [pscustomobject]@{ InstanceID = "Microsoft:PowerSettingDataIndex\{$planId}\AC\{0e796bdb-100d-47d6-a2d5-f7d2daa51f51}"; SettingIndexValue = [uint32]$script:power.ac }
            if ($script:power.hasDc) { $list += [pscustomobject]@{ InstanceID = "Microsoft:PowerSettingDataIndex\{$planId}\DC\{0E796BDB-100D-47D6-A2D5-F7D2DAA51F51}"; SettingIndexValue = [uint32]$script:power.dc } }
            return $list
        }
        default { throw "Unexpected probe $ClassName" }
    }
}
$r = HReadLockOnWake
Assert ($r['Ac'] -eq 0 -and $r['Dc'] -eq 0) 'wake lock slice'
$script:power = @{ ac = 1; dc = 0; hasDc = $true }
$r = HReadLockOnWake
Assert ($r['Ac'] -eq 1 -and $r['Dc'] -eq 0) 'wake lock slice ac on'
$script:power = @{ ac = 1; dc = 0; hasDc = $false }
Assert ((HReadLockOnWake)['Dc'] -eq 1) 'a missing battery setting mirrors the plugged-in one'
$script:power = @{ ac = 5; dc = 0; hasDc = $true }
Reject { HReadLockOnWake } 'not readable'
$script:calls = @()
function Set-CimInstance { param($InputObject, $ErrorAction); $script:calls += ,@('set', [string]$InputObject.SettingIndexValue) }
function Invoke-CimMethod { param($InputObject, $MethodName, $ErrorAction); $script:calls += ,@('invoke', $MethodName) }
$script:power = @{ ac = 0; dc = 0; hasDc = $true }
HSetLockOnWake 'Dc' 1
Assert ((CallLog) -ceq 'set:1,invoke:Activate') "wake lock write: $(CallLog)"
Reject { HSetLockOnWake 'Other' 1 } 'Unknown hardening item'
Reject { HSetLockOnWake 'Ac' 2 } 'Invalid sign-in-on-wake value'

# ---- paused updates (minutes since 1970; expired pauses are not "paused")
MakeSpec $pauseJson
$script:fakeFs = $true
$script:fakePaths = @('HKLM:\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings')
$ts = "yyyy-MM-dd'T'HH:mm:ss'Z'"
$future = [DateTimeOffset]::UtcNow.AddDays(10).UtcDateTime.ToString($ts, [Globalization.CultureInfo]::InvariantCulture)
$past = [DateTimeOffset]::UtcNow.AddDays(-10).UtcDateTime.ToString($ts, [Globalization.CultureInfo]::InvariantCulture)
$nowSeconds = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
$script:fakeKey = ValueKey @{ PauseUpdatesExpiryTime = $future; PauseFeatureUpdatesEndTime = $future; PauseFeatureUpdatesStartTime = $past }
$r = HReadPause
Assert (($r['PauseUpdatesExpiryTime'] * 60) -gt $nowSeconds -and $r['PauseQualityUpdatesEndTime'] -eq 0 -and $r['PauseFeatureUpdatesStartTime'] -gt 0) 'active pause lists every saved time'
Assert (HAnyUnsafe $r) 'an active pause is unsafe'
$script:fakeKey = ValueKey @{ PauseUpdatesExpiryTime = $past; PauseFeatureUpdatesStartTime = $past }
$r = HReadPause
Assert (!(HAnyUnsafe $r) -and $r['PauseFeatureUpdatesStartTime'] -eq 0) 'an expired pause is not paused'
$script:fakeKey = ValueKey @{ PauseUpdatesExpiryTime = 'not a date' }
Reject { HReadPause } 'is not a date'
$script:fakeKey = ValueKey @{ PauseUpdatesExpiryTime = 5 }
Reject { HReadPause } 'is not text'
$script:fakePaths = @()
Assert (!(HAnyUnsafe (HReadPause))) 'no settings key means no pause'
# Writers: remove, and put a saved time back to the minute.
$script:fakePaths = @('HKLM:\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings')
$script:fakeKey = ValueKey @{ PauseUpdatesExpiryTime = $future }
$script:calls = @()
function Remove-ItemProperty { param($LiteralPath, $Name, $ErrorAction); $script:calls += ,@('remove', $Name) }
$pauseDef = HDef 'PauseUpdatesExpiryTime'
HSetPause $pauseDef 0
Assert ((CallLog) -ceq 'remove:PauseUpdatesExpiryTime') "pause removed: $(CallLog)"
$script:calls = @()
$minutes = [int64][Math]::Floor(([DateTimeOffset]::UtcNow.AddDays(10).ToUnixTimeSeconds()) / 60)
$script:hWanted = @{ PauseUpdatesExpiryTime = $minutes }
HSetPause $pauseDef $minutes
Assert ($script:calls.Count -eq 1 -and $script:calls[0][0] -ceq 'reg' -and $script:calls[0][2] -cmatch '^\d{4}-\d\d-\d\dT\d\d:\d\d:00Z$') "pause restored in Windows' own format: $(CallLog)"
$script:calls = @()
$script:hWanted = @{ PauseUpdatesExpiryTime = 100 }
HSetPause $pauseDef 100
Assert ($script:calls.Count -eq 0) 'a pause that already ended is not written back'
Assert (HVerified 'PauseUpdatesExpiryTime' 0 100) 'an ended pause may read back as nothing'
$script:hWanted = @{ PauseUpdatesExpiryTime = $minutes }
Assert (!(HVerified 'PauseUpdatesExpiryTime' 0 $minutes)) 'a pause still in force must read back'
$script:hWanted = @{}
$script:fakeFs = $false

# ---- SmartScreen (string setting plus the policy value)
MakeSpec $ssJson
$script:fakeFs = $true
$script:fakePaths = @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer', 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\System')
$script:fakeKey = ValueKey @{ SmartScreenEnabled = 'Off'; EnableSmartScreen = 0 }
$r = HReadSmartScreen
Assert ($r['SmartScreenEnabled'] -eq 0 -and $r['EnableSmartScreen'] -eq 0 -and (HAnyUnsafe $r)) 'smartscreen off by setting and by policy'
$script:fakeKey = ValueKey @{ SmartScreenEnabled = 'warn' }
$r = HReadSmartScreen
Assert ($r['SmartScreenEnabled'] -eq 1 -and $null -eq $r['EnableSmartScreen'] -and !(HAnyUnsafe $r)) 'warn is protected and an absent policy is protected'
$script:fakeKey = ValueKey @{ RequireAdminOnly = 1 }
$r = HReadSmartScreen
Assert ($null -eq $r['SmartScreenEnabled'] -and !(HAnyUnsafe $r)) 'absent setting is the Windows default'
$script:fakeKey = ValueKey @{ SmartScreenEnabled = 'Strange' }
Reject { HReadSmartScreen } 'not recognised'
$script:fakeKey = ValueKey @{ SmartScreenEnabled = 1 }
Reject { HReadSmartScreen } 'not text'
$script:fakeFs = $false
$script:calls = @()
function Test-Path { param($LiteralPath, $ErrorAction); return $true }
HSetSmartScreen (HDef 'SmartScreenEnabled') 1
HSetSmartScreen (HDef 'SmartScreenEnabled') 0
Assert ((CallLog) -ceq 'reg:SmartScreenEnabled:Warn,reg:SmartScreenEnabled:Off') "smartscreen words: $(CallLog)"
Reject { HSetSmartScreen (HDef 'SmartScreenEnabled') 7 } 'Invalid SmartScreen setting'
$script:calls = @()
HSetSmartScreen (HDef 'EnableSmartScreen') $null
Assert ((CallLog) -ceq 'remove:EnableSmartScreen') "policy value removed: $(CallLog)"

# ---- handled-item controls (hardening.handled.ps1)
$gateJson = '"gate":{"areas":[],"pattern":".","tamperExempt":false,"secedit":false,"ownPolicyKey":"","policyValues":[]}'
function HandledJson([string]$id, [string]$source) { return ('{"id":"' + $id + '","source":"' + $source + '","dynamic":true,"reboot":false,"keys":[{"name":"*","path":"","rule":"set","safe":[0,2],"absentSafe":false,"fix":0,"max":2}],' + $gateJson + '}') }
MakeSpec (HandledJson 'services.unquoted_paths' 'UnquotedServices')
foreach ($ok in @('Spooler','My Service','svc.name$1','Intel(R) Update {1}+x','App#2,b&c')) { Assert (HNameOk $ok) "service name $ok" }
foreach ($bad in @('','a"b',' lead','trail ','x/y','x\y','a*b','a?b','a[1]',("t`tab"),('s' * 257))) { Assert (!(HNameOk $bad)) "service name accepted: $bad" }
Assert (HNameOk ('s' * 256)) 'a 256 character service name is accepted'
MakeSpec (HandledJson 'firewall.user_dir_inbound_allow' 'UserDirFirewall')
Assert (HNameOk 'My app (inbound)') 'firewall rule name'
foreach ($bad in @('','a"b','wild*card','x[1]',' pad',("t`tab"),('r' * 201))) { Assert (!(HNameOk $bad)) "firewall name accepted: $bad" }
MakeSpec (HandledJson 'net.hosts_file' 'HostsFile')
Assert ((HNameOk 'hosts') -and !(HNameOk 'Hosts') -and !(HNameOk 'hosts2') -and !(HNameOk '')) 'hosts name is exactly hosts'
MakeSpec (HandledJson 'persistence.run_and_tasks' 'StartupItems')
foreach ($ok in @('run-machine:Updater','run-user:My App','folder-user:a.lnk','task:\Vendor\Sync')) { Assert (HNameOk $ok) "startup name $ok" }
foreach ($bad in @('Updater','run-other:x','run-user:','task:Vendor\x','task:\Vendor\','run-user:a*b','run-user:a"b','run-user: x',('run-user:' + ('n' * 260)))) { Assert (!(HNameOk $bad)) "startup name accepted: $bad" }

# hosts lines: the same flags as the Tools security check
foreach ($flagged in @('0.0.0.0 windowsupdate.com','127.0.0.1 update.microsoft.com','1.2.3.4 www.paypal.com','10.0.0.9   login.microsoft.com # note')) { Assert (HHostsLineFlag $flagged) "hosts line should be flagged: $flagged" }
foreach ($fine in @('127.0.0.1 localhost','::1 localhost','# 1.2.3.4 paypal.com','','   ','1.2.3.4','not-an-ip paypal.com','192.168.1.5 printer.lan')) { Assert (!(HHostsLineFlag $fine)) "hosts line should be left alone: $fine" }

# HHostsFix touches only flagged lines and keeps every other byte, line ending and BOM
$latin = [Text.Encoding]::GetEncoding(28591)
$original = $latin.GetBytes("# my notes`r`n127.0.0.1 localhost`r`n1.2.3.4 www.paypal.com`r`n`r`n192.168.1.5 printer.lan   # keep`r`n0.0.0.0 windowsupdate.com")
$fixedBytes = HHostsFix $original
$fixedText = $latin.GetString($fixedBytes)
Assert ($fixedText -ceq "# my notes`r`n127.0.0.1 localhost`r`n# turned off by Secblitz 1.2.3.4 www.paypal.com`r`n`r`n192.168.1.5 printer.lan   # keep`r`n# turned off by Secblitz 0.0.0.0 windowsupdate.com") "hosts fix text: $fixedText"
Assert (@(HHostsFlaggedLines $fixedBytes).Count -eq 0) 'a fixed hosts file has nothing flagged'
Assert (@(HHostsFlaggedLines $original).Count -eq 2) 'two flagged lines found'
$unmarked = $fixedText.Replace('# turned off by Secblitz ', '')
Assert ($unmarked -ceq $latin.GetString($original)) 'removing the note gives back the original text'
$bomBytes = [byte[]](@(0xEF, 0xBB, 0xBF) + @($latin.GetBytes("1.2.3.4 www.paypal.com`n")))
$bomFixed = HHostsFix $bomBytes
Assert ($bomFixed[0] -eq 0xEF -and $bomFixed[1] -eq 0xBB -and $bomFixed[2] -eq 0xBF -and $latin.GetString($bomFixed, 3, $bomFixed.Length - 3) -ceq "# turned off by Secblitz 1.2.3.4 www.paypal.com`n") 'BOM kept in front of the note'
Assert (HHostsPlain $original) 'plain text accepted'
Assert (!(HHostsPlain ([byte[]]@(0xFF, 0xFE, 0x31, 0x00)))) 'UTF-16 refused'
Assert (!(HHostsPlain ([byte[]]@(0x31, 0x00, 0x32)))) 'NUL bytes refused'
Assert (!(HHostsPlain $latin.GetBytes("a`rb`n"))) 'a lone carriage return is refused'
Assert (!(HHostsPlain $latin.GetBytes("a`r"))) 'a trailing lone carriage return is refused'
Assert (HHostsPlain $latin.GetBytes("a`r`nb`n")) 'CRLF and LF are accepted'
# a byte order mark only counts at the very start of the file
$midBom = [byte[]](@($latin.GetBytes("127.0.0.1 localhost`n")) + @(0xEF, 0xBB, 0xBF) + @($latin.GetBytes("1.2.3.4 www.paypal.com`n")))
Assert (@(HHostsFlaggedLines $midBom).Count -eq 0) 'a mid-file BOM line is not read as flagged'
Assert (@(HHostsFlaggedLines (HHostsFix $midBom)).Count -eq 0) 'fix and reader agree on a mid-file BOM line'

# hosts: read, fix, undo, and a change in between (state and file are doubles)
function HHostsBytes { if ($null -eq $script:hostsFile) { return $null }; return [byte[]]$script:hostsFile }
function HHostsWrite([byte[]]$bytes) { $script:hostsWrites++; if ($script:hostsFail) { throw 'fixture write failure' }; $script:hostsFile = [byte[]]$bytes }
function HFlushDns { $script:dnsFlushes++ }
function HHostsReadOnly { return [bool]$script:hostsRo }
function HHostsSetReadOnly([bool]$on) { $script:hostsRo = $on }
function HHostsFlaggedOther { return @('1.2.3.4 www.paypal.com') }
$script:hostsRo = $false
function HStateGet([string]$name) { if ($script:undoState.ContainsKey($name)) { return $script:undoState[$name] }; return $null }
function HStateSet([string]$name, $data) { $script:undoState[$name] = $data }
function HStateRemove([string]$name) { $script:undoState.Remove($name) }
$script:undoState = @{}; $script:hostsWrites = 0; $script:dnsFlushes = 0; $script:hostsFail = $false
$script:hostsFile = $original
$r = HReadHosts
Assert ($r['hosts'] -eq 1) 'flagged hosts file reads 1'
Assert ($script:hLabels.Count -gt 0) 'the flagged hosts lines are named'
HSetHosts 'hosts' 0
Assert ($latin.GetString($script:hostsFile) -ceq $fixedText) 'fix writes the commented file'
Assert ($script:dnsFlushes -eq 1) 'DNS cache flushed after the fix'
Assert ((HReadHosts)['hosts'] -eq 0) 'fixed and unchanged reads 0'
HSetHosts 'hosts' 1
Assert ([Convert]::ToBase64String($script:hostsFile) -ceq [Convert]::ToBase64String($original)) 'undo restores the exact original bytes'
Assert ($script:undoState.Count -eq 0 -and $script:dnsFlushes -eq 2) 'undo clears the saved state and flushes DNS'
Assert ((HReadHosts)['hosts'] -eq 1) 'after undo it is flagged again'
HSetHosts 'hosts' 0
$script:hostsFile = [byte[]](@($script:hostsFile) + @($latin.GetBytes("`r`n# someone else was here")))
Assert ((HReadHosts)['hosts'] -eq 2) 'a file changed since reads 2'
$before = [Convert]::ToBase64String($script:hostsFile)
Reject { HSetHosts 'hosts' 1 } 'changed again'
Assert ([Convert]::ToBase64String($script:hostsFile) -ceq $before) 'a changed hosts file is left alone on undo'
# a read-only hosts file: the mark is cleared for the change and put back by undo
$script:undoState = @{}; $script:hostsFile = $original; $script:hostsRo = $true
HSetHosts 'hosts' 0
Assert (!$script:hostsRo) 'the read-only mark is cleared for the change'
Assert ((HReadHosts)['hosts'] -eq 0) 'fixed and unlocked reads 0'
$script:hostsRo = $true
Assert ((HReadHosts)['hosts'] -eq 2) 'a read-only mark added since reads 2'
Reject { HSetHosts 'hosts' 1 } 'changed again'
$script:hostsRo = $false
HSetHosts 'hosts' 1
Assert ($script:hostsRo -and [Convert]::ToBase64String($script:hostsFile) -ceq [Convert]::ToBase64String($original)) 'undo restores the bytes and the read-only mark'
# UTF-16 is read the way the Tools check reads it: flagged, then Not offered
$script:undoState = @{}; $script:hostsRo = $false
$script:hostsFile = [byte[]]@(0xFF, 0xFE, 0x31, 0x00)
Assert ((HReadHosts)['hosts'] -eq 1) 'a UTF-16 hosts file with a redirect reads 1'
# a failed write is rolled back and leaves no saved state
$script:undoState = @{}; $script:hostsFile = $original; $script:hostsFail = $true
Reject { HSetHosts 'hosts' 0 } 'fixture write failure'
$script:hostsFail = $false
Assert ($script:undoState.Count -eq 0) 'a failed fix leaves no saved state'
$script:hostsFile = $latin.GetBytes("127.0.0.1 localhost`r`n")
Reject { HSetHosts 'hosts' 0 } 'no longer needs a change'
Reject { HSetHosts 'other' 0 } 'Invalid hosts file state'
Reject { HSetHosts 'hosts' 2 } 'Invalid hosts file state'
# preflight reasons
$script:hostsFile = $null
Reject { HHostsPreflight } 'Not offered: the hosts file could not be found'
$script:hostsFile = [byte[]](New-Object byte[] ($hHostsMaxBytes + 1))
Reject { HHostsPreflight } 'Not offered: the hosts file is too large'
$script:hostsFile = [byte[]]@(0xFF, 0xFE, 0x31, 0x00)
Reject { HHostsPreflight } 'Not offered: the hosts file uses a format we cannot keep exactly'

# start-up items: Task Manager's on/off record
Assert (HApprovedEnabled $null) 'no record means on'
Assert (HApprovedEnabled ([byte[]]@(2,0,0,0,0,0,0,0,0,0,0,0))) 'even first byte means on'
Assert (!(HApprovedEnabled ([byte[]]@(3,0,0,0,0,0,0,0,0,0,0,0)))) 'odd first byte means off'
Assert (!(HApprovedEnabled ([byte[]]@(2,0)))) 'a short record is treated as off, never overwritten'
Assert (HIsUserKind 'run-user' -and HIsUserKind 'folder-user' -and !(HIsUserKind 'run-machine') -and !(HIsUserKind 'folder-machine')) 'only per-user kinds depend on who is signed in'

Write-Output "Hardening PowerShell fixtures passed: $script:checks checks"
