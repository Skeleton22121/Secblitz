# Secblitz: more automatic checks and hardening (synthesis)

Date: 2026-10-05. Merged from two research reports (A: OS/credential/network; B: protection/update/accounts/privacy), de-duplicated against `src/` (engine.rs, platform/backend.ps1, diagnostics/rules.rs + probes.ps1, readiness, actions). Nothing implemented; nothing committed.

Legend. Mode: AUTO = auto-fix (safe, reversible, journaled), ASK = ask-user, DETECT = detect-only/tip. Risk = breakage risk (L/M/H). Edition H/P = Home and Pro. Source tags: [V] value re-read on Microsoft Learn in the synthesis spot-check; [K] known from baselines/CIS/STIG, verify on the UI-clone VM before shipping an auto-fix; [U] unverified, ship detect-only.

## 0. Global rules (apply to every item)

1. Reuse the existing refusal gate: domain-joined, MDM-managed, third-party AV, tamper-protection-on (for Defender writes). Re-read after every write; report "blocked by tamper protection" if it did not stick.
2. `Set-MpPreference -AttackSurfaceReductionRules_Ids` replaces the whole list. Always `Add-MpPreference` or read-merge-write.
3. Prefer cmdlets or non-policy keys over `HKLM\SOFTWARE\Policies\...` (policy keys can show a "managed by your organization" banner). Where a policy key is the only option, it is flagged.
4. HKCU items must be applied to the original user's hive (loaded `HKU\<SID>`), not SYSTEM's.
5. Journal the prior value before every change; undo restores the journal, not a hardcoded default.

## 1. Summary counts

- Candidates in the two reports: 50 (A) + 54 (B) = 104.
- Merged/de-duplicated to **72 unique candidates** (12 Defender, 4 reputation, 10 OS hardening, 3 credentials, 15 network, 2 remote access, 7 accounts/session, 8 update hygiene, 1 Office, 3 persistence, 7 privacy).
- By mode: AUTO 21, ASK 31, DETECT/tip 20.
- Marked "extends existing": 12 (asr.*, SmartScreen finding, Defender exclusions finding, defender.signatures, proxy.machine_default, lsa.restrict_anonymous_sam, accounts.admin_membership, Windows lifecycle finding, update.freshness, dns.configuration, vbs.memory_integrity, software.end_of_support).
- Excluded: 17 (see section 5).
- Spot-check of 10 values against Microsoft Learn: 8 confirmed, 2 partially (see 1a).

### 1a. Spot-check results

| # | Claim | Result |
|---|---|---|
| 1 | LSA `RunAsPPL` 1 = UEFI lock, 2 = no lock (22H2+); event 12 in System/WinInit; audit events 3033/3063/3065/3066 in CodeIntegrity; SAC suppresses audit events; audit on by default 22H2+ | Confirmed |
| 2 | Auto-enable of LSA PPL only on NEW 22H2+ installs that are enterprise-joined and HVCI-capable | Confirmed, so Home PCs are normally off |
| 3 | `HKLM\SOFTWARE\Policies\Microsoft\Windows\System` `EnableSmartScreen` 0/1 | Confirmed (CSP lists Pro+; Home relies on the non-policy `SmartScreenEnabled` string [K]) |
| 4 | `Set-MpPreference -PUAProtection` Disabled/Enabled/AuditMode | Confirmed |
| 5 | `-EnableNetworkProtection` Disabled/Enabled/AuditMode | Confirmed (Pro/Enterprise per Network protection page) |
| 6 | `-MAPSReporting` 0 = Disabled (parameter default), 1 basic, 2 advanced | Confirmed. A PC can therefore read 0 |
| 7 | LLMNR policy `HKLM\Software\Policies\Microsoft\Windows NT\DNSClient\EnableMulticast` | Confirmed |
| 8 | Recall: `WindowsAI\DisableAIDataAnalysis` (1 = no snapshots, user+device, Pro+), `AllowRecallEnablement` (0 = removed, restart); not configured = snapshots not saved | Confirmed. Recall is opt-in |
| 9 | Windows 11 Home/Pro end dates: 24H2 2026-10-14, 25H2 2027-10-13, 26H1 2028-03-15, 26H2 2028-10-10 (23H2 ended 2025-11-12) | Confirmed. 24H2 ends in 9 days |
| 10 | Point and Print `RestrictDriverInstallationToAdministrators` (default 1, 0 = non-admin install) | Confirmed. `NoWarningNoElevationOnInstall` / `UpdatePromptSettings` are NOT on KB5005652; demoted to [K] |

Not confirmed (kept as [K]/[U], gated by VM verification): `VulnerableDriverBlocklistEnable` registry name, `SmartScreenEnabled` string, `UEFICA2023Status`, `DisableWpad`, `PauseUpdatesExpiryTime`, `EnableMDNS`, Nearby Sharing keys, KernelShadowStacks, `CloudBlockLevel`/`SubmitSamplesConsent` enum names.

## 2. Ranked table

Rank = value to a non-technical user x confidence x safety. "Ext" = extends an existing id.

