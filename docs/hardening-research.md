# Windows 10/11 hardening research

Research/source refresh: **2026-10-02, Secblitz 0.2.0**. Audience: people maintaining personally owned Windows desktops, including gaming, development, assistive-technology, and mixed home/work use.

This is a researched, deliberately bounded decision framework, not an exhaustive baseline, certification, or promise of perfect protection. “Nondisruptive” is an engineering objective that requires testing against the user's actual workflow; even restoring a Windows default can interrupt an application.

## Research method and scope

Primary Microsoft, NIST, and Bitwarden documentation was retrieved with `webfetch`. The source register below distinguishes retrieved references from follow-up reading. The coordinating task also completed **five Tavily `search_depth: advanced` searches**, recorded in the research supplement. Search results were discovery aids; Microsoft Q&A suggestions to weaken authentication were not adopted as normative policy. No research key is stored in the project/application/website/build, and no device inventory or passwords were sent to Tavily. This documentation refresh required no secret access.

Implementation statements come from platform/backend, native permissions/state, engine/model, CLI/UI/tools/service and installer source. See v0.2.0 runtime evidence, expanded engine review, [permissions design](permissions-design.md), permission review, platform gate review and [privilege-escalation research](privilege-escalation.md). [FEATURES.md](FEATURES.md) describes user workflows; [security model](security-model.md) specifies boundaries. Earlier Windows results describe historical v0.1.0 artifacts, not the expanded release.

**Evidence snapshot:** v0.2.0 passed **70 library + 16 CLI = 86 native Windows tests**, and the coordinating run passed **84 Linux tests**. Real Windows 11 repair/undo covered four new registry settings, three firewall defaults and wuauserv DACL; BITS was intentionally skipped for a flagged baseline ACE. Independent cleanup restored all **18 baseline states**. Defender/tamper stayed on. The unsigned Inno 6.7.3 package passed Italian optional-monitor installation, actual scan (18 observations/19 findings) and running uninstall with real journal preservation. The final Italian custom-message repack also passed warning-free compilation and targeted install/uninstall with identical executable and preserved journals/states; scan/running-uninstall was not repeated for that two-string change. **Windows 10 is targeted but untested.** Research breadth is not implementation breadth or proof of general privilege-escalation prevention.

Enterprise documentation describes capabilities that may need a particular edition, build, license, management service, or Defender mode. A page mentioning Windows 10 does not establish that an unsupported Windows 10 release is still serviced. Server-only examples must not be copied into client automation without client capability checks.

## Decision categories

| Category | Meaning | Conditions |
| --- | --- | --- |
| **A - safe automatic candidate** | A narrow, reversible preference repair within a user-initiated hardening action | Known prior value, unmanaged device, supported API, no conflicting provider, successful journal preparation and readback. “Safe” is conditional, not zero-risk. |
| **U - explicit user approval** | A change with compatibility, access, data-transfer, financial, reboot, or recovery consequences | Explain exact changes and hazards; obtain an informed choice; test the relevant workflow; provide a recovery route. |
| **D - advisory/assessment only** | Report evidence or guide the owner/administrator; do not mutate | Managed or ambiguous ownership, missing capability, unreadable state, unsupported OS, firmware/account/recovery operation, or unvalidated implementation. |

An automatic **read-only** assessment can create local logs or reveal sensitive metadata. It must not silently upload reports. These categories are research recommendations; the engine has **no per-control approval, user profiles or selectable plan**. User-initiated `apply` iterates all **18** controls, changing only eligible states. U-category recommendations below are **not enforced by a per-control consent workflow**. The optional service only observes; automatic repair does not mean continual healing.

### Tailoring questions (future planning input)

Ask about workflows, not secrets:

1. Is this device personally owned, employer/school managed, or uncertain? Who administers it?
2. Which Windows edition, release/build, architecture, and servicing entitlement are present?
3. Is Defender the primary antivirus? Is another AV, endpoint agent, firewall, VPN, parental filter, or DNS policy installed?
4. Are RDP, remote support, file/printer sharing, NAS, media casting, game hosting, or local development servers needed?
5. Are games with anti-cheat, VR, controllers, audio interfaces, older printers/scanners, accessibility input devices, or assistive applications essential?
6. Are Office macros, unsigned development tools, WSL/VMs, containers, or custom bootloaders required?
7. Are there recent recoverable backups? Can the owner independently retrieve encryption/account recovery material? Ask for confirmation, never the material itself.
8. What restart window, connectivity constraints, battery/metered-data limits, and privacy preferences apply?

Unknown answers move relevant changes to D or U, never to a guessed safe default. An affirmative answer should narrow the plan, not disable unrelated protection.

## Current mutable controls: exact mapping

There are **18 controls**, not 18 security surfaces: the original four Defender/six firewall/two UAC controls, **four static registry repairs**, and **two fixed native service-DACL repairs**. `platform::backend` supplies 16 controls/14 findings; `permissions::with_permissions` adds two controls/five findings for CLI and monitor. Machine-scoped writes require repeated authority/capability checks. Minimum build is a capability check, not servicing entitlement.

**Gate G:** x64 Windows client, build at least 10240; readable domain status; native `IsDeviceRegisteredWithManagement` result establishing no MDM registration; no OMADM/cloud-join evidence; no relevant configured/ambiguous policy or local computer policy artifacts. PolicyManager/current/provider and resultant registry policy checks are scoped to the affected family: UAC `UserAccountControl_*`, Defender/ADMX Defender, or firewall including its MDM store. Enrollment-template counts, unrelated power/default catalogs and empty LocalGPO containers are not management proof. Only a typed exact missing RSOP namespace can be treated as absent after other gates; access denied, invalid class and provider errors still block. Enabled/filter-allowed nonlocal GPOs veto mutation. **Gate V:** G plus exactly the recognized Defender registration, no unfamiliar AV registration, normal active Defender service/AV state, readable preferences, and tamper protection explicitly off. **Gate F:** G plus no registered additional firewall provider, running BFE/MpsSvc, readable ActiveStore state, and no configured resultant firewall profile GPO. A successfully enumerated empty firewall RSOP store is no policy, not a failed lookup. See platform Wave 3 for authority metadata and residual limits.

