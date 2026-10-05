# Invoked only after the Rust launcher verifies a non-elevated original desktop
# token. Environment variables and profile paths supplied by data are never used.
function SafePath([string]$path) {
    $full = [IO.Path]::GetFullPath($path)
    if ($full -cnotmatch '^[A-Za-z]:\\' -or $full.Substring(2).Contains(':')) { throw 'Nonlocal browser path' }
    $part = [IO.Path]::GetPathRoot($full)
    foreach ($name in $full.Substring($part.Length).Split('\')) {
        if (!$name -or $name -in @('.','..') -or $name.EndsWith(' ') -or $name.EndsWith('.')) { throw 'Invalid path component' }
        $part = [IO.Path]::Combine($part,$name)
        try { $attr = [IO.File]::GetAttributes($part) }
        catch [IO.FileNotFoundException] { return $false }
        catch [IO.DirectoryNotFoundException] { return $false }
        if (($attr -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Reparse point omitted' }
    }
    return $true
}
function SmallJson([string]$path) {
    if (!(SafePath $path)) { throw 'Missing manifest' }
    $stream = [IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite)
    try {
        if ($stream.Length -gt 4194304) { throw 'Browser JSON cap' }
        $reader = [IO.StreamReader]::new($stream,[Text.UTF8Encoding]::new($false,$true),$true,4096,$true)
        try {
            # A concurrent file growth cannot make ReadToEnd unbounded.
            $buffer = [char[]]::new(4194305)
            $length = $reader.ReadBlock($buffer,0,$buffer.Length)
            if ($length -gt 4194304) { throw 'Browser JSON cap' }
            return ConvertFrom-Json -InputObject ([string]::new($buffer,0,$length))
        } finally { $reader.Dispose() }
    } finally { $stream.Dispose() }
}
function BrowserInventory {
    $local = [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)
    $roaming = [Environment]::GetFolderPath([Environment+SpecialFolder]::ApplicationData)
    if (!$local -or !$roaming) { throw 'Original-user folders unavailable' }
    $items = [Collections.Generic.List[object]]::new()
    $truncated = $false; $profiles = 0; $failed = $false
    foreach ($spec in @(@('Chrome','Google\Chrome\User Data'), @('Edge','Microsoft\Edge\User Data'))) {
        $root = [IO.Path]::Combine($local,$spec[1])
        try {
            if (!(SafePath $root)) { continue }
            $candidates = @([IO.Directory]::EnumerateDirectories($root) | Where-Object { [IO.Path]::GetFileName($_) -cmatch '^(Default|Profile [0-9]+)$' } | Select-Object -First 17)
            if ($candidates.Count -gt 16) { $truncated = $true }
            foreach ($profile in @($candidates | Select-Object -First 16)) {
                $profiles++; $extensionRoot = [IO.Path]::Combine($profile,'Extensions')
                if (!(SafePath $extensionRoot)) { continue }
                $extensions = @([IO.Directory]::EnumerateDirectories($extensionRoot) | Select-Object -First 257)
                if ($extensions.Count -gt 256) { $truncated = $true }
                foreach ($directory in @($extensions | Select-Object -First 256)) {
                    if ($items.Count -ge 512) { $truncated = $true; break }
                    $id = [IO.Path]::GetFileName($directory)
                    if ($id -cnotmatch '^[a-p]{32}$') { $failed = $true; continue }
                    try {
                        if (!(SafePath $directory)) { throw 'Extension disappeared' }
                        $versions = @([IO.Directory]::EnumerateDirectories($directory) | Select-Object -First 9)
                        if ($versions.Count -gt 8) { $truncated = $true }
                        foreach ($version in @($versions | Select-Object -First 8)) {
                            if ($items.Count -ge 512) { $truncated = $true; break }
                            $manifest = SmallJson ([IO.Path]::Combine($version,'manifest.json'))
                            $permissions = @()
                            foreach ($field in @('permissions','host_permissions')) {
                                if ($null -ne $manifest.PSObject.Properties[$field]) {
                                    if ($manifest.$field -isnot [array]) { throw 'Invalid permission list' }
                                    $permissions += $manifest.$field
                                }
                            }
                            $broad = ($permissions -contains '<all_urls>') -or ($permissions -contains '*://*/*') -or ($permissions -contains 'https://*/*') -or ($permissions -contains 'http://*/*')
                            $items.Add(@{browser=$spec[0];profile_index=$profiles;id=$id;version=(Text $manifest.version);enabled=@{state='Unknown';value='NotAssessed'};broad_host_access=(Known $broad);native_messaging=(Known ($permissions -contains 'nativeMessaging'))})
                        }
                    } catch { $failed = $true }
                }
            }
        } catch { $failed = $true }
    }
    $root = [IO.Path]::Combine($roaming,'Mozilla\Firefox\Profiles')
    try {
        if (SafePath $root) {
            $candidates = @([IO.Directory]::EnumerateDirectories($root) | Select-Object -First 17)
            if ($candidates.Count -gt 16) { $truncated = $true }
            foreach ($profile in @($candidates | Select-Object -First 16)) {
                $profiles++
                $file = [IO.Path]::Combine($profile,'extensions.json')
                if (!(SafePath $file)) { continue }
                try {
                    $json = SmallJson $file
                    if ($json.addons -isnot [array]) { throw 'Invalid Firefox inventory' }
                    foreach ($addon in $json.addons) {
                        if ($items.Count -ge 512) { $truncated = $true; break }
                        if ($addon.type -cne 'extension') { continue }
                        if ($addon.id -isnot [string] -or $addon.id -cnotmatch '^[A-Za-z0-9_.@{}-]{1,160}$') { throw 'Invalid extension ID' }
                        $items.Add(@{browser='Firefox';profile_index=$profiles;id=$addon.id;version=(Text $addon.version);enabled=(Prop $addon 'active');broad_host_access=@{state='Unknown';value='NotAssessed'};native_messaging=@{state='Unknown';value='NotAssessed'}})
                    }
                } catch { $failed = $true }
            }
        }
    } catch { $failed = $true }
    # Retain valid inventory on individual corrupt manifests; truncated also
    # means incomplete due to inaccessible/corrupt/reparse-point profiles.
    return @{extensions=(Known (Items $items.ToArray() ($truncated -or $failed)));profiles_examined=(Known $profiles)}
}