| Rank | id | Plain name | Category | Mode | Sev | Risk | Ed |
|---|---|---|---|---|---|---|---|
| 1 | os.feature_release_support | Your Windows version is about to stop getting updates (Ext lifecycle) | Update | DETECT | high | L | H/P |
| 2 | defender.threats_and_scan_age | Unresolved threats and stale scans | Defender | DETECT | high | L | H/P |
| 3 | defender.tamper_protection | Stop malware switching off your antivirus | Defender | DETECT | high | L | H/P |
| 4 | defender.exclusions_risky | Folders/programs antivirus ignores (Ext Defender finding) | Defender | ASK per entry | high | L | H/P |
| 5 | smartscreen.apps | Warn before running unknown downloads (Ext SmartScreen info) | Reputation | AUTO | high | L | H/P |
| 6 | defender.cloud_protection | Cloud-assisted detection (incl. block at first seen, sample consent) | Defender | AUTO | med | L (privacy) | H/P |
| 7 | defender.pua | Block junk/bundled adware installers | Defender | AUTO | med | L | H/P |
| 8 | defender.script_nis | Script scanning + network attack inspection | Defender | AUTO | med | L | H/P |
| 9 | defender.asr.standard | Block driver abuse, LSASS theft, WMI persistence (Ext asr.*) | Defender | AUTO | high | L | H/P |
| 10 | lsa.run_as_ppl | Protect the sign-in process from password theft | Credentials | ASK (auto if gate clean) | high | M | H/P |
| 11 | boot.secure_boot_certs | Startup-security certificates expiring (Oct 2026) | Update | DETECT | high | L | H/P |
| 12 | net.public_sharing_exposure | Hide your PC on public Wi-Fi (Ext firewall.*) | Network | AUTO | high | L | H/P |
| 13 | printer.point_and_print | Printer-driver install without permission | Printing | AUTO | high | L | H/P |
| 14 | update.auto_policy_disabled | Automatic updates switched off (Ext update.freshness) | Update | ASK | high | L | H/P |
| 15 | driver.vulnerable_blocklist | Block known-dangerous drivers | OS | AUTO [verify] | high | L | H/P |
| 16 | net.llmnr | Stop fake name-lookup answers (LLMNR) | Network | AUTO | med | L | H/P |
| 17 | accounts.lockout_policy | Lock out password guessers | Accounts | AUTO | med | L | H/P |
| 18 | autorun.disabled | Stop USB/disc auto-run | OS | AUTO | med | L | H/P |
| 19 | ps.v2_engine | Remove old PowerShell 2.0 | OS | AUTO | med | L | H/P |
| 20 | net.wpad | Stop auto-detected proxy hijacking (Ext proxy.machine_default) | Network | AUTO [verify] | med | L | H/P |
| 21 | net.hosts_file | Tampered hosts file | Network | DETECT | med/high | L | H/P |
| 22 | wifi.risky_profiles | Auto-joining open/weak Wi-Fi | Network | ASK | med | L | H/P |
| 23 | defender.asr.web_script_email | Block scripts/email attachments launching programs | Defender | ASK (Warn mode) | high | M | H/P |
| 24 | office.internet_macros | Office: internet macros / Protected View | Office | AUTO (only if Office) | high | M | H/P |
| 25 | defender.asr.office | Stop Office launching programs | Defender | ASK | high | M | H/P |
| 26 | defender.asr.ransomware_usb | Extra ransomware shield, untrusted USB programs | Defender | ASK (Warn) | med | M | H/P |
| 27 | defender.network_protection | Block dangerous sites for every app | Defender | ASK (audit first) | med | M | **Pro only** |
| 28 | smartscreen.browser_policy | Browser safe-browsing turned off by a setting | Reputation | DETECT (+confirm delete) | med | L | H/P |
| 29 | smartscreen.store_apps | Web checks for Store apps | Reputation | AUTO | low | L | H/P |
| 30 | smart_app_control.state | Smart App Control status | Reputation | DETECT | low | L | H/P |
| 31 | update.paused | Updates paused/delayed | Update | ASK | med | L | H/P |
| 32 | update.reboot_overdue | Restart pending for days | Update | ASK | med | L | H/P |
| 33 | software.outdated_winget | Risky apps with newer versions (Ext software.end_of_support) | Update | ASK/DETECT | med | M | H/P |
| 34 | update.drivers_excluded | Driver updates blocked | Update | DETECT | low | L | H/P |
| 35 | update.store_autoupdate_policy | Store apps blocked from updating | Update | ASK | low | L | H/P |
| 36 | persistence.run_and_tasks | Risky auto-start programs/tasks | Persistence | DETECT | high if hit | L | H/P |
| 37 | persistence.wmi_subscriptions | Hidden WMI triggers | Persistence | DETECT | high | L | H/P |
| 38 | services.unquoted_paths | Unquoted service path | Persistence | DETECT | low | L | H/P |
| 39 | accounts.builtin_administrator | Hidden built-in Administrator enabled | Accounts | ASK | high | L | H/P |
| 40 | accounts.daily_admin | Everyday account is admin (Ext admin_membership) | Accounts | DETECT | med | L | H/P |
| 41 | session.lock_on_wake | Lock when you walk away / password on wake | Accounts | ASK | med | M | H/P |
| 42 | accounts.stale_enabled | Old unused accounts | Accounts | DETECT | low | L | H/P |
| 43 | accounts.hello_configured | PIN/Windows Hello set up | Accounts | DETECT [U] | low | L | H/P |
| 44 | accounts.find_my_device | Find my device | Accounts | DETECT [U] | low | L | H/P |
| 45 | remote_assistance.disabled | Block unsolicited Remote Assistance | Remote | AUTO | med | L | H/P |
| 46 | services.legacy_remote | Leftover Telnet/WinRM/SSH/FTP etc. | Remote | ASK | med | L-M | H/P |
| 47 | ntlm.lm_compat_level | NTLMv2 only | Credentials | ASK | med | M | H/P |
| 48 | ntlm.extras | No LM hash, no null fallback | Credentials | AUTO | low | L | H/P |
| 49 | lsa.restrict_anonymous | Block anonymous account/share listing (Ext restrict_anonymous_sam) | Credentials | AUTO | med | L | H/P |
| 50 | net.stack_hardening | Ignore ICMP redirects / source routing | Network | AUTO | low | L | H/P |
| 51 | net.netbios | Turn off NetBIOS name service | Network | AUTO with gate | med | M | H/P |
| 52 | net.mdns | Stop mDNS spoofing | Network | ASK | low | M | H/P |
| 53 | firewall.outbound_smb_internet | Block outbound SMB to internet | Network | ASK | high | M | H/P |
| 54 | firewall.user_dir_inbound_allow | Firewall allows in programs from Downloads/AppData | Network | DETECT | med | L | H/P |
| 55 | smb.shares_exposed | Folders shared with other computers (Ext smb.listener) | Network | DETECT | high | L | H/P |
| 56 | smb.server_encryption | Encrypt file-sharing traffic | Network | DETECT/tip | low | M | H/P |
| 57 | net.dns_encryption | DNS lookups unencrypted (Ext dns.configuration) | Network | DETECT/tip | low | M | H/P |
| 58 | net.nearby_sharing | Nearby sharing visible to everyone | Network | ASK [U] | low | L | H/P |
| 59 | tls.legacy_protocols | Old SSL/TLS in Windows apps | Network | ASK | low | M | H/P |
| 60 | system.exploit_mitigations | DEP/SEHOP/ASLR/CFG drift | OS | AUTO | med | L | H/P |
| 61 | wsh.disabled | Turn off Windows Script Host | OS | ASK | med | M | H/P |
| 62 | files.show_extensions | Show file extensions | OS | ASK | med | L | H/P |
| 63 | printer.spooler_remote | Spooler reachable from network | Printing | ASK | med | M | H/P |
| 64 | vbs.kernel_stack_protection | Kernel stack protection (Ext vbs.memory_integrity) | OS | DETECT [U] | low | M | H/P |
| 65 | defender.cloud_block_level | Stricter cloud blocking | Defender | ASK | low | L | H/P |
| 66 | privacy.recall | Recall screenshots (Copilot+ only) | Privacy | ASK | med | L | H/P |
| 67 | privacy.clipboard_sync | Clipboard history/cloud sync | Privacy | ASK | med | L | H/P |
| 68 | privacy.advertising_id | Advertising ID | Privacy | ASK | low | L | H/P |
| 69 | privacy.diagnostic_data_level | Optional diagnostic data | Privacy | ASK | low | L | H/P |
| 70 | privacy.tailored_experiences | Tailored tips/ads | Privacy | ASK | low | L | H/P |
| 71 | privacy.activity_history | Activity history | Privacy | ASK | low | L | H/P |
| 72 | privacy.delivery_optimization | Update sharing with other PCs | Privacy | ASK | low | L | H/P |