| # | Compiled ID | Actual target | Research category and extra context | Verification | Rollback/hazard |
| --- | --- | --- | --- | --- | --- |
| 1 | `defender.realtime` | `DisableRealtimeMonitoring=false` | A with V; keep existing third-party protection | Preference readback plus `RealTimeProtectionEnabled` and Windows Security | Restore recorded boolean only; restoring disabled protection weakens security; quarantine is not undone |
| 2 | `defender.behavior` | `DisableBehaviorMonitoring=false` | A with V; test normal workloads | Preference plus `BehaviorMonitorEnabled` | Recorded boolean; possible detection/performance effects |
| 3 | `defender.ioav` | `DisableIOAVProtection=false` | A with V | Preference plus `IoavProtectionEnabled` | Recorded boolean; downloaded-file warnings/detections may persist |
| 4 | `defender.archive` | `DisableArchiveScanning=false` | A with V; large archives may cost scan time | Preference plus representative scan/workload | Recorded boolean; no undo of scan results |
| 5 | `firewall.domain.enabled` | PersistentStore `Enabled=true` | A only after connectivity assessment; otherwise U, with F | PersistentStore readback and ActiveStore | Recorded boolean; domain profile on an unmanaged machine may be inactive |
| 6 | `firewall.domain.inbound` | PersistentStore `DefaultInboundAction=Block` | U if dependencies unknown; F | Effective profile and authorized service tests | Restore `Allow`, `Block`, or `NotConfigured` as recorded; may cut access |
| 7 | `firewall.private.enabled` | PersistentStore `Enabled=true` | A only after connectivity assessment; otherwise U, with F | Both stores; printing/NAS/casting/hosting | Recorded boolean; rules remain but previously unfiltered traffic may stop |
| 8 | `firewall.private.inbound` | PersistentStore `DefaultInboundAction=Block` | U for sharing/hosting or unknown use; F | Both stores and required inbound paths | Recorded string; existing allow rules remain effective |
| 9 | `firewall.public.enabled` | PersistentStore `Enabled=true` | A only after connectivity assessment; otherwise U, with F | Both stores; remote-support continuity | Recorded boolean; remote recovery may need local console |
| 10 | `firewall.public.inbound` | PersistentStore `DefaultInboundAction=Block` | U for remote access/unknown use; F | Both stores and required services | Recorded string; not “block all” and not a rule audit |
| 11 | `uac.enabled` | Present DWORD `EnableLUA=1`, only from explicit zero | U; G; restart and legacy-app/accessibility review | Registry readback; runtime behavior after approved restart | Restore original zero; restart required; loss of UAC protection |
| 12 | `uac.consent` | Present DWORD `ConsentPromptBehaviorAdmin=5`, only from explicit zero | U; G; interactive elevation workflow | Registry and non-Windows elevation prompt | Restore original zero; unattended elevation workflows may break |
| 13 | `installer.always_install_elevated` | Machine AlwaysInstallElevated **1→0** | A with scoped G; U for a known dependent workflow | Exact DWORD readback; native apply/undo passed | Preserve HKCU/absence/other values; breaks unsafe MSI elevation conjunction; normal authorized install remains |
| 14 | `lsa.restrict_anonymous_sam` | RestrictAnonymousSAM **0→1** | A with scoped G; U for legacy anonymous enumeration | Exact DWORD readback; native apply/undo passed | Exact original; legacy enumeration/trust workflow may break; not broader RestrictAnonymous |
| 15 | `lsa.limit_blank_password_use` | LimitBlankPasswordUse **0→1** | A with scoped G; U if remote blank-local-password use is expected | Exact DWORD readback; native apply/undo passed | Console logon preserved; no password reading/changing; not every third-party authentication route |
| 16 | `wdigest.use_logon_credential` | UseLogonCredential **1→0** | U for deferred effect/legacy Digest SSO; scoped G | Native stored-state apply/undo passed; runtime credential removal unmeasured | Preserve absence; restart/sign-out may be needed; never read LSASS or force restart |
| 17 | `permissions.service.bits` | Exact versioned repair of dangerous broad-principal service grants | A only with fixed identity/owner/simple-ACL/service-policy gate; otherwise D/U investigation | Native exact descriptor readback; guest BITS **skipped**, not repaired | Exact original-derived target and original snapshot; no ACL normalization to force eligibility |
| 18 | `permissions.service.wuauserv` | Same versioned repair for Windows Update only | Same narrow gates; no arbitrary service/file repair | Native repair, exact undo and independent simulated standard-user rights allowed→denied passed | Safe rights/other ACE bytes/owner/group/flags preserved; benign descriptor drift conflicts |

Sources: [S1], [S2], [S3]. Defender `false` values are **disable flags set to false**, not protection switched off. Absent or nonzero UAC values are preserved; nonzero modes are not all equivalent, but the tool does not normalize them to 5. Secure desktop, built-in Administrator filtering, standard-user prompts, and every other UAC setting remain outside this mutation list.

Rows 13–16 use the exact HKLM paths and primary Microsoft sources in [privilege-escalation.md](privilege-escalation.md) and control-handoff.md. Only explicit unsafe REG_DWORD values are repaired; absent/safe values are preserved, unsupported types/ranges fail. Policy scope adds ADMX_MSI/ApplicationManagement, individual LSA option names and MSSecurityGuide/WDigestAuthentication. The machine Installer target is not itself proof of policy authority; other relevant policy/security-template/registry-preference artifacts still veto. No HKCU mutation.

