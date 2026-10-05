# Every branch is a compiled, read-only probe. No arguments, registry paths,
# executable paths, module names or scriptblocks are accepted from observations.
try {
    $result = switch -CaseSensitive ($probe) {
        'UpdateCache' {
            $session = New-Object -ComObject Microsoft.Update.Session
            $searcher = $session.CreateUpdateSearcher()
            $searcher.Online = $false
            $searcher.IncludePotentiallySupersededUpdates = $false
            $result = $searcher.Search("IsInstalled=0 and Type='Software'")
            $items = @()
            $count = [Math]::Min($result.Updates.Count, 512)
            for ($i=0; $i -lt $count; $i++) {
                $update = $result.Updates.Item($i)
                $quality = $false
                for ($j=0; $j -lt [Math]::Min($update.Categories.Count,64); $j++) {
                    # Security Updates, Update Rollups, Critical Updates.
                    if ($update.Categories.Item($j).CategoryID -in @('0fa1201d-4330-4fa8-8ae9-b877473b6441','28bc880e-0592-4cbf-8f95-c79b17911d5f','e6cf1350-c01b-414d-a61f-263d14d133b4')) { $quality = $true }
                }
                $kb = @()
                for ($j=0; $j -lt [Math]::Min($update.KBArticleIDs.Count,16); $j++) {
                    $number = $update.KBArticleIDs.Item($j)
                    if ($number -isnot [string] -or $number -cnotmatch '^[0-9]{4,10}$') { throw 'Invalid KB identifier' }
                    $kb += $number
                }
                $items += @{quality_classification=$quality;kb=$kb;hidden=(Prop $update 'IsHidden')}
            }
            @{result_code=(Known ([int]$result.ResultCode));missing=(Known (Items $items ($result.Updates.Count -gt 512)))}
        }
        'UpdateHistory' {
            $session = New-Object -ComObject Microsoft.Update.Session
            $searcher = $session.CreateUpdateSearcher()
            $total = $searcher.GetTotalHistoryCount()
            $entries = @()
            if ($total -gt 0) {
                foreach ($item in $searcher.QueryHistory(0, [Math]::Min($total,256))) {
                    # Never emit titles (they may be localized or contain arbitrary text).
                    $entries += @{operation=[int]$item.Operation;result_code=[int]$item.ResultCode;hresult=[int]$item.HResult;date_unix_seconds=(UnixTime $item.Date);quality_title_hint=($item.Title -match '(?i)cumulative|security update|quality update')}
                }
            }
            @{entries=(Known (Items $entries ($total -gt 256)))}
        }
        'DefenderHealth' {
            Load 'Defender'
            $s = Get-MpComputerStatus
            @{service_enabled=(Prop $s 'AMServiceEnabled');antivirus_enabled=(Prop $s 'AntivirusEnabled');realtime_enabled=(Prop $s 'RealTimeProtectionEnabled');behavior_enabled=(Prop $s 'BehaviorMonitorEnabled');ioav_enabled=(Prop $s 'IoavProtectionEnabled');nis_enabled=(Prop $s 'NISEnabled');signatures_age_days=(Prop $s 'AntivirusSignatureAge');signatures_out_of_date=(Prop $s 'DefenderSignaturesOutOfDate');tamper_protected=(Prop $s 'IsTamperProtected');running_mode=(Prop $s 'AMRunningMode')}
        }
        'DefenderPolicy' {
            Load 'Defender'
            $s = Get-MpPreference
            $asr = Fact {
                if ($null -eq $s.PSObject.Properties['AttackSurfaceReductionRules_Ids'] -or $null -eq $s.PSObject.Properties['AttackSurfaceReductionRules_Actions']) { throw 'No ASR properties' }
                $ids = @($s.AttackSurfaceReductionRules_Ids | Where-Object { $null -ne $_ })
                $actions = @($s.AttackSurfaceReductionRules_Actions | Where-Object { $null -ne $_ })
                if ($ids.Count -ne $actions.Count) { throw 'Mismatched ASR arrays' }
                $items = @(); $seen = @{}
                for ($i=0; $i -lt [Math]::Min($ids.Count,512); $i++) {
                    $guid = [Guid]::Parse($ids[$i]).ToString()
                    if ($seen.ContainsKey($guid)) { throw 'Duplicate ASR rule' }
                    $seen[$guid] = $true
                    $items += @{id=$guid;mode=$actions[$i]}
                }
                Items $items ($ids.Count -gt 512)
            }
            @{asr=$asr;cfa_mode=(Code $s 'EnableControlledFolderAccess')}
        }
        'SecurityProviders' {
            $values = @{}
            foreach ($spec in @(@('antivirus','AntiVirusProduct'), @('firewall','FirewallProduct'))) {
                $class = $spec[1]
                $values[$spec[0]] = Fact {
                    $rows = @(Cim $class 'root\SecurityCenter2' | Select-Object -First 513)
                    $items = @($rows | Select-Object -First 512 | ForEach-Object { @{name=(Text $_.displayName);instance_guid=([Guid]::Parse($_.instanceGuid).ToString());product_state=$_.productState} })
                    Items $items ($rows.Count -gt 512)
                }
            }
            $values
        }
        'Management' {
            @{domain_joined=(Fact { (Cim 'Win32_ComputerSystem')[0].PartOfDomain });mdm_registered=(Fact { MdmRegistered });cloud_join_indicator=(Fact { ChildIndicator 'SYSTEM\CurrentControlSet\Control\CloudDomainJoin\JoinInfo' });defender_policy_values=(Fact { (PolicyValues 'SOFTWARE\Policies\Microsoft\Windows Defender') -or (PolicyValues 'SOFTWARE\Policies\Microsoft\Windows Defender\Real-Time Protection') -or (PolicyValues 'SOFTWARE\Policies\Microsoft\Windows Defender\Windows Defender Exploit Guard\ASR\Rules') -or (PolicyValues 'SOFTWARE\Policies\Microsoft\Windows Defender\Windows Defender Exploit Guard\Controlled Folder Access') });update_policy_values=(Fact { (PolicyValues 'SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate') -or (PolicyValues 'SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU') });policy_manager_values=(Fact { (PolicyValues 'SOFTWARE\Microsoft\PolicyManager\current\device\Defender') -or (PolicyValues 'SOFTWARE\Microsoft\PolicyManager\current\device\Update') })}
        }
        'SecureBoot' { Load 'SecureBoot'; @{enabled=(Fact { Confirm-SecureBootUEFI })} }
        'Tpm' {
            Load 'TrustedPlatformModule'
            $s = Get-Tpm
            @{present=(Prop $s 'TpmPresent');ready=(Prop $s 'TpmReady');enabled=(Prop $s 'TpmEnabled');activated=(Prop $s 'TpmActivated')}
        }
        'BitLocker' {
            Load 'CimCmdlets'
            # Get-BitLockerVolume also materializes key-protector data internally.
            # Use only these two documented read-only WMI methods instead.
            $rows = @(Get-CimInstance -Namespace 'root\CIMV2\Security\MicrosoftVolumeEncryption' -ClassName Win32_EncryptableVolume -OperationTimeoutSec 5 | Select-Object -First 513)
            $items = @($rows | Select-Object -First 512 | ForEach-Object {
                $protection = $null; $conversion = $null
                try {
                    $p = Invoke-CimMethod -InputObject $_ -MethodName GetProtectionStatus -OperationTimeoutSec 5
                    if (($p.ReturnValue -is [uint32] -or $p.ReturnValue -is [int]) -and $p.ReturnValue -eq 0) { $protection = $p }
                } catch {}
                try {
                    $c = Invoke-CimMethod -InputObject $_ -MethodName GetConversionStatus -OperationTimeoutSec 5
                    if (($c.ReturnValue -is [uint32] -or $c.ReturnValue -is [int]) -and $c.ReturnValue -eq 0) { $conversion = $c }
                } catch {}
                @{protection_status=(Prop $protection 'ProtectionStatus');volume_status=(Prop $conversion 'ConversionStatus');encryption_percentage=(Prop $conversion 'EncryptionPercentage')}
            })
            @{volumes=(Known (Items $items ($rows.Count -gt 512)))}
        }
        'Vbs' {
            $rows = @(Cim 'Win32_DeviceGuard' 'root\Microsoft\Windows\DeviceGuard')
            if ($rows.Count -ne 1) { throw 'Ambiguous DeviceGuard result' }
            $s = $rows[0]
            @{status=(Prop $s 'VirtualizationBasedSecurityStatus');configured_services=(Prop $s 'SecurityServicesConfigured');running_services=(Prop $s 'SecurityServicesRunning')}
        }
        'Accounts' {
            Load 'Microsoft.PowerShell.LocalAccounts'
            @{administrator_count=(Fact { $members=@(Get-LocalGroupMember -SID 'S-1-5-32-544' | Select-Object -First 513); if ($members.Count -gt 512) { throw 'Member cap' }; $members.Count });guest_enabled=(Fact { $guests=@(Get-LocalUser | Where-Object { $_.SID.Value -match '-501$' }); if ($guests.Count -ne 1) { throw 'Guest identity unknown' }; $guests[0].Enabled })}
        }
        'RemoteAccess' {
            $server = $null; $client = $null
            try { Load 'SmbShare'; $server = Get-SmbServerConfiguration } catch {}
            try { Load 'SmbShare'; $client = Get-SmbClientConfiguration } catch {}
            $rdpListener = Fact { Load 'NetTCPIP'; $key=Get-Item -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server\WinStations\RDP-Tcp'; if ($key.GetValueKind('PortNumber') -ne [Microsoft.Win32.RegistryValueKind]::DWord) { throw 'Unknown RDP port' }; $port=$key.GetValue('PortNumber'); if ($port -lt 1 -or $port -gt 65535) { throw 'Unknown RDP port' }; $listeners=@(Get-NetTCPConnection -State Listen); @($listeners | Where-Object { $_.LocalPort -eq $port }).Count -gt 0 }
            $smbListener = Fact { Load 'NetTCPIP'; $listeners=@(Get-NetTCPConnection -State Listen); @($listeners | Where-Object { $_.LocalPort -eq 445 }).Count -gt 0 }
            @{rdp_denied=(RegBool 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server' 'fDenyTSConnections');rdp_nla_required=(RegBool 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server\WinStations\RDP-Tcp' 'UserAuthentication');rdp_listener=$rdpListener;smb_listener=$smbListener;smb1_enabled=(Prop $server 'EnableSMB1Protocol');smb2_enabled=(Prop $server 'EnableSMB2Protocol');smb_server_signing_required=(Prop $server 'RequireSecuritySignature');smb_client_signing_required=(Prop $client 'RequireSecuritySignature');smb_guest_logons_enabled=(Prop $client 'EnableInsecureGuestLogons')}
        }
        'Software' {
            $items = @(); $truncated = $false
            foreach ($view in @([Microsoft.Win32.RegistryView]::Registry64,[Microsoft.Win32.RegistryView]::Registry32)) {
                $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, $view)
                try {
                    $root = $base.OpenSubKey('SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall', $false)
                    if ($null -eq $root) { continue }
                    try {
                        foreach ($name in $root.GetSubKeyNames()) {
                            if ($items.Count -ge 512) { $truncated = $true; break }
                            $key = $root.OpenSubKey($name, $false)
                            if ($null -eq $key) { throw 'Uninstall key disappeared' }
                            try {
                                $display = $key.GetValue('DisplayName', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                                if ($null -eq $display) { continue }
                                $items += @{name=(Text $display);publisher=(Text $key.GetValue('Publisher','',[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames));version=(Text $key.GetValue('DisplayVersion','',[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames))}
                            } finally { $key.Dispose() }
                        }
                    } finally { $root.Dispose() }
                } finally { $base.Dispose() }
            }
            @{applications=(Known (Items $items $truncated))}
        }
        'BrowserExtensions' { BrowserInventory }
        'Storage' {
            Load 'Storage'
            $rows = @(Get-PhysicalDisk | Select-Object -First 513)
            $items = @($rows | Select-Object -First 512 | ForEach-Object {
                $counter = $null
                try { $counter = $_ | Get-StorageReliabilityCounter } catch {}
                @{health_status=(Code $_ 'HealthStatus');temperature_celsius=(Prop $counter 'Temperature');wear_percent=(Prop $counter 'Wear');read_errors_uncorrected=(Prop $counter 'ReadErrorsUncorrected');write_errors_uncorrected=(Prop $counter 'WriteErrorsUncorrected')}
            })
            @{disks=(Known (Items $items ($rows.Count -gt 512)))}
        }
        'Ntfs' {
            Load 'CimCmdlets'
            $rows = @(Get-CimInstance -ClassName Win32_Volume -Filter 'DriveType=3' -OperationTimeoutSec 5 | Select-Object -First 513)
            $items = @($rows | Select-Object -First 512 | ForEach-Object { @{filesystem=(Prop $_ 'FileSystem');dirty=(Prop $_ 'DirtyBitSet');capacity_bytes=(Prop $_ 'Capacity');free_bytes=(Prop $_ 'FreeSpace')} })
            @{volumes=(Known (Items $items ($rows.Count -gt 512)))}
        }
        'Backup' {
            $shadows = Fact { $rows = @(Cim 'Win32_ShadowCopy' | Select-Object -First 513); if ($rows.Count -gt 512) { throw 'Shadow cap' }; $rows.Count }
            $events = Fact {
                Load 'Microsoft.PowerShell.Diagnostics'
                $rows = @()
                try { $rows = @(Get-WinEvent -FilterHashtable @{LogName='Microsoft-Windows-Backup';ProviderName='Microsoft-Windows-Backup';Id=4;StartTime=[DateTime]::Now.AddDays(-90)} -MaxEvents 65) }
                catch { if ($_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound,*') { throw } }
                $items = @($rows | Select-Object -First 64 | ForEach-Object { @{date_unix_seconds=(UnixTime $_.TimeCreated)} })
                Items $items ($rows.Count -gt 64)
            }
            @{shadow_copy_count=$shadows;success_events=$events}
        }
        'Adapters' {
            Load 'NetAdapter'
            $rows = @(Get-NetAdapter -IncludeHidden | Select-Object -First 513)
            $items = @($rows | Select-Object -First 512 | ForEach-Object { @{operational_status=(Code $_ 'InterfaceOperationalStatus');hardware_interface=(Prop $_ 'HardwareInterface')} })
            @{adapters=(Known (Items $items ($rows.Count -gt 512)))}
        }
        'Dns' {
            Load 'DnsClient'
            $rows = @(Get-DnsClientServerAddress | Select-Object -First 513)
            $items = @($rows | Select-Object -First 512 | ForEach-Object { if ($null -eq $_.PSObject.Properties['ServerAddresses'] -or $null -eq $_.ServerAddresses) { throw 'Missing DNS property' }; @{address_family=$_.AddressFamily;server_count=@($_.ServerAddresses).Count} })
            @{interfaces=(Known (Items $items ($rows.Count -gt 512)))}
        }
        'Vpn' {
            Load 'VpnClient'
            $rows = @(Get-VpnConnection -AllUserConnection | Select-Object -First 513)
            $items = @($rows | Select-Object -First 512 | ForEach-Object {
                $connected = Unknown
                if ([string]$_.ConnectionStatus -ceq 'Connected') { $connected = Known $true }
                elseif ([string]$_.ConnectionStatus -ceq 'Disconnected') { $connected = Known $false }
                @{connected=$connected;split_tunneling=(Prop $_ 'SplitTunneling')}
            })
            @{connections=(Known (Items $items ($rows.Count -gt 512)))}
        }
        'OsSupport' {
            $version = 'SOFTWARE\Microsoft\Windows NT\CurrentVersion'
            @{
                display_version=(Fact { $v = HklmValue $version 'DisplayVersion'; if ($v -isnot [string] -or $v -cnotmatch '^[0-9A-Za-z]{2,8}$') { throw 'Invalid version' }; $v })
                build=(Fact { $v = HklmValue $version 'CurrentBuildNumber'; if ($v -isnot [string] -or $v -cnotmatch '^[0-9]{4,6}$') { throw 'Invalid build' }; [int]$v })
                edition_id=(Fact { $v = HklmValue $version 'EditionID'; if ($v -isnot [string] -or $v -cnotmatch '^[0-9A-Za-z]{2,40}$') { throw 'Invalid edition' }; $v })
            }
        }
        'SecureBootCerts' {
            Load 'SecureBoot'
            Load 'Microsoft.PowerShell.Diagnostics'
            # Event ids only: message text can carry firmware or device details and is never read.
            $ids = $null
            try {
                $rows = @()
                try { $rows = @(Get-WinEvent -FilterHashtable @{LogName='System';Id=@(1795,1796,1797,1798,1801,1808);StartTime=[DateTime]::Now.AddDays(-400)} -MaxEvents 64) }
                catch { if ($_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound,*') { throw } }
                $ids = @($rows | ForEach-Object { [int]$_.Id })
            } catch { $ids = $null }
            $sb = $false
            try { $sb = [bool](Confirm-SecureBootUEFI) } catch { $sb = $false }
            $flag = { param($wanted) if ($null -eq $ids) { return (Unknown) }; $hit = $false; foreach ($i in $ids) { if ($i -in $wanted) { $hit = $true } }; return (Known $hit) }
            @{
                update_completed_event=(& $flag @(1808))
                update_staged_event=(& $flag @(1801))
                update_error_event=(& $flag @(1795,1796,1797,1798))
                servicing_status=(Fact {
                    $v = HklmValue 'SYSTEM\CurrentControlSet\Control\SecureBoot\Servicing' 'UEFICA2023Status'
                    if ($null -eq $v) { return 'Absent' }
                    if ($v -isnot [string]) { throw 'Wrong registry type' }
                    if ($v -cin @('NotStarted','InProgress','Updated')) { return $v }
                    return 'Other'
                })
                ca2023_in_db=(Fact {
                    if (-not $sb) { return $false }
                    $db = Get-SecureBootUEFI -Name db
                    [Text.Encoding]::ASCII.GetString($db.Bytes).Contains('Windows UEFI CA 2023')
                })
                secure_boot_enabled=(Known $sb)
            }
        }
        'DefenderProtection' {
            Load 'Defender'
            $s = Get-MpComputerStatus
            $p = Get-MpPreference
            # Counts only. Exclusion paths, extensions and process names are never emitted.
            $total = $null; $risky = $null
            try {
                $paths = @($p.ExclusionPath | Where-Object { $null -ne $_ })
                $extensions = @($p.ExclusionExtension | Where-Object { $null -ne $_ })
                $processes = @($p.ExclusionProcess | Where-Object { $null -ne $_ })
                if ($paths.Count + $extensions.Count + $processes.Count -gt 4096) { throw 'Exclusion cap' }
                foreach ($entry in @($paths + $extensions + $processes)) { if ([string]$entry -like 'N/A*') { throw 'Exclusions masked' } }
                $n = 0
                foreach ($entry in $paths) {
                    $x = ([string]$entry).Trim().ToLowerInvariant().TrimEnd('\')
                    if ($x -match '^[a-z]:$|^\*$|^[a-z]:\\(windows(\\(temp|system32|syswow64))?|programdata|users(\\public.*|\\[^\\]+(\\(downloads|desktop|documents|appdata(\\local(\\temp)?|\\roaming)?))?)?)?$|(^|\\)temp$|^%(userprofile|temp|tmp|appdata|localappdata|public|systemroot|windir|systemdrive|homedrive)%(\\(downloads|desktop|documents|temp))?$') { $n++ }
                }
                foreach ($entry in $extensions) {
                    $x = ([string]$entry).Trim().ToLowerInvariant().TrimStart('.')
                    if ($x -cin @('exe','dll','ps1','bat','cmd','js','vbs','vbe','scr','msi','com','hta','jar','*')) { $n++ }
                }
                foreach ($entry in $processes) {
                    $x = ([string]$entry).Trim().ToLowerInvariant()
                    $leaf = $x.Substring($x.LastIndexOf('\') + 1)
                    if ($leaf -cin @('powershell.exe','pwsh.exe','cmd.exe','wscript.exe','cscript.exe','mshta.exe','rundll32.exe','regsvr32.exe','*')) { $n++ }
                }
                $total = $paths.Count + $extensions.Count + $processes.Count
                $risky = $n
            } catch { $total = $null; $risky = $null }
            @{
                running_mode=(Prop $s 'AMRunningMode')
                tamper_protected=(Prop $s 'IsTamperProtected')
                tamper_feature_value=(Fact { $v = HklmDword 'SOFTWARE\Microsoft\Windows Defender\Features' 'TamperProtection'; if ($null -eq $v) { return 0 }; if ($v -lt 0 -or $v -gt 255) { throw 'Unknown tamper value' }; $v })
                active_threats=(Fact { @(Get-MpThreat | Where-Object { $_.IsActive }).Count })
                recent_detections=(Fact { $cut = (Get-Date).AddDays(-30); @(Get-MpThreatDetection | Where-Object { $_.InitialDetectionTime -gt $cut }).Count })
                quick_scan_age_days=(Prop $s 'QuickScanAge')
                full_scan_age_days=(Prop $s 'FullScanAge')
                exclusion_count=(Counted $total)
                risky_exclusion_count=(Counted $risky)
            }
        }
        'SmartScreen' {
            $explorer = 'SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer'
            @{
                apps_off_local=(Fact { $v = HklmValue $explorer 'SmartScreenEnabled'; if ($null -eq $v) { return $false }; if ($v -isnot [string]) { throw 'Wrong registry type' }; $v -ceq 'Off' })
                apps_off_policy=(Fact { $v = HklmDword 'SOFTWARE\Policies\Microsoft\Windows\System' 'EnableSmartScreen'; ($null -ne $v) -and ($v -eq 0) })
                edge_off_policy=(Fact { $v = HklmDword 'SOFTWARE\Policies\Microsoft\Edge' 'SmartScreenEnabled'; ($null -ne $v) -and ($v -eq 0) })
                chrome_off_policy=(Fact { $v = HklmDword 'SOFTWARE\Policies\Google\Chrome' 'SafeBrowsingProtectionLevel'; $w = HklmDword 'SOFTWARE\Policies\Google\Chrome' 'SafeBrowsingEnabled'; (($null -ne $v) -and ($v -eq 0)) -or (($null -ne $w) -and ($w -eq 0)) })
                smart_app_control=(Fact {
                    $v = HklmDword 'SYSTEM\CurrentControlSet\Control\CI\Policy' 'VerifiedAndReputablePolicyState'
                    if ($null -eq $v) { return 'Absent' }
                    switch ($v) { 0 { return 'Off' } 1 { return 'On' } 2 { return 'Evaluation' } default { throw 'Unknown state' } }
                })
            }
        }
        'UpdatePolicy' {
            $wu = 'SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate'
            $ux = 'SOFTWARE\Microsoft\WindowsUpdate\UX\Settings'
            @{
                auto_updates_blocked=(Fact { $a = HklmDword "$wu\AU" 'NoAutoUpdate'; $o = HklmDword "$wu\AU" 'AUOptions'; (($null -ne $a) -and ($a -eq 1)) -or (($null -ne $o) -and ($o -eq 1)) })
                update_access_blocked=(Fact { $v = HklmDword $wu 'DisableWindowsUpdateAccess'; ($null -ne $v) -and ($v -eq 1) })
                update_service_disabled=(Fact {
                    $off = $false
                    foreach ($svc in @('wuauserv','UsoSvc','BITS')) { $v = HklmDword "SYSTEM\CurrentControlSet\Services\$svc" 'Start'; if (($null -ne $v) -and ($v -eq 4)) { $off = $true } }
                    $off
                })
                paused=(Fact {
                    $now = [DateTime]::UtcNow; $paused = $false
                    foreach ($name in @('PauseUpdatesExpiryTime','PauseFeatureUpdatesEndTime','PauseQualityUpdatesEndTime')) {
                        $v = HklmValue $ux $name
                        if ($v -is [string]) {
                            $when = [DateTime]::MinValue
                            if ([DateTime]::TryParse($v, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal, [ref]$when) -and $when -gt $now) { $paused = $true }
                        }
                    }
                    $paused
                })
                drivers_excluded=(Fact { $v = HklmDword $wu 'ExcludeWUDriversInQualityUpdate'; ($null -ne $v) -and ($v -eq 1) })
                reboot_pending=(Fact { (HklmKeyExists 'SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired') -or (HklmKeyExists 'SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending') })
                uptime_days=(Fact {
                    $boot = (Cim 'Win32_OperatingSystem')[0].LastBootUpTime
                    if ($boot -isnot [DateTime]) { throw 'Missing boot time' }
                    $days = [Math]::Floor(([DateTime]::Now - $boot).TotalDays)
                    if ($days -lt 0) { $days = 0 }
                    [int][Math]::Min($days, 36500)
                })
            }
        }
        'LegacyFeatures' {
            Load 'Dism'
            @{
                powershell_v2_enabled=(Fact {
                    $f = Get-WindowsOptionalFeature -Online -FeatureName 'MicrosoftWindowsPowerShellV2Root'
                    $state = [string]$f.State
                    if ($state -ceq 'Enabled') { return $true }
                    if ($state -cin @('Disabled','DisabledWithPayloadRemoved')) { return $false }
                    throw 'Unknown feature state'
                })
            }
        }
        'HostsFile' {
            $hosts = [IO.Path]::Combine($env:SystemRoot, 'System32\drivers\etc\hosts')
            $size = $null
            try { $info = New-Object IO.FileInfo $hosts; $size = $(if ($info.Exists) { [long]$info.Length } else { [long]0 }) } catch { $size = $null }
            $redirects = $null; $sensitiveRedirects = $null; $sensitiveBlocks = $null
            try {
                if ($null -eq $size -or $size -gt 4194304) { throw 'Hosts file too large to inspect' }
                $r = 0; $sr = 0; $sb = 0
                if ($size -gt 0) {
                    $lines = [IO.File]::ReadAllLines($hosts)
                    if ($lines.Count -gt 20000) { throw 'Line cap' }
                    foreach ($line in $lines) {
                        $text = $line; $hash = $text.IndexOf('#'); if ($hash -ge 0) { $text = $text.Substring(0, $hash) }
                        $parts = @($text.Trim() -split '\s+' | Where-Object { $_ -ne '' })
                        if ($parts.Count -lt 2) { continue }
                        $address = $null
                        if (-not [Net.IPAddress]::TryParse($parts[0], [ref]$address)) { continue }
                        $loop = $parts[0] -cin @('127.0.0.1','::1','0.0.0.0','::')
                        $names = @($parts[1..($parts.Count - 1)] | ForEach-Object { $_.ToLowerInvariant() })
                        $broad = $false; $update = $false
                        foreach ($h in $names) {
                            if ($h -match '(^|\.)(microsoft|windowsupdate|windows|live|office|office365|msedge|xbox)\.(com|net)$|defender|kaspersky|avast|avg\.com|norton|symantec|mcafee|malwarebytes|bitdefender|eset\.|sophos|trendmicro|avira|webroot|paypal|bank|chase\.com|wellsfargo|citibank|hsbc|barclays|santander|capitalone|americanexpress|revolut') { $broad = $true }
                            if ($h -match 'windowsupdate\.com$|(^|\.)update\.microsoft\.com$|(^|\.)download\.microsoft\.com$|(^|\.)smartscreen[^.]*\.microsoft\.com$|(^|\.)wdcp\.microsoft\.com$|defender|kaspersky|avast|norton|symantec|mcafee|malwarebytes|bitdefender|eset\.|sophos|trendmicro|avira|webroot') { $update = $true }
                        }
                        if ($loop) { if ($update) { $sb++ } }
                        else { $r++; if ($broad) { $sr++ } }
                    }
                }
                $redirects = $r; $sensitiveRedirects = $sr; $sensitiveBlocks = $sb
            } catch { $redirects = $null; $sensitiveRedirects = $null; $sensitiveBlocks = $null }
            @{
                size_bytes=$(if ($null -eq $size) { Unknown } else { Known $size })
                redirect_count=(Counted $redirects)
                sensitive_redirect_count=(Counted $sensitiveRedirects)
                sensitive_block_count=(Counted $sensitiveBlocks)
            }
        }
        'Persistence' {
            $unquoted = $null; $writable = $null
            try {
                $broadWriters = @('S-1-1-0','S-1-5-11','S-1-5-32-545')
                $dirCache = @{}
                $isWritable = {
                    param($dir)
                    if ($dirCache.ContainsKey($dir)) { return $dirCache[$dir] }
                    $result = $false
                    try {
                        if ([IO.Directory]::Exists($dir)) {
                            $acl = (New-Object IO.DirectoryInfo $dir).GetAccessControl([Security.AccessControl.AccessControlSections]::Access)
                            foreach ($rule in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
                                if ($rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow) { continue }
                                if ($rule.IdentityReference.Value -cnotin $broadWriters) { continue }
                                if (([int]$rule.PropagationFlags -band 2) -ne 0) { continue }
                                if ((([int]$rule.FileSystemRights) -band (2 -bor 262144 -bor 524288)) -ne 0) { $result = $true }
                            }
                        }
                    } catch { $result = $false }
                    $dirCache[$dir] = $result
                    return $result
                }
                $root = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Services', $false)
                if ($null -eq $root) { throw 'No services key' }
                $count = 0; $risky = 0
                try {
                    $names = $root.GetSubKeyNames()
                    if ($names.Count -gt 4096) { throw 'Service cap' }
                    foreach ($name in $names) {
                        $key = $root.OpenSubKey($name, $false)
                        if ($null -eq $key) { continue }
                        try {
                            $type = $key.GetValue('Type', $null)
                            $image = $key.GetValue('ImagePath', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                            if ($type -isnot [int] -or ($type -band 0x30) -eq 0 -or $image -isnot [string]) { continue }
                            $image = $image.Trim()
                            if ($image.StartsWith('"') -or $image -notmatch '^(?<exe>[A-Za-z]:\\.*?\.exe)(\s|$)') { continue }
                            $exe = $Matches['exe']
                            if (-not $exe.Contains(' ')) { continue }
                            $count++
                            $hit = $false
                            $at = $exe.IndexOf(' ')
                            while ($at -ge 0) {
                                $dir = [IO.Path]::GetDirectoryName($exe.Substring(0, $at))
                                if ($null -ne $dir -and (& $isWritable $dir)) { $hit = $true }
                                $at = $exe.IndexOf(' ', $at + 1)
                            }
                            if ($hit) { $risky++ }
                        } finally { $key.Dispose() }
                    }
                } finally { $root.Dispose() }
                $unquoted = $count; $writable = $risky
            } catch { $unquoted = $null; $writable = $null }
            @{
                wmi_consumers=(Fact { $n = 0; foreach ($class in @('CommandLineEventConsumer','ActiveScriptEventConsumer')) { $n += @(Cim $class 'root\subscription').Count }; $n })
                unquoted_service_paths=(Counted $unquoted)
                unquoted_service_paths_writable=(Counted $writable)
            }
        }
        'AccountHygiene' {
            Load 'Microsoft.PowerShell.LocalAccounts'
            @{
                builtin_admin_enabled=(Fact { $u = @(Get-LocalUser | Where-Object { $_.SID.Value -cmatch '^S-1-5-21-[0-9-]+-500$' }); if ($u.Count -ne 1) { throw 'Built-in administrator not found' }; [bool]$u[0].Enabled })
                stale_enabled_accounts=(Fact { $cut = (Get-Date).AddDays(-180); @(Get-LocalUser | Where-Object { $_.Enabled -and $_.SID.Value -cnotmatch '-(500|501|503|504)$' -and $null -ne $_.LastLogon -and $_.LastLogon -lt $cut }).Count })
            }
        }
        'Sharing' {
            Load 'SmbShare'
            $server = $null
            try { $server = Get-SmbServerConfiguration } catch {}
            $shares = $null
            try { $shares = @(Get-SmbShare | Where-Object { -not $_.Special } | Select-Object -First 65); if ($shares.Count -gt 64) { throw 'Share cap' } } catch { $shares = $null }
            @{
                share_count=$(if ($null -eq $shares) { Unknown } else { Known $shares.Count })
                broad_access_shares=(Fact {
                    if ($null -eq $shares) { throw 'No shares' }
                    $broad = @()
                    foreach ($sid in @('S-1-1-0','S-1-5-7','S-1-5-32-546')) { $broad += ([Security.Principal.SecurityIdentifier]$sid).Translate([Security.Principal.NTAccount]).Value }
                    $n = 0
                    foreach ($share in $shares) {
                        $hit = $false
                        foreach ($access in @(Get-SmbShareAccess -Name $share.Name)) {
                            if ([string]$access.AccessControlType -ceq 'Allow' -and [string]$access.AccessRight -cin @('Change','Full') -and ([string]$access.AccountName) -in $broad) { $hit = $true }
                        }
                        if ($hit) { $n++ }
                    }
                    $n
                })
                encrypt_data=(Prop $server 'EncryptData')
            }
        }
        'FirewallRules' {
            Load 'NetSecurity'
            $risky = $null; $user = $null
            try {
                $rules = @(Get-NetFirewallRule -PolicyStore ActiveStore -Enabled True -Direction Inbound -Action Allow | Select-Object -First 1025)
                if ($rules.Count -gt 1024) { throw 'Rule cap' }
                $r = 0; $u = 0
                foreach ($rule in $rules) {
                    $filter = $rule | Get-NetFirewallApplicationFilter
                    $program = ([string]$filter.Program).ToLowerInvariant()
                    if ($program -eq '' -or $program -ceq 'any') { continue }
                    if ($program -match '\\users\\|%userprofile%|%appdata%|%localappdata%|%public%|%temp%') { $u++ }
                    if ($program -match '\\users\\[^\\]+\\(downloads|desktop|appdata\\local\\temp)\\|\\users\\public\\|\\windows\\temp\\|%userprofile%\\(downloads|desktop)\\|%temp%\\|%public%\\') { $r++ }
                }
                $risky = $r; $user = $u
            } catch { $risky = $null; $user = $null }
            @{ risky_inbound_allow_rules=(Counted $risky); user_folder_inbound_allow_rules=(Counted $user) }
        }
        default { throw 'Invalid compiled probe' }
    }
    ConvertTo-Json -InputObject $result -Depth 16 -Compress
} catch {
    # No exception text: provider errors can embed user names, paths or endpoints.
    [Console]::Out.Write('{}')
    exit 1
}