## 3. Recommended first batch (22 items)

Chosen for: high value to a non-technical user, high source confidence, safe or detect-only, low support burden.

| # | id | Why |
|---|---|---|
| 1 | os.feature_release_support | 24H2 Home/Pro support ends 2026-10-14 [V]; most time-critical item. Tip/ask only, never silent. |
| 2 | boot.secure_boot_certs | 2011 Secure Boot certs expire Oct 2026. Detect via events 1808/1801 + `UEFICA2023Status`; tip only, never write `AvailableUpdates`. |
| 3 | defender.tamper_protection | Already read as a gate in backend.ps1 but never shown. Free win, zero risk. |
| 4 | defender.threats_and_scan_age | Surfaces ignored detections; read-only. |
| 5 | defender.exclusions_risky | Attackers add exclusions first. Data already read (count only today). |
| 6 | smartscreen.apps | Replaces an info-only finding with a real state check; `Warn` is the default. |
| 7 | defender.cloud_protection | Prerequisite for ASR script/ransomware rules; MAPS parameter default is 0 [V]. |
| 8 | defender.pua | One cmdlet, trivially reversible [V]. |
| 9 | defender.script_nis | Same class as existing realtime/ioav/behavior controls. |
| 10 | defender.asr.standard | Microsoft "standard protection" rules, Block mode, Add- semantics. |
| 11 | lsa.run_as_ppl | Biggest credential-theft mitigation; value 2 only, gated on audit events + Secure Boot + no SAC [V]. |
| 12 | net.public_sharing_exposure | Fixes rule drift on Public profile; does not relabel networks without asking. |
| 13 | printer.point_and_print | Remove insecure policy values only; do not touch Spooler. |
| 14 | update.auto_policy_disabled | Catches "update blockers" and malware that stop patching. |
| 15 | net.llmnr | Policy value confirmed [V]; no casting impact (that is mDNS). |
| 16 | accounts.lockout_policy | `net accounts`; no banner, trivial undo. |
| 17 | autorun.disabled | Standard baseline; low risk. |
| 18 | ps.v2_engine | Missing feature treated as OK. |
| 19 | net.hosts_file | Read-only; catches hijacked update/bank hosts. |
| 20 | wifi.risky_profiles | Set to manual connect only; nothing deleted. |
| 21 | driver.vulnerable_blocklist | High value; ship only after VM confirms the value name. |
| 22 | net.wpad | Ship after VM confirms `DisableWpad` and the service stays running. |

Order inside the batch: cloud_protection before ASR; run_as_ppl before relying on the ASR LSASS rule (redundant then).

## 4. Per-item specs

Common fields omitted when identical: Restart "no", Edition H/P. "Undo" always restores the journaled prior value unless stated.

### 4.1 Defender