Rows 17–18 use [permissions-design.md](permissions-design.md) and Microsoft's service security/API contracts. Only **BITS/wuauserv**, with trusted owner and recognized LocalSystem shared-process system-svchost identity, are mutable. Simple explicit ALLOW grants for Everyone/Authenticated Users/Builtin Users lose SERVICE_CHANGE_CONFIG, DELETE, WRITE_DAC and WRITE_OWNER; generic rights map to service rights when necessary. Safe rights, other ACE bytes/order/padding, owner/group and supported flags remain exact. NULL/absent DACLs, deny/inherited/flagged/complex ACEs are ineligible. Writes request DACL only, never SACL/owner/group or service reconfiguration/start/stop. Service policy uses SystemServices plus BITS/Update areas and vetoes actual resultant computer policy; it does not borrow the firewall gate. No arbitrary Windows file, third-party software or whole-system permissions repair is implemented.

All 18 use exact typed before-images and conflict-aware restoration. Defender runtime/firewall effective verification uses bounded polling; archive has no independent runtime flag, and restored NotConfigured is a stored state rather than recorded historical effective action. The four new registry values have strict readback; UAC/WDigest deferred effects need runtime verification. The engine independently checks post-write exact state before recording completion for every control.

Service state uses **schema-1** journal strings `dacl-v1:<lowercase hex>` containing owner/group/flags and exact DACL bytes; the `service-dacl-repair-v1` catalog sentinel is never written. Recovery derives expected state from the durable original, never fresh current state. Safe-to-safe descriptor drift still conflicts. Bounds are **128 KiB per line, 1 MiB per WAL**, with 16 KiB decoded-descriptor/native 8 KiB query bounds. Descriptor snapshots are sensitive local protected metadata, not passwords; retain them in the restricted journal and redact shared evidence. These checks are not atomic against external writers or a promise of broad compatibility/prevention.

Both outcomes and findings feed review status: attention/**review**/skipped/unknown/error/conflict/pending and unrecognized statuses yield exit 2. Incomplete recovery is `pending`; a sealed transaction retained for optional undo is `info`. Findings failure becomes `unknown` without discarding operation outcomes. Operational failures exit 1.

## Current advisory findings versus researched future work

There are **19 advisory findings**: original 13 plus Automatic logon in `backend.ps1` (14 platform findings), plus five native service audits. Engine recovery/assessment-failure messages are separate. Findings do not add mutation authority; a risky service audit is not proof that its repair gate passes.

| Existing finding | What is actually observed | Important limit | Research extension, not implemented |
| --- | --- | --- | --- |
| Security providers | SecurityCenter2 registered AV/firewall names | Registration is not health; inactive unfamiliar registrations still block relevant writes | Provider health and stale-registration troubleshooting [S1] |
| Windows Firewall | Three ActiveStore profile states, inbound/outbound defaults | No rule, port exposure, router, or application reachability audit | Contextual sharing/hosting plan and reachability verification [S2] |
| Defender | Mode, service/runtime flags, signature timestamp/version, exclusion counts | Seven-day signature heuristic is not patch compliance; hidden exclusions possible | Cloud/PUA/network protection, ASR/CFA pilots, update freshness [S1, S4, S5] |
| Windows lifecycle | OS caption/version/build; pre-22000 attention | ESU entitlement and per-edition release lifecycle unverified | Release/edition/ESU evidence and migration planning [S6] |
| Device encryption | Volume protection and encryption state | Does not verify recovery backup or possession | Owner-guided encryption/recovery assessment [S7] |
| Secure Boot | `Confirm-SecureBootUEFI` | Unsupported/inaccessible is unknown; no key/certificate inventory | Firmware/boot media compatibility review [S8] |
| Windows updates | Offline cached pending nonhidden update count | Zero is not up to date; no online scan | Approved online scan, active hours, reboot tracking [S6, S9] |
| Remote Desktop | `fDenyTSConnections` | No listener, NLA, account, firewall, or Internet exposure verification | Need-based RDP/VPN/gateway review [S10] |
| SMB1 | Optional feature state | Not SMB dialect/signing/share/guest audit | NAS compatibility, signing/encryption/guest review [S11] |
| SmartScreen | Static guidance | No effective-state probe at all | Browser/OS-specific evidence and approved settings [S12] |
| Memory integrity | DeviceGuard configured/running service value 2 | No driver compatibility or performance test | HVCI pilot, OEM updates and recovery [S16] |
| Management and mutation eligibility | Device-wide checks plus UAC-scoped authority, using UAC ID | Not an all-family eligibility result or authoritative management inventory; every control repeats its own gate | Administrator handoff and further capability validation [S17] |
| Automatic logon / AutoSignIn | AutoAdminLogon flag and DefaultPassword **value-name presence** | No password/identity data or LSA-secret reads; absence does not prove no autologon secret | Owner-guided kiosk/sign-in review; no automatic disablement |
| Service permissions: BITS | Broad-principal dangerous ALLOW candidates/unrestricted DACL | Not effective-access analysis; flagged guest baseline is not repairable | Fixed repair implemented only when eligible; other cases owner-guided |
| Service permissions: wuauserv | Same bounded service-object scan | Separate identity/authority/ACL gate determines repair eligibility | Fixed repair implemented; broader dependencies require review |
| Service permissions: WinDefend | Same scan | **Advisory only**, no repair | Authorized service-owner investigation |
| Service permissions: Schedule | Same scan | **Advisory only**, no repair | Authorized service-owner investigation |
| Service permissions: SecblitzMonitor | Same scan; absent optional service is info | **Advisory only**, LocalService not assumed SYSTEM | No automatic healing/ACL repair by the monitor |

### Native evidence and privilege-escalation limits

