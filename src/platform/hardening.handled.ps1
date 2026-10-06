# Handled-item controls: services.unquoted_paths, firewall.user_dir_inbound_allow,
# net.hosts_file, persistence.run_and_tasks. Concatenated between the backend
# definitions and hardening.ps1 (which holds the dispatcher), so everything here
# is available to it and to $spec at call time.
#
# Every item reads 1 while it is flagged and untouched, 0 while it is exactly as
# Secblitz left it, and 2 when Secblitz fixed it and someone changed it since.
# What undo needs (the exact original) lives in Secblitz-owned state under
# HKLM\Software\Secblitz\HardeningUndo\<control id>; the journal only records 1.
# Nothing is deleted and nothing here starts a child process.
$hUserDirPattern = '\\users\\[^\\]+\\(downloads|desktop|appdata\\local\\temp)\\|\\users\\public\\|\\windows\\temp\\|%userprofile%\\(downloads|desktop)\\|%temp%\\|%public%\\'
$hHostsBroad = '(^|\.)(microsoft|windowsupdate|windows|live|office|office365|msedge|xbox)\.(com|net)\z|defender|kaspersky|avast|avg\.com|norton|symantec|mcafee|malwarebytes|bitdefender|eset\.|sophos|trendmicro|avira|webroot|paypal|bank|chase\.com|wellsfargo|citibank|hsbc|barclays|santander|capitalone|americanexpress|revolut'
$hHostsUpdate = 'windowsupdate\.com\z|(^|\.)update\.microsoft\.com\z|(^|\.)download\.microsoft\.com\z|(^|\.)smartscreen[^.]*\.microsoft\.com\z|(^|\.)wdcp\.microsoft\.com\z|defender|kaspersky|avast|norton|symantec|mcafee|malwarebytes|bitdefender|eset\.|sophos|trendmicro|avira|webroot'
$hHostsNote = '# turned off by Secblitz '
$hHostsMaxBytes = 131072
$hStartupMax = 48
$hLabelLimit = 24
$script:hLabels = @{}