**defender.cloud_protection** (merges A8, B9, B10, B15)
- Detect: `Get-MpPreference` `MAPSReporting` (0 off, 1 basic, 2 advanced), `DisableBlockAtFirstSeen` (should be False), `SubmitSamplesConsent`. Policy `...\Policies\Microsoft\Windows Defender\Spynet\SpynetReporting` means managed: skip.
- Safe: MAPSReporting 2 (1 acceptable), BAFS False, consent 1 (send safe samples). Flag consent 2 (never send) as reduced protection.
- Fix: `Set-MpPreference -MAPSReporting Advanced -DisableBlockAtFirstSeen $false`. Consent change is ASK (privacy). Re-read after write.
- Undo: restore captured values. Restart: no.
- Breakage: privacy only; skip privacy-focused/air-gapped users. Src: Set-MpPreference page [V for MAPS enum]; Defender CSP AllowCloudProtection [V per B]; BAFS page [K].

**defender.pua** (A6, B11)
- Detect: `(Get-MpPreference).PUAProtection` 0/1/2. Safe 1.
- Fix: `Set-MpPreference -PUAProtection Enabled`. Undo: `Disabled` or `AuditMode`. Risk: quarantines keygens/cracks/bundled installers. Edge's own PUA setting is a tip only (policy would add banner). Src [V].

**defender.script_nis** (A10, B14, B16)
- Detect: `DisableScriptScanning` False; `DisableIntrusionPreventionSystem` False/null. (`NISEnabled` is already read; this covers the preference.)
- Fix: `Set-MpPreference -DisableScriptScanning $false -DisableIntrusionPreventionSystem $false`. Undo: restore. Risk negligible. Src: Set-MpPreference page [V param names]. Leave `DisableRemovableDriveScanning` info-only.

**defender.tamper_protection** (A11, B13)
- Detect: `(Get-MpComputerStatus).IsTamperProtected`; cross-check `HKLM\SOFTWARE\Microsoft\Windows Defender\Features\TamperProtection` (5 on / 4 off) [K]. Safe True.
- Fix: none by script (by design). Deep-link `windowsdefender://threatsettings`. Never write that registry key. Src [K]; the property is already used in backend.ps1:220.

**defender.exclusions_risky** (A12, B18; extends the count-only Defender finding)
- Detect (elevated; non-admin is masked): `ExclusionPath/Extension/Process/IpAddress`. Flag drive roots, `C:\Windows`, profile/Downloads/Desktop/Temp/Public, extensions exe/dll/ps1/bat/js/vbs/scr, processes powershell/cmd/wscript/mshta. Count the rest.
- Fix: per-entry ASK: `Remove-MpPreference -ExclusionPath ...` (and -Extension/-Process). Undo: `Add-MpPreference` from journal. Risk: games/dev tools slow or false-positive. Src: Defender exclusions docs [K].

**defender.threats_and_scan_age** (A13, B17, B19; extends defender.signatures, which covers signature age only)
- Detect: `Get-MpThreat` where `IsActive` or status not cleaned; `Get-MpThreatDetection` last 30 days; `QuickScanAge`/`FullScanAge` (4294967295 = never). Warn quick scan > 7 days.
- Fix: ASK "Run a quick scan" `Start-MpScan -ScanType QuickScan` (skip on battery). Never auto full scan or `Remove-MpThreat`. Undo: `Stop-MpScan`. Do not print user-folder paths in shareable reports. Src: Get-MpThreat/Get-MpComputerStatus pages [K].

**defender.asr.standard** (A2; extends asr.configured/effective which are read-only)
- Detect: `AttackSurfaceReductionRules_Ids/_Actions` (0 disabled, 1 block, 2 audit, 6 warn). Needs realtime on (existing asr.effective).
- Rules: `56a863a9-875e-4185-98a7-b882c64b5ce5` (vulnerable signed drivers), `9e6c4e1f-7d60-472f-ba1a-a39ef669e4b2` (LSASS credential theft; redundant when run_as_ppl on), `e6db77e5-3df2-4cf1-b95a-636979351e5b` (WMI persistence).
- Fix: `Add-MpPreference -AttackSurfaceReductionRules_Ids <g> -AttackSurfaceReductionRules_Actions Enabled`. Undo: `Remove-MpPreference -AttackSurfaceReductionRules_Ids <g>`. Risk: skip if Configuration Manager managed (WMI rule). Src: ASR rules reference (GUIDs per Report A fetch).

**defender.asr.web_script_email** (A3)
- Rules: `d3e037e1-3eb8-44c8-a917-57927947596d` (JS/VBS launching downloaded content), `5beb7efe-fd9a-4556-801d-275e5ffc04cc` (obfuscated scripts; needs cloud protection), `be9ba2d9-53ea-4cdc-84e5-9b1eeee46550` (executables from email).
- Safe: Warn (6). Fix/undo as above. ASK. Risk: installers using JS/VBS downloaders; skip heavy scripters. Src: ASR reference.

**defender.asr.ransomware_usb** (A4)
- Rules: `c1db55ab-c21a-4637-bb3f-a12568109d35` (advanced ransomware; needs cloud), `b2b3f03d-6a65-4f7b-a9c7-1c7ef74a9ba4` (untrusted USB). Optional `33ddedf1-c6e0-47cb-833e-de6133960387` (Safe Mode reboot).
- Safe: Warn. ASK. Risk: new indie software lacking reputation is blocked; skip gamers/portable-app users.