v0.2.0 results shows all four new registry fixtures applied/restored and wuauserv's Everyone mask changed **0x00060002→0x00020000**: dangerous CHANGE_CONFIG/WRITE_DAC removed, benign READ_CONTROL preserved. Existing ACE bytes, owner/group/flags remained exact; tool undo restored the unsafe staged descriptor. Independent **AuthzAccessCheck simulation** used a synthetic standard-user SID and explicit broad groups: the two tested rights went allowed→denied after repair and back on undo. No real standard-user account/logon, token impersonation, exploit, payload or service reconfiguration was performed.

BITS retained a baseline flagged ACE (0x02 ContainerInherit). It correctly stayed ineligible, leaving the unsafe fixture untouched until manual cleanup. There is **no native BITS repair success**. Benign wuauserv QUERY_STATUS ACE drift caused conflict, then explicit retry after the harness restored expected applied state performed exact undo. Eight controls changed in total; all 18 actual baseline states were restored after tool undo and independent fixture cleanup. Defender/tamper stayed enabled; WDigest was not tested across reboot.


## Researched surface catalog

OS/account/security changes outside the exact **18-control** table are **future/researched only**. The separate 24-character generator and approved Bitwarden desktop installation do not configure credentials or add hardening controls. Current advisory coverage is identified above. The A/U/D labels are this project's conservative synthesis, not a verbatim Microsoft baseline.

### 1. Defender, reputation, and exclusions

- **A:** repair the four core preferences only after V succeeds. Preserve exclusions, scheduling, provider registrations, and all other settings. Existing protections should remain enabled.
- **U:** user-triggered signature refresh or scans (network, CPU, battery and quarantine consequences); enabling cloud-delivered protection/sample submission (explain data transfer); PUA protection and network protection after workload testing and capability checks.
- **D:** competing or passive antivirus, unknown registration, tamper protection, enterprise endpoint policy, hidden exclusions, failed APIs. Do not disable tamper protection, uninstall competing AV, stop security services, or delete exclusions to make an operation succeed.
- **Verify:** Windows Security provider health, `Get-MpComputerStatus`, narrowly selected `Get-MpPreference` fields, signature timestamps and events. A running process alone is insufficient. Offline cloud checks cannot establish cloud protection functionality.
- **Rollback:** exact preferences if still owned and unchanged; detections, quarantines and data already submitted to cloud services cannot be reversed by preference rollback. Exclusion removal needs separate review of the real application requirement and a narrowly scoped alternative. [S1, S4, S5, S12]

### 2. Firewall and network exposure

- Preserve profile-specific rules, outbound defaults, IPsec, service configuration and VPN adapters. Enable profiles and default-block unsolicited inbound traffic only after checking required connectivity. Never substitute a blanket firewall reset for targeted repair.
- Remote administration, game hosting, discovery, SMB printers, development servers and casting make default changes **U**. Do not automatically create broad inbound exceptions to mask failures. Use narrow app/service/profile/address rules with an owner and review date when the user approves.
- Verify the active network category and resultant profiles, then test named workflows from an authorized peer. Default inbound block still allows matching allow rules and solicited traffic; it does not prove that RDP/SMB is unreachable.
- Keep a local console recovery route before remote networking changes. Restore only the recorded changed preference; rolling back the entire firewall export can erase later legitimate changes. No automatic default-deny outbound, IPv6 disablement, proxy replacement, Winsock reset, or firewall service stop. [S2, S10, S11]

### 3. ASR: audit before enforcement

ASR is not currently configured or assessed. Treat an initial audit pilot as **U**, because it changes configuration and generates potentially sensitive application/path telemetry; it is not protection equivalent to blocking.

1. Check the exact rule's supported builds, Defender mode, cloud dependencies, configuration method and reporting license. Do not assume all rules work on every edition.
2. Read the existing per-rule ID/action mapping and exclusions. Preserve existing Block/Warn rules; never downgrade them to Audit to standardize a pilot.
3. Start a small agreed selection in **Audit**. Candidate behavior groups include Office child processes/executable content, script abuse, downloaded executable launching, credential theft, and vulnerable driver abuse. Recheck the rule reference before selecting exact GUIDs.
4. Exercise actual Office macros, developer tools, management scripts, installers, games/anti-cheat, accessibility apps and support tools through a representative work cycle, not merely a quiet idle period.
5. Review local Defender operational events or licensed reporting. Microsoft documents ASR audit/block events such as 1122/1121; validate the provider and current rule documentation. Zero events can mean no exercised activity or unavailable telemetry, not compatibility.
6. Promote individually to Warn/Block only after explicit approval; rule support for Warn varies. Prefer fixing the app or precise exceptions to broad exclusions. Microsoft permits immediate Block/Warn for certain standard rules; this project deliberately uses a more conservative pilot for heterogeneous personal PCs.

Rollback needs the exact previous state of each touched rule, including absence, plus preservation of untouched mapping entries. Whole-array replacement risks deleting other rules. Stopping an audit pilot does not remove historical events or repair an already blocked workflow. [S4]

### 4. Encryption and recovery

**D assessment, U owner-guided enablement.** Device encryption and full BitLocker management have different availability. Home may support Device encryption even when BitLocker management cmdlets are unavailable. Windows 11 24H2 broadened device-encryption prerequisites; do not use an old Modern Standby requirement as a universal gate.

Before enablement: identify OS/data/removable volumes, edition, TPM readiness, firmware mode, disk layout, existing encryption, power availability, and dual-boot/boot-media needs. Have the owner confirm an independently accessible recovery backup using Microsoft's UI and their chosen recovery destination. Never request, print, export, upload, journal, or screenshot the recovery password.

Verify **Protection On** as well as encryption completion; encrypted data with a clear key or suspended protection is not equivalent to protected encryption. Check each intended volume. Encryption protects data at rest, not files that ransomware can access in an unlocked session. External drives are not automatically covered by Device encryption.

