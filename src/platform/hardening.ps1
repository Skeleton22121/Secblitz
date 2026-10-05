# Extended hardening controls. Appended to the backend definitions (never the
# backend dispatcher) for ids in the compiled Rust catalog (src/hardening.rs).
# $hardeningSpecJson is produced by Rust from that catalog: safe values, repair
# values, registry paths and gate requirements live in ONE place. All state is a
# slice {items: {key: int|null}}; null means "not configured".
#
# Writes can only move a key between its recorded unsafe original and the fixed
# value (HFixOf); anything else means the setting changed behind us and stops.
# No native child processes are started (the launcher job forbids them).
$spec = ConvertFrom-Json -InputObject $hardeningSpecJson

function HEq($a, $b) {
    if ($null -eq $a -or $null -eq $b) { return ($null -eq $a -and $null -eq $b) }
    return ([int64]$a -eq [int64]$b)
}
function HDef([string]$name) {
    if ($spec.dynamic) { return $spec.keys[0] }
    foreach ($k in @($spec.keys)) { if ($k.name -ceq $name) { return $k } }
    throw 'Unknown hardening item'
}
function HNameOk([string]$name) {
    if (!$spec.dynamic) { foreach ($k in @($spec.keys)) { if ($k.name -ceq $name) { return $true } }; return $false }
    if ($spec.source -ceq 'FirewallExposure') { return ($name -cmatch '^(FPS|NETDIS)-[A-Za-z0-9_.-]{1,92}$') }
    return ($name.Length -ge 1 -and $name.Length -le 64 -and $name -cnotmatch '[\x00-\x1f\x7f"]' -and $name.Trim() -ceq $name)
}
function HIsSafe($def, $v) {
    if ($def.rule -ceq 'exposure') {
        if ($null -eq $v) { return $false }
        return !((([int]$v -band 8) -ne 0) -and (([int]$v -band 4) -ne 0))
    }
    if ($null -eq $v) { return [bool]$def.absentSafe }
    return (@($def.safe) -contains [int]$v)
}
function HFixOf($def, $v) {
    if (HIsSafe $def $v) { return $v }
    if ($def.rule -ceq 'exposure') {
        if ($null -eq $v) { return $null }
        if (([int]$v -band 3) -eq 0) { return ([int]$v -band (-bnot 8)) }
        return ([int]$v -band (-bnot 4))
    }
    return $def.fix
}
function HAnyUnsafe($slice) {
    foreach ($name in @($slice.Keys)) { if (!(HIsSafe (HDef $name) $slice[$name])) { return $true } }
    return $false
}