**defender.asr.office** (A5)
- Detect Office: `HKLM\SOFTWARE\Microsoft\Office\ClickToRun\Configuration` or winword.exe under Program Files.
- Rules: `d4f940ab-401b-4efc-aadc-ad5f3c50688a`, `3b576869-a4ec-4529-8536-b80a7769e899`, `75668c1f-73b5-4cf0-bb93-3ecf5cb7cc84` (Block only), `26190899-1602-49e8-8b27-eb1d0a1ce869`, optional `92e97fa1-2edf-4476-bdd6-9dd0b4dddc7b`.
- Safe: Block. ASK. Restart Office apps. Risk: macro-heavy workbooks and add-ins break; injection rule conflicts with some endpoint tools.

**defender.network_protection** (A7, B12) Pro/Enterprise only; on Home report "not supported".
- Detect: `EnableNetworkProtection` 0/1/2. Needs realtime, behavior, cloud on. Fix: ASK; offer `AuditMode` first. `Set-MpPreference -EnableNetworkProtection Enabled`. Undo `Disabled`. Risk: false blocks for dev/VPN/anti-cheat. Src [V].

**defender.cloud_block_level** (A9). Detect `CloudBlockLevel` (0 default, 2 high) and `CloudExtendedTimeout`. Safe High (2) + timeout 20; never 6. ASK. Risk: slight false positives, up to 20 s download pause. Enum names [K].

### 4.2 Reputation

**smartscreen.apps** (A16, B20; extends the info-only SmartScreen finding)
- Detect: effective `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SmartScreenEnabled` (REG_SZ Off/Warn/RequireAdmin) [K]; policy `HKLM\SOFTWARE\Policies\Microsoft\Windows\System\EnableSmartScreen` 0/1 [V]. Policy wins if present. Do not use `PreventOverrideForFilesInShell`.
- Safe: not Off; policy absent or 1. Fix: set `SmartScreenEnabled`=`Warn`; remove a locally-set `EnableSmartScreen=0` (if not MDM). Undo: restore. Risk: devs with unsigned tools may have disabled it on purpose. Restart no.

**smartscreen.store_apps** (A17, B21). `HKCU\Software\Microsoft\Windows\CurrentVersion\AppHost\EnableWebContentEvaluation` safe 1 [K]. AUTO for original user. Undo restore.

**smartscreen.browser_policy** (B22). Detect `HKLM\SOFTWARE\Policies\Microsoft\Edge\SmartScreenEnabled=0`, `SmartScreenPuaEnabled=0`; `...\Google\Chrome\SafeBrowsingProtectionLevel=0` [K]. Report; delete value only after confirmation. Browser restart.

**smart_app_control.state** (A18, B23). `HKLM\SYSTEM\CurrentControlSet\Control\CI\Policy\VerifiedAndReputablePolicyState` 0 off/1 on/2 evaluation [K]. Detect/tip only. NEVER write; off cannot be re-enabled without reinstall [V per B]. Note: SAC suppresses LSA audit events [V].

### 4.3 OS hardening

**lsa.run_as_ppl** (A1, B24) [V]
- Detect: `HKLM\SYSTEM\CurrentControlSet\Control\Lsa\RunAsPPL`; effective via System log WinInit event 12 ("protected process with level: 4"); pre-flight CodeIntegrity 3033/3063/3065/3066 in last 30 days.
- Safe: 2 (never 1; UEFI-locked, needs the LsaPplConfig.efi opt-out tool).
- Fix: `Set-ItemProperty ...\Lsa RunAsPPL 2 -Type DWord`. Do not write a Policies key. AUTO only if Secure Boot on, zero audit events, SAC off (audit unreliable under SAC) and no third-party LSA plug-in; otherwise ASK.
- Undo: set 0 or delete, reboot. Restart: yes. Risk: non-Microsoft-signed smartcard/fingerprint/password-filter plug-ins fail to load; third-party AV. Src: Learn LSA page [V].

**driver.vulnerable_blocklist** (A15). `HKLM\SYSTEM\CurrentControlSet\Control\CI\Config\VulnerableDriverBlocklistEnable` (absent = default on in Win11); flag 0. Fix set 1; undo restore; restart yes. Value name [K, verify on VM]. Risk: old RGB/GPU-tool/anti-cheat drivers; skip overclockers.

**system.exploit_mitigations** (A14). `Get-ProcessMitigation -System` DEP/SEHOP/BottomUp/HighEntropy/CFG; `bcdedit /enum {current}` nx. Fix `Set-ProcessMitigation -System -Enable DEP,SEHOP,BottomUp,HighEntropy,CFG`. Never ForceRelocateImages. Undo `-Disable` per captured state. Restart some. Fires only on drift.

**ps.v2_engine** (A35, B26). `Get-WindowsOptionalFeature -Online -FeatureName MicrosoftWindowsPowerShellV2Root` (+`V2`); missing = OK. Fix `Disable-WindowsOptionalFeature ... -NoRestart`. Undo Enable-. Slow DISM; needs healthy servicing. Risk: scripts calling `-version 2`.

**printer.point_and_print** (A41, B27)
- Key `HKLM\SOFTWARE\Policies\Microsoft\Windows NT\Printers\PointAndPrint`. `RestrictDriverInstallationToAdministrators` 0 is unsafe, default 1 [V]. `NoWarningNoElevationOnInstall`=1 and `UpdatePromptSettings`=2 unsafe [K, not on KB5005652].
- Fix: delete unsafe values (or set safe: 1/0/0). Undo restore. Never stop Spooler. Src KB5005652.

**printer.spooler_remote** (A42). `...\Policies\Microsoft\Windows NT\Printers\RegisterSpoolerRemoteRpcEndPoint` 2 = disabled [K]. Only if no shared printer (`Get-Printer | ? Shared`). ASK. Restart Spooler. Policy-key banner risk.

**autorun.disabled** (A39, B28). `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer` `NoDriveTypeAutoRun`=255, `NoAutorun`=1 [K]. Undo delete. Effective at sign-out.