Rollback is not a registry undo: decryption takes time, consumes power, and removes theft protection. Suspending protectors is not a routine rollback. Firmware/TPM changes can prompt recovery or make data inaccessible. Do not clear the TPM, rotate/delete protectors, repartition, change cipher by decrypt/re-encrypt, or auto-enable preboot PINs in a safe baseline. [S7, S8]

### 5. Boot, firmware, HVCI and exploit protection

- **D:** Secure Boot, TPM, firmware version, boot mode and HVCI assessment. Unsupported firmware/query errors must remain unknown, not “disabled.”
- **U:** OEM firmware updates, Secure Boot enablement, memory integrity, credential-isolation features, or per-app exploit mitigations after hardware/driver/edition checks and recovery preparation. Existing stronger settings are preserved.
- Verify Secure Boot after restart, HVCI **running** rather than merely configured, required peripherals, sleep/resume, virtual machines and application performance. Drivers and anti-cheat can fail; older CPUs can have greater virtualization overhead.
- Rollback may require local firmware UI or Windows Recovery Environment and encryption recovery access. UEFI-locked and mandatory VBS settings have harder recovery semantics and are unsuitable for automatic personal-device repair.
- Boot certificate/dbx updates and revocations require current Microsoft/OEM guidance and compatible recovery media; old boot media may stop working. Do not reset Secure Boot keys, alter custom trust/dual boot, enable test signing, disable driver signing, or apply generic exploit-mitigation packs. [S8, S16]

### 6. Browsers, phishing, extensions and document apps

**U changes; D when managed.** Keep the user's chosen browser updated. Review SmartScreen or the browser's equivalent safe-browsing protection, download warnings, site camera/microphone/location/notification permissions, and extensions with broad page access. Preserve intentional browser profiles and enterprise policies. Never assume one Windows registry flag covers Edge, other browsers and all users.

Explain that cloud reputation checks send relevant URL/file information; stricter blocking, cookie restrictions, HTTPS-only settings and tracking prevention may break sign-in, embedded content or internal applications. Verify in the browser UI and policy page, test routine sites and accessibility extensions, and use only an official benign demonstration when needed. Do not open real malware to prove protection.

Keep Office/PDF readers patched, Protected View and attachment-origin checks intact. Internet macro blocking and app-control policies need a documented exception workflow for legitimate macros and signed internal tools. No global removal of Mark of the Web or automatic “unblock all downloads.” App Control/Smart App Control availability, evaluation state, and recovery behavior vary by Windows release; treat them as a separate researched pilot, not a reversible preference toggle.

Rollback individual permission/setting changes; removing an extension or clearing browser data can lose information and is not fully undone by reinstalling it. Do not read cookies, browsing history, saved passwords or vault contents for an audit. [S12, S4, S17]

### 7. Windows, apps, Store, drivers and WinGet updates

**A assessment; U installs and restarts.** Keep normal security servicing enabled, choose active hours, retain update notifications and plan restarts. Do not disable Windows Update, Store, BITS or security intelligence services. Inspect edition/release support and online update results; an offline cached count of zero is not compliance.

WinGet is one inventory/update source, not a universal patch manager. Check that App Installer/WinGet exists in the intended interactive user's context. Respect package identity, source, publisher, pins, per-user/per-machine scope, unknown versions and agreements. Do not assume SYSTEM has the same WinGet registration as the desktop user.

Useful manual review commands, run by the user (they may contact configured sources):

```powershell
winget --version
winget source list
winget list
winget upgrade
```

Only after choosing and reviewing a package, substitute its real ID and source:

```powershell
winget show --id <PACKAGE_ID> --exact --source <SOURCE_NAME>
winget upgrade --id <PACKAGE_ID> --exact --source <SOURCE_NAME> --interactive
```

Angle-bracket tokens are placeholders, not literal executable values. Do not default to `upgrade --all`, include unknown/pinned apps, accept agreements, bypass hashes, override installer arguments, or allow reboots. Installers can close applications, migrate data, add services, or reboot independently of the tool's intent. Use vendor/Store update mechanisms for unmatched software.

Verify installed version and functional launch after update and, when required, restart. App/OS rollback may be time-limited, unavailable, or unsafe after data migration; it is not covered by this tool's preference journal. Driver/firmware updates need OEM compatibility and recovery planning. [S9, S6, S16]

### 8. Passwords, passkeys, MFA and local privilege

**D guidance; U actions performed by the owner in trusted account interfaces.** Recommend unique long passwords stored in a reputable password manager, passkeys where supported, and MFA with recoverable backup methods. Bitwarden is a documented example, not a bundled dependency or requirement.

NIST SP 800-63B-4 recommends/mandates controls for network authentication verifiers in its scope: minimum 15 characters for single-factor passwords, at least eight when used only within MFA, support for long passwords, blocklists, password-manager/paste support, no arbitrary composition rules, and no periodic change without compromise evidence. These are not a license to rewrite Windows local password policy or to impose a 15-character rule on a device-bound Hello activation PIN.

Passkeys bind authentication to the relying party and resist phishing; synced and device-bound credentials have different recovery and assurance properties. Windows 11's native passkey-management experience begins with 22H2 plus the applicable update; older Windows/browser/authenticator combinations differ. Hello PIN/biometric unlock is not evidence of a weak or missing account password.

The application must not read SAM/LSASS, Credential Manager, browser credential databases, vaults, password hashes, tokens, recovery codes, private keys, or Wi-Fi keys. It must not attempt password validation, password resets, account disabling or group-membership changes. The current `PasswordRequired` flag assessment cannot determine actual strength or whether a password exists.