# ---------------------------------------------------------------- readers
function HReadRegistry($def) {
    if (!(Test-Path -LiteralPath $def.path -ErrorAction Stop)) { return $null }
    $key = Get-Item -LiteralPath $def.path -ErrorAction Stop
    if ($key.GetValueNames() -notcontains $def.name) { return $null }
    if ($key.GetValueKind($def.name) -ne [Microsoft.Win32.RegistryValueKind]::DWord) { throw "$($def.name) is not a DWORD" }
    return [int]$key.GetValue($def.name)
}
$hMaps = @('Disabled','Basic','Advanced')
$hPua = @('Disabled','Enabled','AuditMode')
function HEnumNumber($v, [string[]]$names) {
    if ($null -eq $v) { throw 'Defender preference is not readable' }
    if ($v -is [string]) {
        $i = [array]::IndexOf($names, $v)
        if ($i -lt 0) { throw 'Defender preference is not readable' }
        return $i
    }
    return [int]$v
}
function HReadDefenderPref() {
    Load 'Defender'
    $p = Get-MpPreference
    $out = @{}
    foreach ($def in @($spec.keys)) {
        $raw = $p.($def.name)
        if ($def.name -ceq 'MAPSReporting') { $out[$def.name] = HEnumNumber $raw $hMaps }
        elseif ($def.name -ceq 'PUAProtection') { $out[$def.name] = HEnumNumber $raw $hPua }
        else {
            # Disable* preferences: absent means Defender's default (feature on).
            if ($null -eq $raw) { $out[$def.name] = 0 }
            elseif ($raw -isnot [bool]) { throw 'Defender preference is not a readable boolean' }
            else { $out[$def.name] = [int]$raw }
        }
    }
    return $out
}
function HReadAsr() {
    Load 'Defender'
    $p = Get-MpPreference
    $ids = @($p.AttackSurfaceReductionRules_Ids | Where-Object { $null -ne $_ })
    $acts = @($p.AttackSurfaceReductionRules_Actions | Where-Object { $null -ne $_ })
    if ($ids.Count -ne $acts.Count) { throw 'Attack surface rule lists are inconsistent' }
    $out = @{}
    foreach ($def in @($spec.keys)) { $out[$def.name] = $null }
    for ($i = 0; $i -lt $ids.Count; $i++) {
        $guid = ([string]$ids[$i]).ToLowerInvariant()
        if ($out.ContainsKey($guid)) { $out[$guid] = [int]$acts[$i] }
    }
    return $out
}
function HAdsiPolicy() {
    $d = [ADSI]('WinNT://' + [Environment]::MachineName)
    if ([string]$d.SchemaClassName -cne 'Domain') { throw 'Local account policy is not readable' }
    return $d
}
function HReadLockout() {
    $d = HAdsiPolicy
    $n = $d.Properties['MaxBadPasswordsAllowed'].Value
    if ($null -eq $n -or $n -isnot [int]) { throw 'Lockout threshold is not readable' }
    return @{ LockoutThreshold = [int]$n }
}
function HBuiltinAdmin() {
    Load 'Microsoft.PowerShell.LocalAccounts'
    $found = @(Get-LocalUser | Where-Object { $_.SID.Value -cmatch '^S-1-5-21-[0-9-]+-500$' })
    if ($found.Count -ne 1) { throw 'Built-in Administrator account cannot be identified' }
    return $found[0]
}
function HReadBuiltinAdmin() {
    $a = HBuiltinAdmin
    if ($a.Enabled -isnot [bool]) { throw 'Administrator state is not readable' }
    return @{ Enabled = [int]$a.Enabled }
}
function HReadFirewall() {
    Load 'NetSecurity'
    $out = @{}
    $rules = @()
    foreach ($pattern in @('FPS-*','NETDIS-*')) {
        # A pattern with no match is an empty result, not a failure.
        try { $rules += @(Get-NetFirewallRule -PolicyStore PersistentStore -Name $pattern -ErrorAction Stop) }
        catch { if ($_.CategoryInfo.Category -ne 'ObjectNotFound') { throw } }
    }
    foreach ($r in $rules) {
        $name = [string]$r.Name
        if (!(HNameOk $name)) { continue }
        if ([string]$r.Direction -cne 'Inbound' -or [string]$r.Action -cne 'Allow') { continue }
        $bits = [int]$r.Profile -band 7
        if ($bits -eq 0) { $bits = 7 }
        $enabled = [string]$r.Enabled -ceq 'True'
        if ([string]$r.Enabled -cnotin @('True','False')) { continue }
        $out[$name] = $bits + $(if ($enabled) { 8 } else { 0 })
    }
    return $out
}
function HWlanRoot() {
    return [IO.Path]::Combine([Environment]::GetFolderPath('CommonApplicationData'), 'Microsoft\Wlansvc\Profiles\Interfaces')
}
function HLoadXml([string]$path) {
    # DTDs prohibited and no resolver: profile files are data, never instructions.
    $settings = [Xml.XmlReaderSettings]::new()
    $settings.DtdProcessing = [Xml.DtdProcessing]::Prohibit
    $settings.XmlResolver = $null
    $reader = [Xml.XmlReader]::Create($path, $settings)
    try { $doc = [Xml.XmlDocument]::new(); $doc.Load($reader) } finally { $reader.Dispose() }
    return $doc
}
function HXmlText($doc, [string]$name) {
    $node = $doc.SelectSingleNode("//*[local-name()='$name']")
    if ($null -eq $node) { return '' }
    return [string]$node.InnerText
}
function HWlanProfiles() {
    # Saved all-user profiles with a weak or no security type. Reads profile
    # files only (never key material) so no external process is needed.
    $root = HWlanRoot
    $list = @()
    if (!(Test-Path -LiteralPath $root)) { return $list }
    foreach ($dir in @(Get-ChildItem -LiteralPath $root -Directory)) {
        $iface = [Guid]::Empty
        if (![Guid]::TryParse($dir.Name, [ref]$iface)) { continue }
        foreach ($file in @(Get-ChildItem -LiteralPath $dir.FullName -Filter '*.xml' -File)) {
            $doc = HLoadXml $file.FullName
            $name = HXmlText $doc 'name'
            $mode = HXmlText $doc 'connectionMode'
            $auth = HXmlText $doc 'authentication'
            $enc = HXmlText $doc 'encryption'
            $oneX = HXmlText $doc 'useOneX'
            if (!(HNameOk $name) -or $mode -cnotin @('auto','manual') -or $oneX -ceq 'true') { continue }
            $risky = ($auth -ceq 'open' -and $enc -ceq 'none') -or ($enc -ceq 'WEP') -or ($auth -ceq 'shared') -or
                (($auth -ceq 'WPAPSK' -or $auth -ceq 'WPA') -and $enc -ceq 'TKIP')
            if ($risky) { $list += @{ name = $name; value = $(if ($mode -ceq 'auto') { 1 } else { 0 }); iface = $iface; file = $file.FullName } }
        }
    }
    return $list
}
function HReadWifi() {
    $out = @{}
    $dupes = @{}
    foreach ($p in @(HWlanProfiles)) {
        if ($out.ContainsKey($p.name) -or $dupes.ContainsKey($p.name)) { $dupes[$p.name] = $true; $out.Remove($p.name); continue }
        $out[$p.name] = $p.value
    }
    return $out
}
function HRead() {
    switch -CaseSensitive ($spec.source) {
        'Registry' { $out = @{}; foreach ($def in @($spec.keys)) { $out[$def.name] = HReadRegistry $def }; return $out }
        'DefenderPref' { return (HReadDefenderPref) }
        'DefenderAsr' { return (HReadAsr) }
        'Lockout' { return (HReadLockout) }
        'BuiltinAdmin' { return (HReadBuiltinAdmin) }
        'FirewallExposure' { return (HReadFirewall) }
        'WifiProfiles' { return (HReadWifi) }
    }
    throw 'Unknown hardening source'
}