**wsh.disabled** (A38, B30). `HKLM\SOFTWARE\Microsoft\Windows Script Host\Settings\Enabled`=0 (absent = on), also HKCU [K]. ASK. Breaks .vbs/.js logon scripts and some installers. Undo delete.

**files.show_extensions** (B29). `HKCU\...\Explorer\Advanced\HideFileExt` safe 0, then `SHChangeNotify`. ASK (visible UI change). Undo 1.

**vbs.kernel_stack_protection** (B25; extends vbs.memory_integrity). `...\DeviceGuard\Scenarios\KernelShadowStacks\Enabled` [U]. Detect/tip only; user flips the Windows Security toggle.

### 4.4 Credentials

**ntlm.lm_compat_level** (A30). `HKLM\SYSTEM\CurrentControlSet\Control\Lsa\LmCompatibilityLevel` (absent = 3); safe 5. ASK. Restart. Breaks old NAS/scan-to-folder. Win11 24H2 already dropped NTLMv1 client, so mainly older builds.

**ntlm.extras** (A31). `Lsa\NoLMHash`=1, `Lsa\MSV1_0\allownullsessionfallback`=0, `Lsa\UseMachineId`=1 [K]. AUTO. Restart. `UseMachineId` can upset some NAS auth: if user reports, revert.

**lsa.restrict_anonymous** (A32; extends restrict_anonymous_sam). `Lsa\RestrictAnonymous`=1, `EveryoneIncludesAnonymous`=0; `LanmanServer\Parameters\RestrictNullSessAccess`=1, empty `NullSessionPipes/Shares`. AUTO. Restart (server service). Old workgroup sharing risk.

### 4.5 Network

**net.llmnr** (A19, B47). `HKLM\SOFTWARE\Policies\Microsoft\Windows NT\DNSClient\EnableMulticast` [V]; absent/1 = on; safe 0. AUTO. Undo delete value. Restart DNS Client. Policy-key (banner possible). Risk: old devices found by short name.

**net.mdns** (A20, B47). `HKLM\SYSTEM\CurrentControlSet\Services\Dnscache\Parameters\EnableMDNS`=0 [K]. ASK only (Chromecast, AirPrint, Apple, smart home).

**net.netbios** (A21). `Win32_NetworkAdapterConfiguration.TcpipNetbiosOptions` safe 2. `Invoke-CimMethod ... SetTcpipNetbios`. AUTO only if no mapped drive/share uses a NetBIOS-only host and SMB1 is off; else DETECT. Undo restore per adapter.

**net.wpad** (A22). `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Internet Settings\WinHttp\DisableWpad`=1 [K]. Do not disable `WinHttpAutoProxySvc`. AUTO after VM verify. Undo delete. Restart service/reboot.

**net.stack_hardening** (A23). `Tcpip\Parameters` `EnableICMPRedirect`=0, `DisableIPSourceRouting`=2 (and Tcpip6); `NetBT\Parameters\NoNameReleaseOnDemand`=1 [K, MSS legacy baseline]. AUTO. Restart. Undo delete.

**net.public_sharing_exposure** (A24, B48; extends firewall.*). `Get-NetFirewallRule -PolicyStore ActiveStore` enabled inbound allow in groups "Network Discovery", "File and Printer Sharing" (and Remote Desktop/Assistance/WinRM) on Public profile. Fix `Set-NetFirewallRule -DisplayGroup <g> -Profile Domain,Private` (remove Public). Network category relabel is ASK only. Undo restore profile. Verify group display names on VM (localized: use rule Group IDs, not names).

**firewall.outbound_smb_internet** (A25). Block outbound TCP 445/139 to `Internet` keyword, named rule `Secblitz: block outbound SMB to Internet`. ASK. Undo `Remove-NetFirewallRule`. Breaks Azure Files/SMB-over-internet. Verify `Internet` keyword on VM.

**firewall.user_dir_inbound_allow** (A26). Enabled inbound Allow rules whose program is under Downloads/AppData/Temp/Users\Public. DETECT with per-rule Disable button. Breakage: game multiplayer.

**net.hosts_file** (A27, B46; extends nothing, dns.configuration covers resolvers only). Parse `%SystemRoot%\System32\drivers\etc\hosts`; flag non-loopback maps, security/Microsoft/bank domains, size > 1 MB; 0.0.0.0 blocklists are info. DETECT; optional backup + remove flagged lines (ASK). Src Microsoft hosts reset KB [K].

**wifi.risky_profiles** (A28, A29, B45). `netsh wlan show profiles`; prefer `netsh wlan export profile` XML (`<authentication>`, `<encryption>`, `<connectionMode>`) because output labels are localized; never `key=clear`. Flag Open/WEP/WPA-TKIP with auto-connect. Fix ASK: `netsh wlan set profileparameter name=X connectionmode=manual`; never delete. Weak current-network security is a tip (router-side fix). Skip when no WLAN service.

**smb.shares_exposed** (A33). `Get-SmbShare | ? {-not $_.Special}`; `Get-SmbShareAccess` Everyone/Anonymous/Guests with Change/Full. DETECT with per-share ASK removal. Undo `New-SmbShare`/`Grant-SmbShareAccess` from journal.

**smb.server_encryption** (A34). `Get-SmbServerConfiguration` `EncryptData`. Tip only (breaks non-SMB3 clients). 

**net.dns_encryption** (B51). `Get-DnsClientDohServerAddress` vs `Get-DnsClientServerAddress`. Tip only; never change user's DNS (parental filters, captive portals) [V per B].