Password generation is a **separate explicit `password` action** in `src/ui.rs`: 24 characters from a 64-symbol alphabet using OS randomness, interactive-terminal output only, no account change, clipboard copy, journal or report inclusion. It is not one of the 18 hardening controls. Tests use synthetic input rather than generating/exposing a secret. Bitwarden's generator history has a different lifecycle; terminal output can persist in scrollback/transcripts.

`tools bitwarden --yes` is implemented in `src/tools.rs` and requires the original desktop user's **non-elevated session**. Consent covers download/install and package/source agreements. It resolves registered App Installer, validates the Microsoft WinGet repository metadata, selects exact `Bitwarden.Bitwarden` in user scope, preserves installer-hash checks, avoids upgrading existing installations, bounds execution and detects installation afterward. Actual network installation remains untested in the offline guest. This does not create a vault/account, inspect/import credentials, configure browser extensions/autofill or enroll MFA/passkeys. Preference rollback does not uninstall the app. See [FEATURES.md](FEATURES.md).

Verify the owner can sign in with the new method and retrieve independent recovery options before removing an old method. Retain a separate administrator recovery path before choosing standard-user daily use. Password/passkey changes are not reversible from this journal, and deleting a passkey or resetting a password can lock out access. [S13–S15, S3]

### 9. Backups, ransomware and recovery readiness

**D assessment; U backup setup and CFA.** Use multiple copies with a versioned/offline or otherwise separately protected copy, and periodically restore a sample to a separate location. Select an acceptable recovery point/time with the user. A connected writable backup can be encrypted alongside primary data; synchronization can propagate deletion/encryption. Cloud version retention, account recovery and subscription coverage need confirmation.

Verify actual restore readability, recent successful backup jobs, coverage of important folders and app data, and recovery access from a second device. Keep recovery secrets outside reports. System Restore and this preference journal are not personal-file backups or bare-metal recovery.

CFA requires active Defender and real-time protection; start with an approved audit pilot, exercise save/export/game-save/developer/backup workflows, then approve blocking with precise trusted-app exceptions. Audit does not stop ransomware. Protected-folder configuration alone does not prove every required folder is protected.

Rollback CFA only to the recorded state and preserve existing allowed apps/folders; do not blanket-allow scripting engines. Turning it off cannot recover files already encrypted or writes that failed. A backup restore can overwrite newer work; test separately and let the owner choose restore scope. For suspected infection, use a trusted incident-response/recovery process rather than treating hardening as malware removal. [S5, S18]

### 10. SMB, RDP, remote management and home networks

- **SMB (U):** remove SMB1 after identifying old NAS/scanner dependencies; prefer supported device firmware/protocols instead of restoring SMB1. Assess client and server separately. Signing protects integrity; encryption protects confidentiality; they are not interchangeable. Defaults and command support differ across Windows 10 and Windows 11 editions/builds, especially 24H2. Do not enable guest fallback or disable signing as a generic NAS fix.
- **RDP (U):** disable the host only if the user does not need it and has another access route. Windows Home is not an inbox RDP host. For required RDP, retain NLA, restrict allowed users and firewall scope, and prefer an approved VPN/gateway to direct Internet exposure. NLA is not MFA. No automatic router port-forward changes.
- **Other listeners (D/U):** review WinRM, Remote Assistance, third-party support agents, OpenSSH, network discovery, file/printer shares and developer listeners by owner and purpose. Do not stop services solely because they listen. Keep printing and remote assistance working when explicitly needed.
- **Verify:** optional-feature state after any restart, negotiated SMB dialect/signing/encryption against the real NAS, required print/scan paths, authorized remote login and firewall scope. The current single RDP registry probe cannot establish these properties.
- **Rollback:** exact individual settings, share/rule ownership and approved feature state; reinstalling SMB1 or removing NLA weakens security and requires separate risk review. Never enforce blanket NTLM/LLMNR/NetBIOS disablement without name-resolution and legacy-authentication testing. [S10, S11, S2]

### 11. DNS, Wi-Fi, VPN and routers

**D assessment; U changes.** Prefer WPA3-Personal when adapter, driver and router support it, or WPA2-AES for compatible legacy equipment; move away from open/WEP/TKIP networks. Confirm the actual connected security type, not just adapter capability. Router updates and guest/IoT separation are owner-guided external actions; account credentials and Wi-Fi keys are never requested by the tool.

Use Public network classification on untrusted networks unless a specific workflow requires otherwise. Review automatic connection to public hotspots and obsolete profiles. Do not silently delete saved networks, disable Bluetooth, change MAC randomization or remove VPN software: this can break provisioning, network allowlists and assistive devices.

Encrypted DNS protects the path to the resolver, not against the resolver itself, all traffic observation, or malicious content. Browser secure DNS and OS secure DNS are separate. Gate OS DoH on actual client build/cmdlet/UI support; Windows 10 should not be assumed to expose the same native controls as Windows 11. The retrieved DoH reference [S19] is **Server-specific** and supports the protocol/fallback discussion, not a Windows 10/11 compatibility claim.

Before DNS changes, record per-interface automatic/manual state, IPv4/IPv6 servers, search suffixes, VPN split-DNS/NRPT dependencies, parental filtering and captive portal behavior. A public resolver can break internal names or bypass intentional filtering; encrypted-only settings can cause total lookup failure. Explain fallback versus fail-closed behavior and resolver privacy policy.

Verify public and required private names, VPN connect/disconnect, captive portals and both address families; verify transport separately because a successful lookup does not prove DoH. Restore the exact original per-interface configuration, not a hardcoded public resolver. Never export WLAN profiles with clear keys. [S2, S19, S17]

### 12. Privacy, accessibility, gaming and peripherals

Privacy is user choice as well as security. **U:** per-app camera, microphone, location, advertising/personalization and optional diagnostics review; separately review browser and application telemetry. Edition-specific minimum diagnostic levels matter. Turning off optional diagnostics does not mean no network traffic or no required service data. Avoid hosts-file/DNS blocklists that break account login, security reputation and updates. [S20]

