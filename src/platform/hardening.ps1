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
    if ($spec.source -ceq 'NetbiosAdapters') { return ($name -cmatch '^\{[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}\}$') }
    return ($name.Length -ge 1 -and $name.Length -le 64 -and $name -cnotmatch '[\x00-\x1f\x7f"]' -and $name.Trim() -ceq $name)
}
function HIsSafe($def, $v) {
    if ($def.rule -ceq 'exposure') {
        if ($null -eq $v) { return $false }
        return !((([int]$v -band 8) -ne 0) -and (([int]$v -band 4) -ne 0))
    }
    if ($null -eq $v) { return [bool]$def.absentSafe }
    return (@($def.safe) -contains [int64]$v)
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
function HValueName($def) {
    # The registry value name (a control may hold one value under several keys).
    if ($null -ne $def.PSObject.Properties['valueName'] -and [string]$def.valueName) { return [string]$def.valueName }
    return [string]$def.name
}
function HReadRegistry($def) {
    if (!(Test-Path -LiteralPath $def.path -ErrorAction Stop)) { return $null }
    $key = Get-Item -LiteralPath $def.path -ErrorAction Stop
    $vn = HValueName $def
    if ($key.GetValueNames() -notcontains $vn) { return $null }
    if ($key.GetValueKind($vn) -ne [Microsoft.Win32.RegistryValueKind]::DWord) { throw "$vn is not a DWORD" }
    $n = [int64]$key.GetValue($vn)
    # DWORDs are unsigned: 0xFFFFFFFF reads back as -1.
    if ($n -lt 0) { $n += 4294967296 }
    return $n
}
$hMaps = @('Disabled','Basic','Advanced')
$hPua = @('Disabled','Enabled','AuditMode')
$hNp = @('Disabled','Enabled','AuditMode')
$hCbl = @{ Default = 0; Moderate = 1; High = 2; HighPlus = 4; ZeroTolerance = 6 }
function HCloudLevel($v) {
    if ($null -eq $v) { throw 'Defender preference is not readable' }
    if ($v -is [string]) {
        if (!$hCbl.ContainsKey($v)) { throw 'Defender preference is not readable' }
        return [int]$hCbl[$v]
    }
    $n = [int]$v
    if (@(0,1,2,4,6) -notcontains $n) { throw 'Defender preference is not readable' }
    return $n
}
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
        elseif ($def.name -ceq 'EnableNetworkProtection') {
            # Editions without the feature may not report it: treat as off, never as protected.
            if ($null -eq $raw) { $out[$def.name] = 0 } else { $out[$def.name] = HEnumNumber $raw $hNp }
        }
        elseif ($def.name -ceq 'CloudBlockLevel') { $out[$def.name] = HCloudLevel $raw }
        elseif ($def.name -ceq 'CloudExtendedTimeout') {
            if ($null -eq $raw) { throw 'Defender preference is not readable' }
            $out[$def.name] = [int]$raw
        }
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
function HReadNetbios() {
    Load 'CimCmdlets'
    $out = @{}
    $dupes = @{}
    foreach ($c in @(Get-CimInstance -ClassName Win32_NetworkAdapterConfiguration -Filter 'IPEnabled = True')) {
        $id = [string]$c.SettingID
        if (!(HNameOk $id) -or $null -eq $c.TcpipNetbiosOptions) { continue }
        $n = [int]$c.TcpipNetbiosOptions
        if (@(0,1,2) -notcontains $n) { continue }
        # Two configurations with one id are ambiguous: leave them alone.
        if ($out.ContainsKey($id) -or $dupes.ContainsKey($id)) { $dupes[$id] = $true; $out.Remove($id); continue }
        $out[$id] = $n
    }
    return $out
}
$hOutboundRule = 'Secblitz-Block-Outbound-SMB-Internet'
$hOutboundName = 'Secblitz: block outbound file sharing to the internet'
function HReadOutbound() {
    Load 'NetSecurity'
    try { $rules = @(Get-NetFirewallRule -PolicyStore PersistentStore -Name $hOutboundRule -ErrorAction Stop) }
    catch { if ($_.CategoryInfo.Category -ne 'ObjectNotFound') { throw }; return @{ RulePresent = 0 } }
    if ($rules.Count -eq 0) { return @{ RulePresent = 0 } }
    if ($rules.Count -ne 1) { throw 'The Secblitz firewall rule exists more than once' }
    $r = $rules[0]
    $ports = @($r | Get-NetFirewallPortFilter | ForEach-Object { $_.RemotePort } | ForEach-Object { [string]$_ })
    $proto = @($r | Get-NetFirewallPortFilter | ForEach-Object { [string]$_.Protocol })
    $addr = @($r | Get-NetFirewallAddressFilter | ForEach-Object { $_.RemoteAddress } | ForEach-Object { [string]$_ })
    $same = [string]$r.Direction -ceq 'Outbound' -and [string]$r.Action -ceq 'Block' -and [string]$r.Enabled -ceq 'True' -and
        $proto.Count -eq 1 -and $proto[0] -ceq 'TCP' -and $ports.Count -eq 2 -and $ports -contains '445' -and $ports -contains '139' -and
        $addr.Count -eq 1 -and $addr[0] -ieq 'Internet'
    if (!$same) { throw 'A firewall rule with the Secblitz name exists but is different; it was left alone' }
    return @{ RulePresent = 1 }
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
        'NetbiosAdapters' { return (HReadNetbios) }
        'FirewallOutbound' { return (HReadOutbound) }
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
    if ($spec.source -ceq 'FirewallExposure' -or $spec.source -ceq 'FirewallOutbound') { HGateFirewall }
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
        { $_ -in @('defender.asr.standard','defender.asr.web_script_email','defender.asr.office','defender.asr.ransomware_usb','defender.network_protection','defender.cloud_block_level') } {
            Load 'Defender'
            $status = Get-MpComputerStatus
            if ($status.RealTimeProtectionEnabled -ne $true) { throw 'Not offered: Defender real-time protection is off' }
            if (Test-Path -LiteralPath 'HKLM:\SOFTWARE\Microsoft\CCM') { throw 'Not offered: this PC uses Configuration Manager' }
            if (@('defender.asr.web_script_email','defender.asr.ransomware_usb','defender.network_protection','defender.cloud_block_level') -ccontains $spec.id) {
                $maps = HEnumNumber (Get-MpPreference).MAPSReporting $hMaps
                if ($maps -eq 0) { throw 'Not offered: Defender cloud protection is off' }
            }
            if ($spec.id -ceq 'defender.asr.office' -and !(HOfficeInstalled)) { throw 'Not offered: Microsoft Office was not found' }
            if ($spec.id -ceq 'defender.network_protection') {
                if (!(HEditionHasNetworkProtection)) { throw 'Not offered: this edition of Windows does not include it' }
                if ($status.BehaviorMonitorEnabled -ne $true) { throw 'Not offered: Defender behavior monitoring is off' }
            }
        }
        'net.netbios' { HNetbiosPreflight }
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

function HOfficeInstalled() {
    if (Test-Path -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Office\ClickToRun\Configuration') { return $true }
    foreach ($root in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
        if (!$root) { continue }
        foreach ($sub in @('Microsoft Office\root\Office16\WINWORD.EXE','Microsoft Office\Office16\WINWORD.EXE','Microsoft Office\Office15\WINWORD.EXE')) {
            if (Test-Path -LiteralPath ([IO.Path]::Combine($root, $sub))) { return $true }
        }
    }
    return $false
}
function HEditionHasNetworkProtection() {
    # Pro, Enterprise and Education family SKUs only. Anything unknown (Home
    # included) is treated as not supported.
    Load 'CimCmdlets'
    $os = Get-CimInstance Win32_OperatingSystem
    $sku = [int]$os.OperatingSystemSKU
    return (@(4,27,48,49,70,84,121,122,125,126,161,162,164,165,175) -contains $sku)
}
function HRemoteHost([string]$path) {
    if ($path -cmatch '^\\\\([^\\/]+)[\\/]') { return $Matches[1] }
    return ''
}
function HBareName([string]$hostName) {
    # A name with no dot that is not an address can only be found through NetBIOS.
    if (!$hostName) { return $false }
    $ip = $null
    if ([Net.IPAddress]::TryParse($hostName, [ref]$ip)) { return $false }
    return !$hostName.Contains('.')
}
function HRemoteHosts() {
    $paths = @()
    Load 'SmbShare'
    try { $paths += @(Get-SmbMapping -ErrorAction Stop | ForEach-Object { [string]$_.RemotePath }) }
    catch { if ($_.CategoryInfo.Category -ne 'ObjectNotFound') { throw } }
    try { $paths += @(Get-SmbConnection -ErrorAction Stop | ForEach-Object { '\\' + [string]$_.ServerName + '\x' }) }
    catch { if ($_.CategoryInfo.Category -ne 'ObjectNotFound') { throw } }
    # Persistent mapped drives live in each signed-in user's hive.
    foreach ($hive in @(Get-ChildItem -LiteralPath 'Registry::HKEY_USERS')) {
        if ($hive.PSChildName -cnotmatch '^S-1-5-21-[0-9-]+$') { continue }
        $net = "Registry::HKEY_USERS\$($hive.PSChildName)\Network"
        if (!(Test-Path -LiteralPath $net)) { continue }
        foreach ($drive in @(Get-ChildItem -LiteralPath $net)) {
            $p = (Get-ItemProperty -LiteralPath $drive.PSPath).RemotePath
            if ($p) { $paths += [string]$p }
        }
    }
    return @($paths | ForEach-Object { HRemoteHost $_ } | Where-Object { $_ })
}
function HNetbiosPreflight() {
    Load 'SmbShare'
    $server = Get-SmbServerConfiguration
    if ($server.EnableSMB1Protocol -isnot [bool]) { throw 'Not offered: the old file-sharing version could not be checked' }
    if ($server.EnableSMB1Protocol) { throw 'Not offered: the old file-sharing version (SMB1) is still on' }
    $mr = 'HKLM:\SYSTEM\CurrentControlSet\Services\mrxsmb10'
    if (Test-Path -LiteralPath $mr) {
        $k = Get-Item -LiteralPath $mr
        if ($k.GetValueNames() -notcontains 'Start' -or [int]$k.GetValue('Start') -ne 4) { throw 'Not offered: the old file-sharing version (SMB1) is still on' }
    }
    foreach ($h in @(HRemoteHosts)) {
        if (HBareName $h) { throw 'Not offered: a shared folder or drive may rely on the old name service' }
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
    $vn = HValueName $def
    if ($null -eq $v) { Remove-ItemProperty -LiteralPath $def.path -Name $vn -ErrorAction Stop; return }
    if (!(Test-Path -LiteralPath $def.path)) { $null = New-Item -Path $def.path -Force -ErrorAction Stop }
    # DWORDs are unsigned; the cmdlet wants the same 32 bits as a signed int.
    $bits = [BitConverter]::ToInt32([BitConverter]::GetBytes([uint32][int64]$v), 0)
    New-ItemProperty -LiteralPath $def.path -Name $vn -PropertyType DWord -Value $bits -Force -ErrorAction Stop | Out-Null
}
function HSetDefenderPref($def, $v) {
    Load 'Defender'
    $p = @{}
    if ($def.name -ceq 'MAPSReporting') { $p[$def.name] = $hMaps[[int]$v] }
    elseif ($def.name -ceq 'PUAProtection') { $p[$def.name] = $hPua[[int]$v] }
    elseif ($def.name -ceq 'EnableNetworkProtection') { $p[$def.name] = $hNp[[int]$v] }
    elseif ($def.name -ceq 'CloudBlockLevel') {
        $level = @($hCbl.Keys | Where-Object { $hCbl[$_] -eq [int]$v })
        if ($level.Count -ne 1) { throw 'Invalid cloud block level' }
        $p[$def.name] = [string]$level[0]
    }
    elseif ($def.name -ceq 'CloudExtendedTimeout') { $p[$def.name] = [uint32]$v }
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
function HSetNetbios($name, $v) {
    if (!(HNameOk $name) -or @(0,1,2) -notcontains [int]$v) { throw 'Invalid NetBIOS setting' }
    Load 'CimCmdlets'
    $found = @(Get-CimInstance -ClassName Win32_NetworkAdapterConfiguration -Filter "SettingID = '$name'")
    if ($found.Count -ne 1) { throw 'Network adapter not found exactly once' }
    $r = Invoke-CimMethod -InputObject $found[0] -MethodName SetTcpipNetbios -Arguments @{ TcpipNetbiosOptions = [uint32]$v }
    # 0 = done, 1 = done but a restart is needed.
    if ($null -eq $r -or @(0,1) -notcontains [int]$r.ReturnValue) { throw "NetBIOS setting was refused (code $($r.ReturnValue))" }
}
function HSetOutbound($v) {
    Load 'NetSecurity'
    if ([int]$v -eq 1) {
        New-NetFirewallRule -PolicyStore PersistentStore -Name $hOutboundRule -DisplayName $hOutboundName -Description 'Added by Secblitz. Stops this PC sending file-sharing traffic to the internet.' -Direction Outbound -Action Block -Protocol TCP -RemotePort 445,139 -RemoteAddress Internet -Profile Any -Enabled True -ErrorAction Stop | Out-Null
    } else {
        Remove-NetFirewallRule -PolicyStore PersistentStore -Name $hOutboundRule -ErrorAction Stop
    }
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
        'NetbiosAdapters' { HSetNetbios $name $v }
        'FirewallOutbound' { HSetOutbound $v }
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
            if ($v -lt 0 -or $v -gt [int64](HDef $p.Name).max) { throw 'Hardening value out of range' }
            $v = [int64]$v
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
        if (!$verified) {
            if ($spec.id.StartsWith('defender.')) { throw 'Readback did not match; Windows Security may be blocking this change (tamper protection); mutation outcome requires review' }
            throw 'Readback did not match; mutation outcome requires review'
        }
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