# ---- Secblitz-owned undo state
function HStateSub() { return ('SOFTWARE\Secblitz\HardeningUndo\' + [string]$spec.id) }
function HStateNames() {
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey((HStateSub), $false)
    if ($null -eq $key) { return @() }
    try { return @($key.GetValueNames()) } finally { $key.Dispose() }
}
function HStateGet([string]$name) {
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey((HStateSub), $false)
    if ($null -eq $key) { return $null }
    try {
        if ($key.GetValueNames() -cnotcontains $name) { return $null }
        $text = $key.GetValue($name)
        if ($text -isnot [string]) { return $null }
        return (ConvertFrom-Json -InputObject $text)
    } finally { $key.Dispose() }
}
function HStateSet([string]$name, $data) {
    $key = [Microsoft.Win32.Registry]::LocalMachine.CreateSubKey((HStateSub))
    try { $key.SetValue($name, (ConvertTo-Json -InputObject $data -Compress), [Microsoft.Win32.RegistryValueKind]::String) } finally { $key.Dispose() }
}
function HStateRemove([string]$name) {
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey((HStateSub), $true)
    if ($null -eq $key) { return }
    try {
        if ($key.GetValueNames() -ccontains $name) { $key.DeleteValue($name, $false) }
    } finally { $key.Dispose() }
}
function HSha256Hex([byte[]]$bytes) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-', '').ToLowerInvariant()) } finally { $sha.Dispose() }
}
function HLabel([string]$name, [string]$text) {
    $clean = ($text -replace '[\x00-\x1f\x7f]', ' ').Trim()
    if ($clean.Length -gt 120) { $clean = $clean.Substring(0, 120) }
    $script:hLabels[$name] = $clean
}
function HLabelKind([string]$name) {
    switch -CaseSensitive ([string]$spec.source) {
        'UnquotedServices' { return 'service' }
        'UserDirFirewall' { return 'rule' }
        'HostsFile' { return 'hosts' }
        'StartupItems' { if ($name.StartsWith('task:')) { return 'task' } else { return 'startup' } }
    }
    return ''
}
function HLabelList($slice) {
    # The exact items a fix would change, for the row's details (display only).
    $out = @()
    foreach ($name in @($slice.Keys | Sort-Object)) {
        if ($null -eq $slice[$name] -or [int64]$slice[$name] -ne 1 -or !$script:hLabels.ContainsKey($name)) { continue }
        $kind = HLabelKind $name
        if ($kind -eq '') { continue }
        $out += @{ kind = $kind; name = [string]$script:hLabels[$name] }
        if ($out.Count -ge $hLabelLimit) { break }
    }
    return $out
}

# ---- services.unquoted_paths
function HServicesRoot() { return 'SYSTEM\CurrentControlSet\Services' }
function HBroadWriters() { return @('S-1-1-0', 'S-1-5-11', 'S-1-5-32-545') }
function HDirWritable([string]$dir, $cache) {
    if ($cache.ContainsKey($dir)) { return [bool]$cache[$dir] }
    $result = $false
    try {
        if ([IO.Directory]::Exists($dir)) {
            $acl = (New-Object IO.DirectoryInfo $dir).GetAccessControl([Security.AccessControl.AccessControlSections]::Access)
            foreach ($rule in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
                if ($rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow) { continue }
                if ((HBroadWriters) -cnotcontains $rule.IdentityReference.Value) { continue }
                if (([int]$rule.PropagationFlags -band 2) -ne 0) { continue }
                if ((([int]$rule.FileSystemRights) -band (2 -bor 262144 -bor 524288)) -ne 0) { $result = $true }
            }
        }
    } catch { $result = $false }
    $cache[$dir] = $result
    return $result
}
function HUnquotedSplit([string]$raw) {
    # The program part of an unquoted path that has spaces, or $null. Leading and
    # trailing blanks would not survive a quote-and-unquote round trip, so those are left alone.
    if ($raw -cne $raw.Trim() -or $raw.StartsWith('"') -or $raw -notmatch '^(?<exe>[A-Za-z]:\\.*?\.exe)(\s|$)') { return $null }
    $exe = $Matches['exe']
    if (!$exe.Contains(' ') -or $exe.Contains('"')) { return $null }
    return $exe
}
function HUnquotedProblem([string]$exe) {
    # $null when quoting is safe, otherwise the reason it is not offered.
    if (![IO.File]::Exists($exe)) { return 'Not offered: a background program file could not be found' }
    $at = $exe.IndexOf(' ')
    while ($at -ge 0) {
        $candidate = $exe.Substring(0, $at)
        foreach ($path in @(($candidate + '.exe'), $candidate)) {
            if ($path -cne $exe -and [IO.File]::Exists($path)) { return 'Not offered: another program could be started first' }
        }
        $at = $exe.IndexOf(' ', $at + 1)
    }
    return $null
}
function HUnquotedEntries() {
    $out = @()
    $root = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey((HServicesRoot), $false)
    if ($null -eq $root) { throw 'The list of background programs is not readable' }
    $cache = @{}
    try {
        $names = @($root.GetSubKeyNames())
        if ($names.Count -gt 4096) { throw 'Too many background programs to check' }
        foreach ($name in $names) {
            $key = $root.OpenSubKey($name, $false)
            if ($null -eq $key) { continue }
            try {
                $type = $key.GetValue('Type', $null)
                if ($type -isnot [int] -or ($type -band 0x30) -eq 0) { continue }
                if ($key.GetValueNames() -cnotcontains 'ImagePath') { continue }
                $kind = $key.GetValueKind('ImagePath')
                if ($kind -ne [Microsoft.Win32.RegistryValueKind]::String -and $kind -ne [Microsoft.Win32.RegistryValueKind]::ExpandString) { continue }
                $raw = $key.GetValue('ImagePath', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                if ($raw -isnot [string]) { continue }
                $exe = HUnquotedSplit $raw
                if ($null -eq $exe) { continue }
                $hit = $false
                $at = $exe.IndexOf(' ')
                while ($at -ge 0) {
                    $dir = [IO.Path]::GetDirectoryName($exe.Substring(0, $at))
                    if ($null -ne $dir -and (HDirWritable $dir $cache)) { $hit = $true }
                    $at = $exe.IndexOf(' ', $at + 1)
                }
                if (!$hit -or !(HNameOk $name)) { continue }
                $out += @{ name = $name; raw = $raw; exe = $exe; fixed = ('"' + $exe + '"' + $raw.Substring($exe.Length)); kind = $kind; problem = (HUnquotedProblem $exe) }
            } finally { $key.Dispose() }
        }
    } finally { $root.Dispose() }
    return $out
}
function HReadUnquoted() {
    $out = @{}
    $script:hLabels = @{}
    foreach ($e in @(HUnquotedEntries)) { $out[$e.name] = 1; HLabel $e.name ($e.name + ' (' + $e.exe + ')') }
    foreach ($name in @(HStateNames)) {
        if ($out.ContainsKey($name) -or !(HNameOk $name)) { continue }
        $st = HStateGet $name
        if ($null -eq $st) { continue }
        $current = $null
        $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey((HServicesRoot) + '\' + $name, $false)
        if ($null -ne $key) {
            try { if ($key.GetValueNames() -ccontains 'ImagePath') { $current = $key.GetValue('ImagePath', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } } finally { $key.Dispose() }
        }
        $out[$name] = $(if ($current -is [string] -and $current -ceq [string]$st.f) { 0 } else { 2 })
    }
    if ($out.Count -gt 256) { throw 'Too many background programs to handle at once' }
    return $out
}
function HSetUnquoted([string]$name, $v) {
    if (!(HNameOk $name) -or $null -eq $v -or ([int]$v -ne 0 -and [int]$v -ne 1)) { throw 'Invalid background program state' }
    $path = (HServicesRoot) + '\' + $name
    if ([int]$v -eq 0) {
        $entry = @(HUnquotedEntries | Where-Object { $_.name -ceq $name })
        if ($entry.Count -ne 1) { throw 'The background program no longer needs a change' }
        $e = $entry[0]
        if ($null -ne $e.problem) { throw $e.problem }
        HStateSet $name @{ o = [string]$e.raw; f = [string]$e.fixed }
        try {
            $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($path, $true)
            if ($null -eq $key) { throw 'The background program was not found' }
            try { $key.SetValue('ImagePath', [string]$e.fixed, $e.kind) } finally { $key.Dispose() }
        } catch { HStateRemove $name; throw }
        return
    }
    $st = HStateGet $name
    if ($null -eq $st) { throw 'Secblitz no longer has the saved original for this background program' }
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($path, $true)
    if ($null -eq $key) { throw 'The background program was not found' }
    try {
        $current = $key.GetValue('ImagePath', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        if ($current -isnot [string] -or $current -cne [string]$st.f) { throw 'The background program changed again; it was left alone' }
        $key.SetValue('ImagePath', [string]$st.o, $key.GetValueKind('ImagePath'))
    } finally { $key.Dispose() }
    HStateRemove $name
}
function HUnquotedPreflight() {
    foreach ($e in @(HUnquotedEntries)) { if ($null -ne $e.problem) { throw $e.problem } }
}

# ---- firewall.user_dir_inbound_allow
function HReadUserDirFirewall() {
    Load 'NetSecurity'
    $out = @{}
    $script:hLabels = @{}
    $seen = @{}
    try { $rules = @(Get-NetFirewallRule -PolicyStore PersistentStore -Enabled True -Direction Inbound -Action Allow -ErrorAction Stop) }
    catch { if ($_.CategoryInfo.Category -ne 'ObjectNotFound') { throw }; $rules = @() }
    if ($rules.Count -gt 2048) { throw 'Too many firewall rules to check' }
    foreach ($rule in $rules) {
        $name = [string]$rule.Name
        $filter = $rule | Get-NetFirewallApplicationFilter
        $program = ([string]@($filter)[0].Program)
        if ($program -eq '' -or $program -ceq 'Any') { continue }
        if ($program.ToLowerInvariant() -cnotmatch $hUserDirPattern) { continue }
        if (!(HNameOk $name)) { continue }
        # Two rules with one name are ambiguous: leave them alone.
        if ($seen.ContainsKey($name)) { $out.Remove($name); continue }
        $seen[$name] = $true
        $out[$name] = 1
        $display = [string]$rule.DisplayName
        if ($display -eq '' -or $display.StartsWith('@')) { $display = $name }
        HLabel $name ($display + ' (' + $program + ')')
    }
    foreach ($name in @(HStateNames)) {
        if ($out.ContainsKey($name) -or !(HNameOk $name)) { continue }
        $found = @()
        try { $found = @(Get-NetFirewallRule -PolicyStore PersistentStore -Name $name -ErrorAction Stop) }
        catch { if ($_.CategoryInfo.Category -ne 'ObjectNotFound') { throw } }
        $out[$name] = $(if ($found.Count -eq 1 -and [string]$found[0].Enabled -ceq 'False') { 0 } else { 2 })
    }
    if ($out.Count -gt 256) { throw 'Too many firewall rules to handle at once' }
    return $out
}
function HSetUserDirFirewall([string]$name, $v) {
    if (!(HNameOk $name) -or $null -eq $v -or ([int]$v -ne 0 -and [int]$v -ne 1)) { throw 'Invalid firewall rule state' }
    Load 'NetSecurity'
    $found = @(Get-NetFirewallRule -PolicyStore PersistentStore -Name $name -ErrorAction Stop)
    if ($found.Count -ne 1) { throw 'The firewall rule was not found exactly once' }
    if ([int]$v -eq 0) {
        # Only a rule that is still enabled and still flagged is switched off.
        if (!(HReadUserDirFirewall).ContainsKey($name)) { throw 'The firewall rule no longer needs a change' }
        HStateSet $name @{ was = 'enabled' }
        try { Set-NetFirewallRule -PolicyStore PersistentStore -Name $name -Enabled False -ErrorAction Stop }
        catch { HStateRemove $name; throw }
        return
    }
    if ($null -eq (HStateGet $name)) { throw 'Secblitz no longer has the saved state for this firewall rule' }
    if ([string]$found[0].Enabled -cne 'False') { throw 'The firewall rule changed again; it was left alone' }
    Set-NetFirewallRule -PolicyStore PersistentStore -Name $name -Enabled True -ErrorAction Stop
    HStateRemove $name
}

# ---- net.hosts_file
function HHostsPath() { return [IO.Path]::Combine($env:SystemRoot, 'System32\drivers\etc\hosts') }
function HHostsBytes() {
    $p = HHostsPath
    if (![IO.File]::Exists($p)) { return $null }
    if ((New-Object IO.FileInfo $p).Length -gt 4194304) { throw 'The hosts file is too large to read' }
    return [byte[]][IO.File]::ReadAllBytes($p)
}
function HHostsPlain([byte[]]$bytes) {
    # ASCII-compatible text only (UTF-8 included): changing it keeps every other byte as it was.
    if ($bytes.Length -ge 2 -and (($bytes[0] -eq 0xFF -and $bytes[1] -eq 0xFE) -or ($bytes[0] -eq 0xFE -and $bytes[1] -eq 0xFF))) { return $false }
    return ([Array]::IndexOf($bytes, [byte]0) -lt 0)
}
function HHostsLineFlag([string]$line) {
    # Same test as the Tools security check (probes.ps1, HostsFile).
    $text = $line
    $hash = $text.IndexOf('#')
    if ($hash -ge 0) { $text = $text.Substring(0, $hash) }
    $parts = @($text.Trim() -split '\s+' | Where-Object { $_ -ne '' })
    if ($parts.Count -lt 2) { return $false }
    $address = $null
    if (-not [Net.IPAddress]::TryParse($parts[0], [ref]$address)) { return $false }
    $loop = $parts[0] -cin @('127.0.0.1', '::1', '0.0.0.0', '::')
    $broad = $false; $update = $false
    foreach ($h in @($parts[1..($parts.Count - 1)] | ForEach-Object { $_.ToLowerInvariant() })) {
        if ($h -match $hHostsBroad) { $broad = $true }
        if ($h -match $hHostsUpdate) { $update = $true }
    }
    if ($loop) { return $update }
    return $broad
}
function HHostsText([byte[]]$bytes) { return [Text.Encoding]::GetEncoding(28591).GetString($bytes) }
function HHostsFlaggedLines([byte[]]$bytes) {
    $out = @()
    $bom = [string][char]0xEF + [char]0xBB + [char]0xBF
    foreach ($raw in @((HHostsText $bytes) -split "`n")) {
        $line = $raw.TrimEnd([char]13)
        if ($line.StartsWith($bom, [StringComparison]::Ordinal)) { $line = $line.Substring(3) }
        if (HHostsLineFlag $line) { $out += $line }
    }
    return $out
}
function HHostsFix([byte[]]$bytes) {
    # Comment out only the flagged lines; every other byte stays exactly as it was.
    $bom = [string][char]0xEF + [char]0xBB + [char]0xBF
    $lines = @((HHostsText $bytes) -split "`n", -1)
    $done = @()
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $raw = $lines[$i]
        $cr = ''
        if ($raw.Length -gt 0 -and $raw[$raw.Length - 1] -eq [char]13) { $cr = [string][char]13; $raw = $raw.Substring(0, $raw.Length - 1) }
        $head = ''
        if ($i -eq 0 -and $raw.StartsWith($bom, [StringComparison]::Ordinal)) { $head = $bom; $raw = $raw.Substring(3) }
        if (HHostsLineFlag $raw) { $raw = $hHostsNote + $raw }
        $done += ($head + $raw + $cr)
    }
    return [byte[]][Text.Encoding]::GetEncoding(28591).GetBytes(($done -join "`n"))
}
function HReadHosts() {
    $out = @{}
    $script:hLabels = @{}
    $bytes = HHostsBytes
    if ($null -eq $bytes) { return $out }
    $flagged = @(HHostsFlaggedLines $bytes)
    if ($flagged.Count -gt 0) {
        $out['hosts'] = 1
        HLabel 'hosts' (($flagged | Select-Object -First 3) -join ' ; ')
        return $out
    }
    $st = HStateGet 'hosts'
    if ($null -ne $st) { $out['hosts'] = $(if ((HSha256Hex $bytes) -ceq [string]$st.f) { 0 } else { 2 }) }
    return $out
}
function HHostsWrite([byte[]]$bytes) {
    $fs = New-Object IO.FileStream((HHostsPath), [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::Read)
    try {
        # Overwrite in place, then cut to length: the file is never empty part-way.
        $fs.Position = 0
        $fs.Write($bytes, 0, $bytes.Length)
        $fs.SetLength($bytes.Length)
        $fs.Flush($true)
    } finally { $fs.Dispose() }
}
function HFlushDns() {
    # Best effort: the next lookup would pick the change up anyway.
    try { Load 'DnsClient'; Clear-DnsClientCache -ErrorAction Stop } catch { }
}
function HSetHosts([string]$name, $v) {
    if ($name -cne 'hosts' -or $null -eq $v -or ([int]$v -ne 0 -and [int]$v -ne 1)) { throw 'Invalid hosts file state' }
    $current = HHostsBytes
    if ($null -eq $current) { throw 'The hosts file was not found' }
    if ([int]$v -eq 0) {
        if (@(HHostsFlaggedLines $current).Count -eq 0) { throw 'The hosts file no longer needs a change' }
        $fixed = HHostsFix $current
        HStateSet 'hosts' @{ o = [Convert]::ToBase64String($current); f = (HSha256Hex $fixed) }
        try { HHostsWrite $fixed }
        catch { try { HHostsWrite $current } catch { }; HStateRemove 'hosts'; throw }
        HFlushDns
        return
    }
    $st = HStateGet 'hosts'
    if ($null -eq $st) { throw 'Secblitz no longer has the saved original of the hosts file' }
    if ((HSha256Hex $current) -cne [string]$st.f) { throw 'The hosts file changed again; it was left alone' }
    HHostsWrite ([Convert]::FromBase64String([string]$st.o))
    HStateRemove 'hosts'
    HFlushDns
}
function HHostsPreflight() {
    $bytes = HHostsBytes
    if ($null -eq $bytes) { throw 'Not offered: the hosts file could not be found' }
    if ($bytes.Length -gt $hHostsMaxBytes) { throw 'Not offered: the hosts file is too large to change safely' }
    if (!(HHostsPlain $bytes)) { throw 'Not offered: the hosts file uses a format we cannot keep exactly' }
    if (((New-Object IO.FileInfo (HHostsPath)).Attributes -band [IO.FileAttributes]::ReadOnly) -ne 0) { throw 'Not offered: the hosts file is locked against changes' }
}

# ---- persistence.run_and_tasks
function HStartupRoot([string]$kind) {
    # Where each kind of entry lives and where Windows keeps its on/off switch.
    $explorer = 'SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\'
    $run = 'SOFTWARE\Microsoft\Windows\CurrentVersion\Run'
    switch -CaseSensitive ($kind) {
        'run-machine' { return @{ hive = [Microsoft.Win32.Registry]::LocalMachine; run = $run; approved = ($explorer + 'Run') } }
        'run-machine32' { return @{ hive = [Microsoft.Win32.Registry]::LocalMachine; run = 'SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run'; approved = ($explorer + 'Run32') } }
        'run-user' { return @{ hive = [Microsoft.Win32.Registry]::CurrentUser; run = $run; approved = ($explorer + 'Run') } }
        'folder-machine' { return @{ hive = [Microsoft.Win32.Registry]::LocalMachine; dir = [Environment]::GetFolderPath('CommonStartup'); approved = ($explorer + 'StartupFolder') } }
        'folder-user' { return @{ hive = [Microsoft.Win32.Registry]::CurrentUser; dir = [Environment]::GetFolderPath('Startup'); approved = ($explorer + 'StartupFolder') } }
    }
    throw 'Unknown start-up entry kind'
}
function HStartupExpand([string]$text) {
    $t = $text.Trim()
    $appData = [Environment]::GetFolderPath('ApplicationData')
    $localData = [Environment]::GetFolderPath('LocalApplicationData')
    $userProfile = [Environment]::GetFolderPath('UserProfile')
    $commonData = [Environment]::GetFolderPath('CommonApplicationData')
    $publicDir = $null
    try { $publicDir = [IO.Path]::GetDirectoryName([Environment]::GetFolderPath('CommonDocuments')) } catch { }
    foreach ($pair in @(@('%appdata%', $appData), @('%localappdata%', $localData), @('%temp%', "$localData\Temp"), @('%tmp%', "$localData\Temp"), @('%userprofile%', $userProfile), @('%programdata%', $commonData), @('%public%', $publicDir))) {
        if ($null -ne $pair[1]) { $t = $t -ireplace [regex]::Escape($pair[0]), $pair[1].Replace('$', '$$') }
    }
    return [Environment]::ExpandEnvironmentVariables($t)
}
function HStartupTarget([string]$command) {
    $c = (HStartupExpand $command).Trim()
    if ($c -match '^"(?<p>[^"]+)"') { return $Matches['p'] }
    if ($c -match '^(?<p>[A-Za-z]:\\.*?\.(exe|dll|bat|cmd|vbs|vbe|js|jse|wsf|hta|ps1|scr|com|lnk|msi|cpl))(\s|$)') { return $Matches['p'] }
    return $null
}
function HStartupRisky([string]$command, [string]$exePath, [bool]$startupFolder) {
    # Same test as the Tools security check (probes.ps1, Autostart).
    $lower = $command.ToLowerInvariant()
    if ($lower -match '(powershell|pwsh)(\.exe)?["\s].*\s-(e|ec|enc|encodedcommand)\s+[a-z0-9+/=]{40,}' -or $lower -match '(mshta|regsvr32|certutil|bitsadmin|rundll32)\b.*https?://' -or $lower -match '(powershell|pwsh)\b.*(downloadstring|downloadfile|net\.webclient|\biwr\b|\biex\b)') { return $true }
    $path = $null
    if ($exePath -ne '') { $path = (HStartupExpand $exePath).Trim().Trim('"') } else { $path = HStartupTarget $command }
    if ($null -eq $path -or $path -cnotmatch '^[A-Za-z]:\\') { return $false }
    $p = $path.ToLowerInvariant()
    $riskyPath = '\\appdata\\local\\temp\\|\\windows\\temp\\|\\users\\public\\|\\downloads\\|\\appdata\\roaming\\[^\\]+\z'
    $scripts = '\.(bat|cmd|vbs|vbe|js|jse|wsf|hta|ps1)\z'
    $here = ($p -match $riskyPath) -or ($startupFolder -and $p -match $scripts)
    if (-not $here) { return $false }
    if (-not [IO.File]::Exists($path)) { return $false }
    try { $status = [string](Get-AuthenticodeSignature -LiteralPath $path).Status } catch { return $false }
    return ($status -cne 'Valid')
}
function HApprovedBytes($root, [string]$name) {
    # $null when the entry has no on/off record; otherwise the exact bytes.
    $key = $root.hive.OpenSubKey($root.approved, $false)
    if ($null -eq $key) { return $null }
    try {
        if ($key.GetValueNames() -cnotcontains $name) { return $null }
        if ($key.GetValueKind($name) -ne [Microsoft.Win32.RegistryValueKind]::Binary) { throw 'A start-up on/off record is not in the usual format' }
        return [byte[]]$key.GetValue($name)
    } finally { $key.Dispose() }
}
function HApprovedEnabled($bytes) {
    # Task Manager marks an entry off with an odd first byte (3); anything else runs.
    if ($null -eq $bytes) { return $true }
    if ($bytes.Length -lt 4) { return $false }
    return (($bytes[0] -band 1) -eq 0)
}
function HStartupEntries() {
    $out = @()
    foreach ($kind in @('run-machine', 'run-machine32', 'run-user')) {
        $root = HStartupRoot $kind
        $key = $root.hive.OpenSubKey($root.run, $false)
        if ($null -eq $key) { continue }
        try {
            $names = @($key.GetValueNames())
            if ($names.Count -gt 256) { throw 'Too many start-up entries to check' }
            foreach ($valueName in $names) {
                $value = $key.GetValue($valueName, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                if ($value -isnot [string] -or $value.Length -ge 2048 -or $valueName -eq '') { continue }
                $name = $kind + ':' + $valueName
                if (!(HNameOk $name) -or !(HStartupRisky $value '' $false)) { continue }
                if (!(HApprovedEnabled (HApprovedBytes $root $valueName))) { continue }
                $out += @{ name = $name; kind = $kind; item = $valueName; label = ($valueName + ' (' + $value + ')') }
            }
        } finally { $key.Dispose() }
    }
    foreach ($kind in @('folder-machine', 'folder-user')) {
        $root = HStartupRoot $kind
        $dir = [string]$root.dir
        if ([string]::IsNullOrEmpty($dir) -or -not [IO.Directory]::Exists($dir)) { continue }
        $files = @([IO.Directory]::GetFiles($dir))
        if ($files.Count -gt 256) { throw 'Too many start-up entries to check' }
        foreach ($file in $files) {
            $leaf = [IO.Path]::GetFileName($file)
            if ($leaf -ceq 'desktop.ini') { continue }
            if ($file.EndsWith('.lnk', [StringComparison]::OrdinalIgnoreCase)) {
                $shell = New-Object -ComObject WScript.Shell
                $link = $shell.CreateShortcut($file)
                $risky = HStartupRisky ('"' + $link.TargetPath + '" ' + $link.Arguments) '' $true
            } else { $risky = HStartupRisky $file $file $true }
            $name = $kind + ':' + $leaf
            if (!$risky -or !(HNameOk $name)) { continue }
            if (!(HApprovedEnabled (HApprovedBytes $root $leaf))) { continue }
            $out += @{ name = $name; kind = $kind; item = $leaf; label = $leaf }
        }
    }
    Load 'ScheduledTasks'
    $tasks = @(Get-ScheduledTask | Where-Object { $_.TaskPath -notlike '\Microsoft\*' -and $_.TaskName -notlike 'Secblitz*' -and [string]$_.State -cne 'Disabled' } | Select-Object -First 2049)
    if ($tasks.Count -gt 2048) { throw 'Too many scheduled tasks to check' }
    foreach ($task in $tasks) {
        $risky = $false
        foreach ($action in @($task.Actions)) {
            if ($null -eq $action.PSObject.Properties['Execute'] -or [string]::IsNullOrWhiteSpace([string]$action.Execute)) { continue }
            if (HStartupRisky (([string]$action.Execute) + ' ' + ([string]$action.Arguments)) ([string]$action.Execute) $false) { $risky = $true }
        }
        $name = 'task:' + [string]$task.TaskPath + [string]$task.TaskName
        if (!$risky -or !(HNameOk $name)) { continue }
        $out += @{ name = $name; kind = 'task'; item = [string]$task.TaskName; label = ([string]$task.TaskPath + [string]$task.TaskName) }
    }
    return $out
}
function HTaskParts([string]$name) {
    $m = [regex]::Match($name, '^task:(?<path>\\(?:.*\\)?)(?<leaf>[^\\]+)\z')
    if (!$m.Success) { throw 'Invalid scheduled task name' }
    return @{ path = $m.Groups['path'].Value; leaf = $m.Groups['leaf'].Value }
}
function HStartupTaskState([string]$name) {
    # 'Disabled', another state, or $null when the task is gone.
    $parts = HTaskParts $name
    Load 'ScheduledTasks'
    $found = @(Get-ScheduledTask -TaskPath $parts.path -TaskName $parts.leaf -ErrorAction SilentlyContinue)
    if ($found.Count -ne 1) { return $null }
    return [string]$found[0].State
}
function HReadStartup() {
    $out = @{}
    $script:hLabels = @{}
    foreach ($e in @(HStartupEntries)) { $out[$e.name] = 1; HLabel $e.name $e.label }
    foreach ($name in @(HStateNames)) {
        if ($out.ContainsKey($name) -or !(HNameOk $name)) { continue }
        $st = HStateGet $name
        if ($null -eq $st) { continue }
        if ($name.StartsWith('task:')) { $out[$name] = $(if ((HStartupTaskState $name) -ceq 'Disabled') { 0 } else { 2 }); continue }
        $kind = $name.Substring(0, $name.IndexOf(':'))
        $current = HApprovedBytes (HStartupRoot $kind) $name.Substring($kind.Length + 1)
        $out[$name] = $(if ($null -ne $current -and [Convert]::ToBase64String($current) -ceq [string]$st.d) { 0 } else { 2 })
    }
    if ($out.Count -gt 256) { throw 'Too many start-up items to handle at once' }
    return $out
}
function HSetApproved($root, [string]$item, [byte[]]$bytes) {
    $key = $root.hive.CreateSubKey($root.approved)
    try { $key.SetValue($item, $bytes, [Microsoft.Win32.RegistryValueKind]::Binary) } finally { $key.Dispose() }
}
function HSetStartup([string]$name, $v) {
    if (!(HNameOk $name) -or $null -eq $v -or ([int]$v -ne 0 -and [int]$v -ne 1)) { throw 'Invalid start-up item state' }
    if ($name.StartsWith('task:')) {
        $parts = HTaskParts $name
        Load 'ScheduledTasks'
        if ([int]$v -eq 0) {
            if (@(HStartupEntries | Where-Object { $_.name -ceq $name }).Count -ne 1) { throw 'The scheduled task no longer needs a change' }
            HStateSet $name @{ was = 'enabled' }
            try { $null = Disable-ScheduledTask -TaskPath $parts.path -TaskName $parts.leaf -ErrorAction Stop }
            catch { HStateRemove $name; throw }
            return
        }
        if ($null -eq (HStateGet $name)) { throw 'Secblitz no longer has the saved state for this scheduled task' }
        if ((HStartupTaskState $name) -cne 'Disabled') { throw 'The scheduled task changed again; it was left alone' }
        $null = Enable-ScheduledTask -TaskPath $parts.path -TaskName $parts.leaf -ErrorAction Stop
        HStateRemove $name
        return
    }
    $kind = $name.Substring(0, $name.IndexOf(':'))
    $item = $name.Substring($kind.Length + 1)
    $root = HStartupRoot $kind
    $original = HApprovedBytes $root $item
    if ([int]$v -eq 0) {
        if (@(HStartupEntries | Where-Object { $_.name -ceq $name }).Count -ne 1) { throw 'The start-up entry no longer needs a change' }
        [byte[]]$off = @(3, 0, 0, 0) + [BitConverter]::GetBytes([DateTime]::UtcNow.ToFileTimeUtc())
        HStateSet $name @{ o = $(if ($null -eq $original) { $null } else { [Convert]::ToBase64String($original) }); d = [Convert]::ToBase64String($off) }
        try { HSetApproved $root $item $off }
        catch { HStateRemove $name; throw }
        return
    }
    $st = HStateGet $name
    if ($null -eq $st) { throw 'Secblitz no longer has the saved state for this start-up entry' }
    if ($null -eq $original -or [Convert]::ToBase64String($original) -cne [string]$st.d) { throw 'The start-up entry changed again; it was left alone' }
    if ($null -eq $st.o) {
        $key = $root.hive.OpenSubKey($root.approved, $true)
        try { $key.DeleteValue($item, $false) } finally { $key.Dispose() }
    } else { HSetApproved $root $item ([Convert]::FromBase64String([string]$st.o)) }
    HStateRemove $name
}
function HStartupPreflight() {
    if (@(HStartupEntries).Count -gt $hStartupMax) { throw 'Not offered: too many items to switch off safely at once' }
}