Accessibility must remain a first-class requirement. Preserve Narrator, screen readers, Magnifier, speech/voice access, Sticky/Filter Keys, UIAccess behavior and input devices. Secure-desktop prompts, microphone denial, short lock timeouts and strict app control can block essential access. Ask which workflows must be tested; do not infer disability or collect health information. Verify keyboard-only navigation, readable status text, screen-reader announcements, zoom/high contrast and usable elevation/rollback. UIAccess changes are **U/D**, not an automatic “attack surface reduction” tweak. [S3]

For gaming/development/media work, test launchers, anti-cheat, overlays, controllers, VR, voice chat, save directories, mods, WSL/VMs and audio/video capture. Do not use “gaming mode” as permission to disable Defender, firewall, HVCI, driver signatures, UAC or Windows servicing. HVCI/ASR/CFA changes require measured before/after application and latency checks. No automatic disabling of Xbox, Bluetooth, Print Spooler or virtualization services. [S16, S4, S5]

For peripherals, prefer supported OEM-signed drivers, firmware and least-privilege vendor utilities. USB/storage/device-install restrictions require explicit approval and a recovery input method; they can disable security keys, keyboards, backup drives, scanners or accessibility devices. Bluetooth may be needed for cross-device passkeys. Restore only the changed permission/policy; re-enabling a service may not restore a removed pairing, driver or app data. [S15–S17]

### 13. Managed and ambiguous devices

Domain membership, Entra join/enrollment, MDM, local/domain GPO, security-provider ownership and effective policy matter more than possession of a local administrator token. A BYOD device can have managed work data without full device management; local probes are imperfect.

Current implementation combines device-wide management vetoes with **scoped relevant policy authority**. Native MDM registration replaces the enrollment-child-count heuristic; unrelated provider power knobs and empty LocalGPO containers are not automatic vetoes. Relevant staged/orphaned provider settings, ambiguous authority, access failures and local policy artifacts remain conservative blockers. Current metadata is accepted as inactive only with explicit DWORD `_ProviderSet=0` and absent/empty-string winning provider; default-valued policy alone is not evidence of absence. No provider GUID is universally exempted, and these registry internals are not claimed as an exhaustive documented policy contract. **D:** unresolved ownership goes to the administrator. Do not remove enrollment or weaken policy to pass a gate. review-platform.md

Current checks use scoped source/result evidence; further capability work should refine distinctions among managed, unsupported, unknown and already configured states without replacing uncertainty with eligibility. Microsoft Policy CSP distinguishes requested/resultant policy and user/device scope. Administrator-controlled rollout belongs in the management system. v0.2.0 native mutation/undo is now recorded for the four registry controls and wuauserv, with separate fail-closed BITS evidence; this does not cover every management combination. [S17]

## Lifecycle: Windows 10 is already past general support

Standard Windows 10 support ended **October 14, 2025**. As of this research date, ordinary un-enrolled Windows 10 installations should not be described as supported merely because the tool can query them or Defender definitions still update.

- Commercial/education ESU provides eligible enrolled Windows 10 22H2 devices critical/important security updates for up to three years, subject to the program's prerequisites and annual entitlement. It is not feature development or general support.
- Consumer ESU has different enrollment terms. The live Microsoft consumer page retrieved in this pass states coverage through **October 12, 2027**, with automatic continuation for enrolled devices. This differs from earlier guidance describing an October 2026 end date: treat the live terms and the device's enrollment status as time-sensitive evidence, not a hardcoded lifecycle rule. Verify regional conditions and current terms before advice; do not apply commercial pricing/terms to home users. [S21]
- LTSC/LTSB and IoT releases have distinct lifecycles. Establish the exact SKU and release; “Windows 10” is not enough to infer an end date.
- Windows 11 also has release- and edition-specific support deadlines. Recommend a supported upgrade/migration with hardware, software and accessibility checks, not bypassing Windows 11 hardware requirements.
- Current source reports the distinction but does **not** validate ESU enrollment, support entitlement, or the current supported Windows 11 release. [S6; follow-up links below]

## Verification and acceptance for future controls

Each candidate needs a record containing: purpose/threat, source and review date, applicable edition/build, owner/scope, before value including absence, target, A/U/D category, exact gate, user explanation, reboot/network/data-transfer implications, verification method, recovery steps, and evidence status.

Use separate outcomes for **preference written**, **effective state observed**, **workflow tested**, and **recovery tested**. Test unavailable/denied APIs, management policy, competing providers, current stronger settings, fresh drift, interrupted writes and unavailable recovery paths. A score or green badge must never convert unknown evidence to a pass.

Representative acceptance workloads should include home browsing/printing, laptop sleep/resume, VPN/remote support, NAS backup/restore, Office macros, game launch/online play/save, development builds/VMs, assistive input/screen reader, and installed peripheral use. Conduct only applicable tests with the owner's participation. No automatic real-malware execution or broad external scanning.

## Primary source register

The following URLs were retrieved for this document on 2026-10-02. Recommendations above synthesize these sources; not every project policy is a verbatim vendor requirement. Revalidate documentation and local capability before adding executable controls.

