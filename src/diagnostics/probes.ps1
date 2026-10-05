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
        default { throw 'Invalid compiled probe' }
    }
    ConvertTo-Json -InputObject $result -Depth 16 -Compress
} catch {
    # No exception text: provider errors can embed user names, paths or endpoints.
    [Console]::Out.Write('{}')
    exit 1
}