**net.nearby_sharing** (B50) `HKCU\...\CDP\CdpSessionUserAuthzPolicy` (2 = everyone) [U]. ASK to set 1; VM-verify first.

**tls.legacy_protocols** (A46). SCHANNEL `Protocols\{SSL 3.0,TLS 1.0,TLS 1.1}\{Client,Server}` `Enabled`=0, `DisabledByDefault`=1. ASK. Restart. Browsers unaffected; legacy .NET/accounting/NAS may break.

### 4.6 Remote access

**remote_assistance.disabled** (A40, B49). `HKLM\SYSTEM\CurrentControlSet\Control\Remote Assistance\fAllowToGetHelp`=0 (non-policy; no banner). AUTO per A, ASK per B: use **ASK** until VM-verified. Tip that Quick Assist is separate. Undo 1.

**services.legacy_remote** (A47). Running/Automatic `RemoteRegistry`, `WinRM`(+listeners), `sshd`, `TlntSvr`, `FTPSVC`, `W3SVC`, `SNMP`; features `TelnetClient`, `TFTP`, `SimpleTCP`. ASK. Fix Stop-Service + Disabled; `Disable-PSRemoting`. Undo captured start type. Extends remote.rdp/remote.listener.

### 4.7 Accounts and session

**accounts.lockout_policy** (A44, B33). `net accounts` threshold/duration/window; "Never" = finding. Fix `net accounts /lockoutthreshold:10 /lockoutduration:10 /lockoutwindow:10`. Undo `/lockoutthreshold:0`. Local accounts only. AUTO.

**accounts.builtin_administrator** (B31). RID 500 enabled. ASK `Disable-LocalUser -SID`. Refuse if it is the only enabled admin.

**accounts.daily_admin** (B32; extends admin_membership). Tip only; never auto-demote.

**session.lock_on_wake** (A43, B35). `powercfg /q SCHEME_CURRENT SUB_NONE CONSOLELOCK` (GUID `0e796bdb-100d-47d6-a2d5-f7d2daa51f51`) = 1 for AC/DC; optional `InactivityTimeoutSecs`=900. Fix `powercfg /setacvalueindex ... CONSOLELOCK 1` + `/setdcvalueindex` + `/setactive`. ASK; warn first if an account has no password. Skip kiosk/media PCs. `InactivityTimeoutSecs` is a policy path (banner risk): prefer powercfg only.

**accounts.stale_enabled** (B34). `LastLogon` older than 180 days (null = unknown). Tip only.

**accounts.hello_configured** (B36) `dsregcmd /status` NgcSet [U]; tip only. **accounts.find_my_device** (B37) `LocationSyncEnabled` [U]; tip only, laptops with MSA.

### 4.8 Update hygiene

**os.feature_release_support** (B1; extends Windows lifecycle finding which only flags build < 22000) [V dates]
- Detect: `DisplayVersion`, `CurrentBuildNumber`, `EditionID`; embedded table: 24H2 2026-10-14, 25H2 2027-10-13, 26H1 2028-03-15, 26H2 2028-10-10, 23H2 ended 2025-11-12. Warn 60 days before end, high after. Enterprise/Education use different dates. Table must be updatable.
- Fix: open Windows Update / existing patching flow; ASK; never silent; skip on metered or < 20 GB free. Restart yes.

**boot.secure_boot_certs** (B2). System events 1808 (done), 1801 (staged), 1795/1796 (error), 1797/1798 (blocked); `HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\Servicing\UEFICA2023Status`; `Get-SecureBootUEFI -Name db` contains "Windows UEFI CA 2023" (throws on BIOS/SB off = N/A). Tip only: install updates, check OEM BIOS, back up BitLocker key. Never write firmware triggers.

**update.auto_policy_disabled** (B3; extends update.freshness, permissions.service.wuauserv). `...\Policies\Microsoft\Windows\WindowsUpdate\AU` `NoAutoUpdate`=1 or `AUOptions` 1-3; `DisableWindowsUpdateAccess`=1; `wuauserv/UsoSvc/BITS` StartType Disabled. ASK: delete values / reset start type (journal). Skip if managed.

**update.paused** (B4). `HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings` `PauseUpdatesExpiryTime`, `PauseFeatureUpdatesEndTime`, `PauseQualityUpdatesEndTime` [K]; flag future expiry or span > 35 days; policy `DeferQualityUpdatesPeriodInDays` > 14. ASK delete (= Resume).

**update.reboot_overdue** (B5; extends readiness-only reboot check). `RebootRequired`/`RebootPending` keys + uptime > 7 d warn, > 21 d high; ignore `PendingFileRenameOperations` alone. Offer restart/schedule; never auto-restart. Undo `shutdown /a`.

**update.drivers_excluded** (B6). `ExcludeWUDriversInQualityUpdate`=1; tip only. **update.store_autoupdate_policy** (B7). `Policies\Microsoft\WindowsStore\AutoDownload`=2; ASK delete.

**software.outdated_winget** (B8; extends software.end_of_support). `winget upgrade` as original user (not elevated); parse defensively, unknown on doubt; allowlist browsers/Java/Adobe/7-Zip/WinRAR/Zoom/VLC/Notepad++. ASK per app. No rollback. Currently gated in patching.rs.

### 4.9 Office, persistence, privacy

**office.internet_macros** (A48). Only if Office present. Per-user `Policies\Microsoft\Office\16.0\{word,excel,powerpoint}\security\blockcontentexecutionfrominternet`=1; flag `ProtectedView\Disable*InPV`=1 and reset to 0. Policy-path (HKCU) writes. Restart Office. Breaks macro files from email until unblocked. Src [K], verify.