- **[S1] Microsoft - Defender compatibility and modes:** https://learn.microsoft.com/en-us/defender-endpoint/microsoft-defender-antivirus-compatibility
- **[S2] Microsoft - Windows Firewall overview:** https://learn.microsoft.com/en-us/windows/security/operating-system-security/network-security/windows-firewall/
- **[S3] Microsoft - UAC settings and configuration:** https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/settings-and-configuration
- **[S4] Microsoft - ASR deployment guide:** https://learn.microsoft.com/en-us/defender-endpoint/attack-surface-reduction-rules-deployment (links to rule reference, test and monitoring guides).
- **[S5] Microsoft - Controlled folder access:** https://learn.microsoft.com/en-us/defender-endpoint/controlled-folder-access-overview (retrieved via the older `controlled-folders` URL, which redirected).
- **[S6] Microsoft - Windows 10 ESU:** https://learn.microsoft.com/en-us/windows/whats-new/extended-security-updates
- **[S7] Microsoft - BitLocker and Device encryption:** https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/
- **[S8] Microsoft - Secure the Windows boot process:** https://learn.microsoft.com/en-us/windows/security/operating-system-security/system-security/secure-the-windows-10-boot-process
- **[S9] Microsoft - WinGet upgrade:** https://learn.microsoft.com/en-us/windows/package-manager/winget/upgrade
- **[S10] Microsoft - Remote Desktop prerequisites and NLA:** https://learn.microsoft.com/en-us/windows-server/remote/remote-desktop-services/remotepc/remote-desktop-allow-access
- **[S11] Microsoft - SMB security hardening:** https://learn.microsoft.com/en-us/windows-server/storage/file-server/smb-security-hardening (client/server features; check edition-specific linked references).
- **[S12] Microsoft - Edge SmartScreen:** https://learn.microsoft.com/en-us/deployedge/microsoft-edge-security-smartscreen
- **[S13] NIST - SP 800-63B-4, authentication and authenticator management:** https://pages.nist.gov/800-63-4/sp800-63b.html (especially password requirements and syncable authenticators).
- **[S14] Bitwarden - Password/username generator:** https://bitwarden.com/help/generator/
- **[S15] Microsoft - Passkeys in Windows:** https://learn.microsoft.com/en-us/windows/security/identity-protection/passkeys/
- **[S16] Microsoft - Memory integrity requirements, verification and recovery:** https://learn.microsoft.com/en-us/windows/security/hardware-security/enable-virtualization-based-protection-of-code-integrity
- **[S17] Microsoft - Policy CSP, configuration versus result and scope:** https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-configuration-service-provider
- **[S18] NIST - Ransomware prevention and recovery resources:** https://www.nist.gov/itl/smallbusinesscyber/guidance-topic/ransomware
- **[S19] Microsoft - DNS-over-HTTPS client behavior (Server-specific):** https://learn.microsoft.com/en-us/windows-server/networking/dns/doh-client-support
- **[S20] Microsoft - Windows diagnostic data:** https://learn.microsoft.com/en-us/windows/privacy/configure-windows-diagnostic-data-in-your-organization
- **[S21] Microsoft - Consumer ESU eligibility, enrollment and coverage:** https://www.microsoft.com/windows/extended-security-updates (live retrieved page states October 12, 2027; recheck regional terms and device enrollment).

### v0.2.0 primary-source additions

The detailed source-to-control rationale and retrieval record are in [privilege-escalation.md](privilege-escalation.md), [permissions-design.md](permissions-design.md) and platform-gate-review.md. Key Microsoft references:

- [AlwaysInstallElevated](https://learn.microsoft.com/en-us/windows/win32/msi/alwaysinstallelevated): machine/user conjunction and administrative-rights risk.
- [Restrict anonymous SAM enumeration](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-10/security/threat-protection/security-policy-settings/network-access-do-not-allow-anonymous-enumeration-of-sam-accounts): baseline and legacy impact.
- [Limit blank local passwords to console logon](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-10/security/threat-protection/security-policy-settings/accounts-limit-local-account-use-of-blank-passwords-to-console-logon-only): scope and third-party limitations.
- [WDigest advisory 2871997](https://support.microsoft.com/help/2871997): UseLogonCredential behavior; the legacy update is not installed by Secblitz.
- [Automatic logon](https://learn.microsoft.com/en-us/troubleshoot/windows-server/user-profiles-and-logon/turn-on-automatic-logon): registry/LSA-secret distinctions; only nonsecret metadata is assessed.
- [Service security and access rights](https://learn.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights), [QueryServiceObjectSecurity](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-queryserviceobjectsecurity), [SetServiceObjectSecurity](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-setserviceobjectsecurity) and [QueryServiceConfigW](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-queryserviceconfigw): fixed-service rights, query/write scope and identity checks.
- [RSOP_PolicySetting](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/policy/rsop-policysetting) and [SystemServices Policy CSP](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-systemservices): actual resultant settings versus containers; no exhaustive service-DACL authority claim.

### Follow-up references and research gaps

These are primary-source follow-up URLs, not claims of additional completed retrieval:

- Exact lifecycle search, including LTSC/IoT: https://learn.microsoft.com/en-us/lifecycle/
- Windows release health: https://learn.microsoft.com/en-us/windows/release-health/
- ASR per-rule requirements: https://learn.microsoft.com/en-us/defender-endpoint/attack-surface-reduction-rules-reference
- BitLocker recovery: https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/recovery-overview
- Bitwarden passkey storage: https://bitwarden.com/help/storing-passkeys/
- Bitwarden recovery code: https://bitwarden.com/help/two-step-recovery-code/
- Microsoft accessibility: https://www.microsoft.com/accessibility

Direct Microsoft Support URLs attempted for Wi-Fi/accessibility returned 404 in the original pass. Exact client UI/support matrices, OEM requirements, non-Edge browser behavior, app-control recovery, Office policies, boot-certificate rollout and per-rule ASR gates need follow-up before broader automation. **Five advanced Tavily searches are complete** in the supplement. v0.2.0 evidence records registry/wuauserv repair/undo and corrected Italian repack acceptance, not broad prevention. Windows 10, eligible native BITS repair, post-reboot WDigest and wider physical workflows remain open. The six-language CLI/website includes Italian; local preview was launched at http://localhost:57435, not a public deployment. No broader researched feature is thereby marked implemented.