# ------------------------------------------------------------------ gates
function HGateCommon() {
    Load 'CimCmdlets'
    $os = Get-CimInstance Win32_OperatingSystem
    if ($os.ProductType -ne 1 -or [int]$os.BuildNumber -lt 10240 -or ![Environment]::Is64BitProcess) { throw 'Unsupported Windows client capability' }
    $cs = Get-CimInstance Win32_ComputerSystem
    if ($cs.PartOfDomain -isnot [bool]) { throw 'Domain membership is not readable' }
    if ($cs.PartOfDomain) { ThrowGate 'Domain-managed machine: assessment only' }
    if (MdmRegistered) { ThrowGate 'Device is registered with MDM: assessment only' }
    foreach ($p in @('HKLM:\SOFTWARE\Microsoft\Provisioning\OMADM\Accounts','HKLM:\SYSTEM\CurrentControlSet\Control\CloudDomainJoin\JoinInfo')) {
        if ((Test-Path -LiteralPath $p) -and @(Get-ChildItem -LiteralPath $p).Count -gt 0) { ThrowGate 'Enrollment or cloud-management evidence: assessment only' }
    }
}
function HRegistryPaths() {
    $paths = @()
    foreach ($def in @($spec.keys)) { if ($def.path) { $paths += @{ path = [string]$def.path; name = [string]$def.name } } }
    foreach ($pv in @($spec.gate.policyValues)) { $paths += @{ path = [string]$pv.path; name = [string]$pv.name } }
    return $paths
}
function HRsop() {
    try { $gpos = @(Get-CimInstance -Namespace root\rsop\computer -ClassName RSOP_GPO) }
    catch { if (MissingRsopNamespace $_.Exception) { return }; throw }
    foreach ($gpo in $gpos) {
        if ($gpo.id -isnot [string] -or [string]::IsNullOrWhiteSpace($gpo.id)) { throw 'Group Policy authority is unknown: assessment only' }
        if ($gpo.enabled -isnot [bool] -or $gpo.accessDenied -isnot [bool] -or $gpo.filterAllowed -isnot [bool] -or $gpo.accessDenied) { throw 'Group Policy authority is unknown: assessment only' }
        if ($gpo.enabled -and $gpo.filterAllowed -and $gpo.id -ne 'LocalGPO') { ThrowGate 'Applied computer Group Policy: assessment only' }
    }
    $watch = @(HRegistryPaths)
    if ($watch.Count -eq 0) { return }
    foreach ($setting in @(Get-CimInstance -Namespace root\rsop\computer -ClassName RSOP_RegistryPolicySetting)) {
        if ($setting.registryKey -isnot [string] -or $setting.valueName -isnot [string]) { throw 'Group Policy authority is unknown: assessment only' }
        foreach ($w in $watch) {
            $pattern = [regex]::Escape($w.path.Substring(5)) + '(\\|$)'
            if (('\' + $setting.registryKey) -match $pattern -and $setting.valueName -eq $w.name) { ThrowGate 'Relevant resultant Group Policy: assessment only' }
        }
    }
}
function HGatePolicy() {
    foreach ($area in @($spec.gate.areas)) {
        CheckPolicyValues (PolicyValues "HKLM:\SOFTWARE\Microsoft\PolicyManager\current\device\$area") $spec.gate.pattern $true
    }
    $root = 'HKLM:\SOFTWARE\Microsoft\PolicyManager\providers'
    if ((@($spec.gate.areas)).Count -gt 0 -and (Test-Path -LiteralPath $root)) {
        foreach ($provider in @(Get-ChildItem -LiteralPath $root)) {
            foreach ($area in @($spec.gate.areas)) {
                CheckPolicyValues (PolicyValues (Join-Path $provider.PSPath "default\device\$area")) $spec.gate.pattern $false
            }
        }
    }
    foreach ($pv in @($spec.gate.policyValues)) {
        if (Test-Path -LiteralPath $pv.path) {
            $key = Get-Item -LiteralPath $pv.path
            if ($key.GetValueNames() -contains $pv.name) { ThrowGate 'Relevant policy is configured: assessment only' }
        }
    }
    $own = [string]$spec.gate.ownPolicyKey
    if ($own) {
        if (Test-Path -LiteralPath $own) {
            $key = Get-Item -LiteralPath $own
            $ours = @(@($spec.keys) | ForEach-Object { $_.name })
            if (@($key.GetValueNames() | Where-Object { $_ -and $ours -cnotcontains $_ }).Count -gt 0 -or $key.SubKeyCount -gt 0) { ThrowGate 'Relevant policy is configured: assessment only' }
        }
    }
    $artifacts = @('System32\GroupPolicy\Machine\Registry.pol','System32\GroupPolicy\gpt.ini')
    if ($spec.gate.secedit) { $artifacts += @('System32\GroupPolicy\Machine\Microsoft\Windows NT\SecEdit\GptTmpl.inf') }
    foreach ($pol in $artifacts) {
        if (Test-Path -LiteralPath ([IO.Path]::Combine($env:SystemRoot, $pol))) { throw 'Local computer policy artifacts: assessment only' }
    }
}
function HGateFirewall() {
    Load 'CimCmdlets'; Load 'NetSecurity'
    foreach ($service in @('MpsSvc','BFE')) {
        $s = @(Get-Service $service)
        if ($s.Count -ne 1 -or [string]$s[0].Status -cne 'Running') { throw 'Firewall services unavailable' }
    }
    if (@(Get-CimInstance -Namespace root\SecurityCenter2 -ClassName FirewallProduct).Count -gt 0) { throw 'Additional or unrecognized security provider: assessment only' }
    foreach ($p in @(Get-NetFirewallProfile -PolicyStore RSOP)) {
        if ([string]$p.Enabled -ne 'NotConfigured' -or [string]$p.DefaultInboundAction -ne 'NotConfigured' -or [string]$p.DefaultOutboundAction -ne 'NotConfigured') { ThrowGate 'Firewall profile has resultant Group Policy: assessment only' }
    }
    try { $rsopRules = @(Get-NetFirewallRule -PolicyStore RSOP -ErrorAction Stop) }
    catch { if ($_.CategoryInfo.Category -ne 'ObjectNotFound') { throw }; $rsopRules = @() }
    if ($rsopRules.Count -gt 0) { ThrowGate 'Firewall profile has resultant Group Policy: assessment only' }
}
function HGateWifi() {
    foreach ($p in @('HKLM:\SOFTWARE\Policies\Microsoft\Windows\Wireless','HKLM:\SOFTWARE\Policies\Microsoft\Windows\WcmSvc')) {
        if (HasValues $p) { ThrowGate 'Configured management/security policy: assessment only' }
    }
}
function HGate() {
    if ($spec.id.StartsWith('defender.')) { $null = Gate $spec.id; return }
    HGateCommon
    HGatePolicy
    HRsop
    if ($spec.source -ceq 'FirewallExposure') { HGateFirewall }
    if ($spec.source -ceq 'WifiProfiles') { HGateWifi }
}

# Extra conditions that must hold before a repair (never before an undo).
function HPreflight() {
    switch -CaseSensitive ($spec.id) {
        'lsa.run_as_ppl' {
            Load 'SecureBoot'
            if (!(Confirm-SecureBootUEFI)) { throw 'Not offered: Secure Boot is off' }
            $sac = 'HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy'
            if (Test-Path -LiteralPath $sac) {
                $k = Get-Item -LiteralPath $sac
                if ($k.GetValueNames() -contains 'VerifiedAndReputablePolicyState' -and [int]$k.GetValue('VerifiedAndReputablePolicyState') -ne 0) { throw 'Not offered: Smart App Control is on' }
            }
            Load 'Microsoft.PowerShell.Diagnostics'
            $hit = $false
            try {
                $null = Get-WinEvent -FilterHashtable @{ LogName = 'Microsoft-Windows-CodeIntegrity/Operational'; Id = 3033,3063,3065,3066; StartTime = (Get-Date).AddDays(-30) } -MaxEvents 1 -ErrorAction Stop
                $hit = $true
            } catch { if ([string]$_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound*') { throw } }
            if ($hit) { throw 'Not offered: some sign-in add-ons would stop working' }
            $lsa = Get-Item -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Lsa'
            $allowed = @{
                'Authentication Packages' = @('msv1_0')
                'Notification Packages' = @('scecli','rassfm')
                'Security Packages' = @('kerberos','msv1_0','schannel','wdigest','tspkg','pku2u','cloudap','negoexts','livessp')
            }
            foreach ($name in $allowed.Keys) {
                if ($lsa.GetValueNames() -notcontains $name) { continue }
                foreach ($entry in @($lsa.GetValue($name))) {
                    foreach ($pkg in ([string]$entry -split '[,\s"]+')) {
                        if ($pkg -and $allowed[$name] -cnotcontains $pkg.ToLowerInvariant()) { throw 'Not offered: sign-in add-ons from other companies are installed' }
                    }
                }
            }
        }
        { $_ -in @('defender.asr.standard','defender.asr.web_script_email') } {
            Load 'Defender'
            if ((Get-MpComputerStatus).RealTimeProtectionEnabled -ne $true) { throw 'Not offered: Defender real-time protection is off' }
            if (Test-Path -LiteralPath 'HKLM:\SOFTWARE\Microsoft\CCM') { throw 'Not offered: this PC uses Configuration Manager' }
            if ($spec.id -ceq 'defender.asr.web_script_email') {
                $maps = HEnumNumber (Get-MpPreference).MAPSReporting $hMaps
                if ($maps -eq 0) { throw 'Not offered: Defender cloud protection is off' }
            }
        }
        'accounts.builtin_administrator' {
            Load 'Microsoft.PowerShell.LocalAccounts'
            $other = $false
            try {
                foreach ($m in @(Get-LocalGroupMember -SID 'S-1-5-32-544')) {
                    $sid = [string]$m.SID.Value
                    if ($sid -cmatch '^S-1-5-21-[0-9-]+$' -and $sid -cnotmatch '-500$' -and [string]$m.ObjectClass -ceq 'User') {
                        if ((Get-LocalUser -SID $m.SID).Enabled -eq $true) { $other = $true }
                    }
                }
            } catch { throw 'Not offered: no other administrator account could be confirmed' }
            if (!$other) { throw 'Not offered: no other administrator account is enabled' }
        }
    }
}

# --------------------------------------------------------------- observe
function HObserve() {
    $slice = HRead
    $o = @{ value = @{ items = $slice }; eligible = $true; reason = 'Eligible unmanaged local preference' }
    try {
        HGate
        if (HAnyUnsafe $slice) { HPreflight }
    } catch {
        $o.eligible = $false
        $o.reason = $_.Exception.Message
    }
    return $o
}

# ----------------------------------------------------------------- writers
function HSetRegistry($def, $v) {
    if ($null -eq $v) { Remove-ItemProperty -LiteralPath $def.path -Name $def.name -ErrorAction Stop; return }
    if (!(Test-Path -LiteralPath $def.path)) { $null = New-Item -Path $def.path -Force -ErrorAction Stop }
    New-ItemProperty -LiteralPath $def.path -Name $def.name -PropertyType DWord -Value ([int]$v) -Force -ErrorAction Stop | Out-Null
}
function HSetDefenderPref($def, $v) {
    Load 'Defender'
    $p = @{}
    if ($def.name -ceq 'MAPSReporting') { $p[$def.name] = $hMaps[[int]$v] }
    elseif ($def.name -ceq 'PUAProtection') { $p[$def.name] = $hPua[[int]$v] }
    else { $p[$def.name] = [bool]([int]$v) }
    Set-MpPreference @p
}
function HSetAsr($def, $v) {
    Load 'Defender'
    if ($null -eq $v) { Remove-MpPreference -AttackSurfaceReductionRules_Ids $def.name; return }
    $action = switch ([int]$v) { 0 { 'Disabled' } 1 { 'Enabled' } 2 { 'AuditMode' } 6 { 'Warn' } default { throw 'Invalid attack surface rule action' } }
    Add-MpPreference -AttackSurfaceReductionRules_Ids $def.name -AttackSurfaceReductionRules_Actions $action
}
function HSetLockout($def, $v) {
    $d = HAdsiPolicy
    $d.Put('MaxBadPasswordsAllowed', [int]$v)
    $d.SetInfo()
}
function HSetBuiltinAdmin($def, $v) {
    $a = HBuiltinAdmin
    if ([int]$v -eq 1) { Enable-LocalUser -SID $a.SID } else { Disable-LocalUser -SID $a.SID }
}
function HSetFirewall($name, $v) {
    Load 'NetSecurity'
    $bits = [int]$v -band 7
    $profiles = if ($bits -eq 7) { 'Any' } else {
        @(@(@{ b = 1; n = 'Domain' }, @{ b = 2; n = 'Private' }, @{ b = 4; n = 'Public' }) | Where-Object { ($bits -band $_.b) -ne 0 } | ForEach-Object { $_.n })
    }
    $enabled = if (([int]$v -band 8) -ne 0) { 'True' } else { 'False' }
    Set-NetFirewallRule -PolicyStore PersistentStore -Name $name -Profile $profiles -Enabled $enabled -ErrorAction Stop
}
function HWlanApi() {
    if ($null -ne ('Secblitz.WlanApi' -as [type])) { return }
    # Reflection.Emit P/Invoke stubs: no csc.exe child process (see MdmRegistration).
    $dll = [IO.Path]::Combine($env:SystemRoot, 'System32\wlanapi.dll')
    $assembly = [AppDomain]::CurrentDomain.DefineDynamicAssembly([Reflection.AssemblyName]::new('Secblitz.WlanApi'), [Reflection.Emit.AssemblyBuilderAccess]::Run)
    $module = $assembly.DefineDynamicModule('Secblitz.WlanApi')
    $type = $module.DefineType('Secblitz.WlanApi', [Reflection.TypeAttributes]'Public, Abstract, Sealed')
    $defs = @(
        @{ n = 'WlanOpenHandle'; p = [Type[]]@([uint32], [IntPtr], [uint32].MakeByRefType(), [IntPtr].MakeByRefType()) },
        @{ n = 'WlanCloseHandle'; p = [Type[]]@([IntPtr], [IntPtr]) },
        @{ n = 'WlanSetProfile'; p = [Type[]]@([IntPtr], [Guid].MakeByRefType(), [uint32], [string], [string], [bool], [IntPtr], [uint32].MakeByRefType()) }
    )
    foreach ($d in $defs) {
        $m = $type.DefinePInvokeMethod($d.n, $dll, $d.n, [Reflection.MethodAttributes]'Public, Static, PinvokeImpl', [Reflection.CallingConventions]::Standard, [uint32], $d.p, [Runtime.InteropServices.CallingConvention]::Winapi, [Runtime.InteropServices.CharSet]::Unicode)
        $m.SetImplementationFlags($m.GetMethodImplementationFlags() -bor [Reflection.MethodImplAttributes]::PreserveSig)
    }
    $null = $type.CreateType()
}
function HSetWifi($name, $v) {
    $found = @(HWlanProfiles | Where-Object { $_.name -ceq $name })
    if ($found.Count -ne 1) { throw 'Saved Wi-Fi network not found exactly once' }
    $p = $found[0]
    $doc = HLoadXml $p.file
    $node = $doc.SelectSingleNode("//*[local-name()='connectionMode']")
    if ($null -eq $node) { throw 'Wi-Fi profile has no connection mode' }
    $node.InnerText = $(if ([int]$v -eq 1) { 'auto' } else { 'manual' })
    HWlanApi
    $negotiated = [uint32]0; $handle = [IntPtr]::Zero
    $rc = [Secblitz.WlanApi]::WlanOpenHandle([uint32]2, [IntPtr]::Zero, [ref]$negotiated, [ref]$handle)
    if ($rc -ne 0) { throw "Wi-Fi service is not available (code $rc)" }
    try {
        $iface = [Guid]$p.iface; $reason = [uint32]0
        # Flags 0 = all-user profile, overwrite in place; nothing is deleted.
        $rc = [Secblitz.WlanApi]::WlanSetProfile($handle, [ref]$iface, [uint32]0, $doc.OuterXml, $null, $true, [IntPtr]::Zero, [ref]$reason)
        if ($rc -ne 0) { throw "Wi-Fi profile update was refused (code $rc, reason $reason)" }
    } finally { $null = [Secblitz.WlanApi]::WlanCloseHandle($handle, [IntPtr]::Zero) }
}
function HSet([string]$name, $v) {
    $def = HDef $name
    switch -CaseSensitive ($spec.source) {
        'Registry' { HSetRegistry $def $v }
        'DefenderPref' { HSetDefenderPref $def $v }
        'DefenderAsr' { HSetAsr $def $v }
        'Lockout' { HSetLockout $def $v }
        'BuiltinAdmin' { HSetBuiltinAdmin $def $v }
        'FirewallExposure' { HSetFirewall $name $v }
        'WifiProfiles' { HSetWifi $name $v }
        default { throw 'Unknown hardening source' }
    }
}

function HParseInput($inputValue) {
    $names = @($inputValue.PSObject.Properties.Name)
    if ($names.Count -ne 1 -or $names[0] -cne 'items') { throw 'Invalid hardening state' }
    $wanted = @{}
    foreach ($p in @($inputValue.items.PSObject.Properties)) {
        if (!(HNameOk $p.Name)) { throw 'Unknown hardening item' }
        $v = $p.Value
        if ($null -ne $v) {
            if ($v -isnot [int] -and $v -isnot [long]) { throw 'Invalid hardening value' }
            if ($v -lt 0 -or $v -gt [int](HDef $p.Name).max) { throw 'Hardening value out of range' }
            $v = [int]$v
        }
        $wanted[$p.Name] = $v
    }
    if (!$spec.dynamic -and $wanted.Count -ne @($spec.keys).Count) { throw 'Hardening state must contain every item' }
    return $wanted
}

function HWrite($inputValue) {
    $wanted = HParseInput $inputValue
    HGate
    $current = HRead
    $steps = @()
    $repairing = $false
    foreach ($name in @($wanted.Keys | Sort-Object)) {
        if (!$current.ContainsKey($name)) { throw "Item $name no longer exists; nothing was changed" }
        $cur = $current[$name]; $want = $wanted[$name]
        if (HEq $cur $want) { continue }
        $def = HDef $name
        if (!(HIsSafe $def $cur) -and (HEq (HFixOf $def $cur) $want)) { $repairing = $true }
        elseif (!(HIsSafe $def $want) -and (HEq (HFixOf $def $want) $cur)) { }
        else { throw "Item $name changed before the write; nothing was changed" }
        $steps += @{ name = $name; from = $cur; to = $want }
    }
    if ($repairing) { HPreflight }
    $done = @()
    try {
        foreach ($s in $steps) { HSet $s.name $s.to; $done += $s }
        $verified = $false
        for ($attempt = 0; $attempt -lt 10; $attempt++) {
            $now = HRead
            $ok = $true
            foreach ($name in $wanted.Keys) { if (!$now.ContainsKey($name) -or !(HEq $now[$name] $wanted[$name])) { $ok = $false } }
            if ($ok) { $verified = $true; break }
            if ($attempt -lt 9) { Start-Sleep -Milliseconds 500 }
        }
        if (!$verified) { throw 'Readback did not match; mutation outcome requires review' }
    } catch {
        $failure = $_
        # Best effort: put back what this call changed so undo still matches.
        for ($i = $done.Count - 1; $i -ge 0; $i--) { try { HSet $done[$i].name $done[$i].from } catch { } }
        throw $failure
    }
}

try {
    switch -CaseSensitive ($action) {
        'observe' { Emit (HObserve) }
        'write' { HWrite $value; Emit @{ok=$true} }
        default { throw 'Unknown operation' }
    }
} catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