**persistence.run_and_tasks** (B52). Run/RunOnce, Startup folders, non-Microsoft scheduled tasks. Flag only suspicious path/style AND unsigned. DETECT; optional ASK disable (not delete) with backup. **persistence.wmi_subscriptions** (B53). `Get-CimInstance root\subscription CommandLineEventConsumer/ActiveScriptEventConsumer`; ignore default SCM consumer; DETECT. **services.unquoted_paths** (B54). Unquoted path with spaces and user-writable prefix; DETECT only; never edit ImagePath.

**privacy.recall** (A50, B38) [V]. Copilot+ only. Feature `Recall` via `Get-WindowsOptionalFeature`; policy `...\Policies\Microsoft\Windows\WindowsAI\DisableAIDataAnalysis`=1 (no snapshots; deletes existing; Pro+) and `AllowRecallEnablement`=0 (removes; restart). Not configured = snapshots not saved (opt-in). ASK. Undo delete policy / `Enable-WindowsOptionalFeature`. Warn removal deletes snapshots.

**privacy.clipboard_sync** (B43): `Policies\Microsoft\Windows\System\AllowCrossDeviceClipboard`=0 (Pro+), Home: `HKCU\Software\Microsoft\Clipboard` values [K/U]. **privacy.advertising_id** (B39): `HKCU\...\AdvertisingInfo\Enabled`=0. **privacy.diagnostic_data_level** (B40): `Policies\Microsoft\Windows\DataCollection\AllowTelemetry`=1 only; never 0 on Home/Pro; never disable DiagTrack. **privacy.tailored_experiences** (B41): `HKCU\...\Privacy\TailoredExperiencesWithDiagnosticDataEnabled`=0. **privacy.activity_history** (B42): `Policies\Microsoft\Windows\System\EnableActivityFeed`/`PublishUserActivities`/`UploadUserActivities`=0. **privacy.delivery_optimization** (A49, B44): `Policies\Microsoft\Windows\DeliveryOptimization\DODownloadMode`=0/1 or prefer the Settings toggle (policy banner). All privacy items ASK, presented as "Privacy tidy-up", never red alerts. Undo: delete/restore.

## 5. Excluded items

| Item | Source | Reason |
|---|---|---|
| Autologon | A | Duplicate: `AutoLogonFinding` exists |
| accounts.no_password | A45 | Duplicate: backend.ps1:525 "Local accounts" already flags enabled accounts with `!PasswordRequired` |
| Guest account | B | Duplicate: accounts.guest |
| Defender realtime/behavior/IOAV/archive/signatures, ASR inventory, CFA, firewall profiles, SMB1/signing/guest, RDP/NLA, UAC, WDigest, AlwaysInstallElevated, BitLocker, TPM, Secure Boot on/off, HVCI | A, B | Duplicates of existing ids |
| ps.script_block_logging | A36 | Deferred: STIG vs CIS disagree, logs capture secrets, nobody reads it on a home PC |
| ps.execution_policy | A37 | Deferred: not a security boundary; breaks dev scripts |
| Credential Guard | A, B | Enterprise/Education licensing; vbs.* already covers VBS |
| Enhanced Phishing Protection | B | Protects work/school passwords only |
| Windows Hello-only sign-in | A | Breaks RDP/network/password sign-in |
| Disable Print Spooler outright | A, B | Breaks printing and Print to PDF; use point_and_print / spooler_remote |
| Disable all macros / VBAWarnings | A | High breakage; office.internet_macros instead |
| Outbound NTLM block, SMB BlockNTLM | A | Breaks NAS/IP shares |
| Disable SMB admin shares (`AutoShareWks=0`) | A | Breaks remote tools; info only inside smb.shares_exposed |
| Mandatory ASLR, ASR rules 01443614 / d1e49aac / c0033c00 | A | False positives and breakage |
| Application Guard | A | Deprecated |
| Password-never-expires flag | B | Contradicts current NIST/Microsoft guidance |
| Telemetry 0 / disabling DiagTrack | B | Unsupported on Home/Pro; breaks update reporting and Defender cloud |
| Click to Do, Settings agent, agent connectors | B | Insider/Enterprise only per CSP [V] |
| Location privacy alerts | B | Legitimate app use; not a hardening failure |
| Edge/PUA policy writes (SmartScreenPuaEnabled) | A | Managed-browser banner; tip only |
| Writing `AvailableUpdates` / Secure Boot firmware triggers | B | Unsafe on arbitrary OEM firmware |
| `HKLM\...\Features\TamperProtection` writes, SAC registry flips | A, B | Unsupported; can corrupt state |
| RunAsPPL = 1 | A, B | UEFI-locked, effectively irreversible |
| NetBIOS blanket disable (as AUTO without gate) | B | Folded into net.netbios with a gate |

## 6. Implementation notes

- Add each AUTO item as an engine control with read-before/after and journal; DETECT items as diagnostics rules + advice copy (non-technical wording, i18n entries in `i18n.rs`).
- VM verification list before any AUTO ships: `VulnerableDriverBlocklistEnable`, `SmartScreenEnabled`, `DisableWpad`, firewall group IDs, `Internet` keyword, `CloudBlockLevel`/`SubmitSamplesConsent` enums, `UEFICA2023Status`, `NoWarningNoElevationOnInstall`/`UpdatePromptSettings`.
- Reports A and B disagreed on mode for: lsa.run_as_ppl (A auto-if-gated, B ask), net.llmnr (A auto, B ask), remote_assistance (A auto, B ask), defender.cloud_protection (A auto, B ask-until-accepted). This document takes the more cautious option except llmnr (confirmed policy, no casting impact).
