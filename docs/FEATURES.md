# Secblitz feature and user guide

## 0.6.0 terminal redesign, in development

The current source has **Fix recommended / Review and choose fixes / Check again / Advanced / Exit**. Advanced contains Undo, maintenance, profiles, quality updates, extra tools and technical reports. Navigation refreshes one alternate-screen surface instead of accumulating text. Compact status badges distinguish verified protection, actionable fixes, review and unknown evidence.

Select with Space, review the named recap, then explicitly choose Apply. Back is the default. Change selection preserves the previous selection. PgUp/PgDn read long pages; approval requires actually rendered recap content. Back/Esc returns one level. Progress and the independent post-check update in place before the results screen. Animation honors `--no-animation` and Windows accessibility settings; `NO_COLOR` removes colors, and narrow terminals reflow or pause confirmation when too small.

New `diagnostics`, `operations` and `quality-updates` command groups are described in their localized `--help` and [candidate scope](ROADMAP.md#060-development-candidate). They require separate maintenance/source/EULA consent where relevant. Ordinary setting Undo does not undo Windows servicing, scans or installation.

The following sections describe **published 0.5.0**, whose public downloads remain unchanged. In particular, the candidate now preserves selection on return, has additional diagnostics, uses copy-on-write journals and can recover certain incomplete tails; do not infer these changes are in the older download.

## A safer PC. Without headaches.

Check your Windows security settings, choose supported fixes, and retain original settings for review and undo. **No-argument launch opens the guide and scans first; it never applies repairs by default.**

The live release is at **[secblitz.lol](https://secblitz.lol/)**. [Download 0.5.0](https://secblitz.lol/downloads/secblitz-0.5.0-windows-x64-setup.exe), or use the local [installer](../dist/secblitz-0.5.0-windows-x64-setup.exe) and [checksums](../dist/SHA256SUMS). The genuine published **0.4.3 to 0.5.0** update passed [live acceptance](windows-v050-results.md), including monitor resumption, release-floor advancement and real-guide deferral.

Tested platform: Windows 11 Enterprise Evaluation build 26200.9457 x64. **Windows 10 is targeted but untested.** Windows binaries are unsigned by Authenticode; the updater authenticates signed Ed25519 release metadata separately. Neither is a claim of perfect protection.

## Scan, choose, fix, undo

Setup initially selects **desktop shortcut, guided Finish launch and automatic updates**. The read-only monitor is initially unchecked. Automatic-update preference is retained across upgrades. A silent install does not launch the guide.

| Interaction | Keyboard behavior |
| --- | --- |
| Menu | Up/Down moves; Enter chooses; Esc goes back or exits at the root |
| Fix checklist | Items start unchecked; Space toggles; Enter submits the selection for review |
| Repair review | **Apply these fixes / Change selection / Back**, default **Back**; highlight Apply and press Enter **once** |
| Undo/extra-action confirmation | Default No; highlight Yes and press Enter once |
| Cancellation | Esc cancels; an unchecked checklist submitted with Enter applies nothing |

Space does not approve a confirmation. Production no longer uses typed numeric lists/ranges or `all`/`none` parsing. Ctrl+C remains OS-managed; use Esc for normal cancellation rather than assuming termination preserves all cleanup.

**Fix recommended first** previews the exact current recommended batch; it never applies merely because the menu item was chosen. **Change selection** opens unchecked boxes and replaces, rather than silently extends, that batch. You can also start with individual selection. Only displayed, deduplicated compiled IDs from the current assessment are submitted after Enter-only approval. Back/Esc/empty selection causes neither repair nor extra verification. The engine still rechecks live authority/state. Initial administrator access is not repair consent.

After **every attempted apply or Undo**, the guide captures **exactly one fresh audit automatically**, including no-op, partial and failed attempts, before rendering results. The original operation failure, latest verification failure and last completed operation remain separate. A successful post-check does not erase a failed operation; a failed post-check withholds actionable stale candidates while keeping Undo available. Newly discovered candidates require another explicit approval. Extra actions remain separate and may require a later manual check; repair approval never authorizes downloads, restarts or scans.

Reports use **Protection / Status / What happens next**. Informational findings appear under **More information**, excluded from protection/issue totals. Attention/unknown evidence remains reviewable, not falsely protected. `--details` reveals native evidence. The 18-control/19-finding catalog is unchanged, and **does not mean 37 vulnerabilities**. Readiness has a separate **Device check** heading and does not inflate those counts. All six app languages include these changes.

### Effective protection and device readiness

Firewall observations now carry typed `effective` and `authority` evidence for exactly six firewall IDs. Eligible Local **PersistentStore NotConfigured + ActiveStore Block** is recognized as compliant/unchanged: no saved-preference normalization, repair candidate or new WAL. Genuine local Allow/effective Allow can still be repaired. Managed/Unknown authority cannot grant eligibility; missing, malformed or contradictory evidence cannot produce a green result. Raw originals remain unchanged in schema-1 journals, including NotConfigured undo.

Four independent, read-only readiness signals are included separately:

| Signal | What is measured | Meaning/limit |
| --- | --- | --- |
| System volume | Caller/quota-aware available bytes and read-only volume flag | `u64`, preserving values above 4 GiB; not disk reliability or writable-ACL proof |
| Journal volume | Same facts for the native saved-changes location | Can be Unknown under LocalService; not zero or false |
| Power | AC, battery presence and valid percentage when available | No battery differs from unknown battery/power |
| Windows Update reboot | Native Boolean RebootRequired property only | False is not online update compliance; Unknown is not false |

Only **confirmed read-only system/journal volume or zero available journal bytes** blocks new repairs. Power/reboot notices, unknown probes and zero system-volume space alone are informational, not new blockers. Undo never gates on readiness, but actual storage/authority/journal failures can still prevent it. The collector makes no setting, network, WUA-search, download or journal writes. It is not DISM/SFC, storage cleanup, repair or a new antivirus feature.

### Independent batches and undo

A later selected subset can form a new batch after earlier work is sealed. Each unreverted control has one original-owning transaction. Re-selecting a matching owned control leaves its original intact; selected-owner drift blocks new work in that selection. Incomplete apply/undo blocks subsequent work.

**Undo restores the newest batch only**, leaving earlier batches in place until separate undo actions. It restores recorded settings where current state and eligibility still permit it. It cannot undo Defender remediation, installation, updates, a restart or settings changed elsewhere. Restoring an unsafe original can lower security. Checks/writes are not atomic against external administrators or policy agents.

A corrupt or torn journal fails closed and can make both checking and undo unavailable. Preserve the protected files and investigate; **do not delete or edit a bad journal to force recovery**. No automatic salvage or complete-system rollback exists.

## Commands and reports

```powershell
.\secblitz.exe guide
.\secblitz.exe audit --no-animation
.\secblitz.exe audit --details
.\secblitz.exe history
.\secblitz.exe revert
```

The explicit advanced `apply` command processes the full eligible catalog without guided subset selection. Its existing-unreverted-transaction behavior is more conservative than successive selected batches. There is no arbitrary control/path/force switch.

Audit/history require elevation because they open protected state. The guide requires all three streams to be interactive; redirected guide requests fail before work. For raw reports, start an elevated terminal:

```powershell
.\secblitz.exe audit --json > .\audit.json
.\secblitz.exe history --json
.\secblitz.exe update check --json
.\secblitz.exe update status --json
```

Audit/apply/revert JSON retains `transaction`, `results`, `findings` and adds separate typed `readiness` when collected. Firewall outcomes can carry optional `effective`/`authority`; native evidence and field names are not translated. Known false, Unknown and not-attempted/null remain distinct. History is an array of strings. The schema-1 **WAL**, unlike reports, remains raw originals without this metadata. Updater JSON is unchanged. Do not put reports in the journal directory.

| Exit | Meaning |
| --- | --- |
| 0 | Report has no review-required status, guide/history exited normally, or updater returned a nonfailure outcome |
| 1 | Operational/elevation/I/O failure, or updater `Failed` |
| 2 | Invalid command, review-required report status, or confirmed readiness repair blocker |

A normally exited guide can return 0 after showing warnings. An updater `DeferredBusy` or `WorkerStarted` also returns 0 without proving installation. Read the result, not just the exit code.

## Keeping Secblitz up to date

The installer registers owned root task `SecblitzUpdate` as SYSTEM when selected. It invokes only installed `secblitz.exe update check`, first one hour after registration and then every hour. Runs require the machine and scheduler to be available. **No `StartWhenAvailable` catch-up is configured**; power-off, sleep, busy sessions or network failures can delay updates. An hourly schedule is not an hourly freshness deadline. Setup registers the task but does not trigger a feed check.

The updater checks fixed HTTPS, forbids redirects, verifies exact signed metadata and installer size/SHA-256, and revalidates before fixed silent setup. A protected **highest-seen release floor** rejects replay below a persisted higher version, same-version hash substitution and older metadata timestamps; valid unchanged-content renewals remain allowed. Floor/status/staging writes use a flushed protected temporary file and atomic native replacement. `WorkerStarted` is a handoff; `Installed` follows setup success and version validation. The worker waits for setup **without a 15-minute kill deadline**, holding locks/payload pin. This is not transactional installer rollback or full TUF.

An open installed guide causes deferral rather than force-close. Update and engine operations share the base `engine.lock`; `Updates/update.lock` serializes updater work. The worker uses a sanitized environment and detached output handles so captured parent JSON reaches EOF promptly.

| Outcome | Interpretation |
| --- | --- |
| `NotConfigured` | No configured update source; with `checked_at=0`, no saved update information yet |
| `UpToDate` | Verified feed is the running version; not proof of all-PC patch compliance |
| `DeferredBusy` | Conflicting activity; retry on a later check, without closing the user session |
| `WorkerStarted` | Separate worker started; installation not yet established |
| `Installed` | Setup and installed-version check succeeded |
| `Failed` | Read the local result and investigate; do not assume no partial changes occurred |

0.5.0 uses **https://secblitz.lol**. Legacy clients retain their compiled beacons.lol origin and receive feeds/downloads directly. The old root and primary www redirect safely; updater endpoints do not, and mail was untouched. No runtime endpoint override exists. Published 0.4.0 still requires manual upgrade to fix its historical worker defect.

Uninstall removes only the owned updater task, preserves data and records the updater preference disabled. Interactive Setup can change an existing enabled preference. A silent upgrade with an empty task list or `/SECBLITZUPDATE=1` preserves the saved preference rather than silently opting out. See [update contract](update-contract.md) for exact scheduling, flags and limits.

## Optional actions

These require separate choices and are outside the reversible control journal:

| Action | What it does | What it does not prove |
| --- | --- | --- |
| Defender update | Uses configured sources; can download protection updates | Latest signatures merely because a command returned |
| Defender quick scan | Requests a scan, possibly remediation under existing policy | Threat-free system or broad prevention efficacy |
| Install/start monitor | Installs if absent and starts fixed LocalService monitor | Report freshness merely from SCM Running, or automatic healing |
| Open Windows Settings | Requests one fixed update/security/encryption/sign-in page | Update installed, disk encrypted or account changed |
| Generate password | 24 OS-random characters, interactive display only | Saved vault entry or changed account password |
| Bitwarden installation | Exact approved user-scope package through verified WinGet source | Account/vault creation, imports, extension/autofill/MFA setup |

Defender/tamper protection is not disabled for supported scan/update actions. Timeouts/errors can leave work in flight; review Windows Security before retrying. Earlier quick-scan timestamp and offline-update tests retain their historical scope; the live 0.5.0 upgrade did not rerun them.

Bitwarden must run in the original non-elevated desktop account. The elevated guide can return fixed request codes **23 Bitwarden, 24 Windows Update, 25 Windows Security, 26 encryption, 27 sign-in** to its waiting original parent. The parent checks its context and asks again; only explicit Return re-elevates. No arbitrary URI/command travels in those codes. Settings dispatch independently rejects an elevated token. The **full standard-user, over-the-shoulder UAC and original-user broker path remains untested end to end**.

Password output is never copied to clipboard or included in journals/reports, but scrollback, screen sharing and recording can retain it. No existing password, hash, vault, token, recovery key or LSASS content is inspected.

## Exact implemented scope

### 18 repair controls

The guide applies only selected eligible controls; presence in this catalog does not override management, provider or state gates.

| # | Control ID | Fixed repair and boundary |
| --- | --- | --- |
| 1 | `defender.realtime` | DisableRealtimeMonitoring=false; preserve exclusions |
| 2 | `defender.behavior` | DisableBehaviorMonitoring=false |
| 3 | `defender.ioav` | DisableIOAVProtection=false |
| 4 | `defender.archive` | DisableArchiveScanning=false; no independent archive runtime flag |
| 5 | `firewall.domain.enabled` | Enable Domain profile; preserve rules/outbound policy |
| 6 | `firewall.domain.inbound` | Persistent default inbound Block |
| 7 | `firewall.private.enabled` | Enable Private profile |
| 8 | `firewall.private.inbound` | Persistent default inbound Block |
| 9 | `firewall.public.enabled` | Enable Public profile |
| 10 | `firewall.public.inbound` | Persistent default inbound Block |
| 11 | `uac.enabled` | Explicit EnableLUA 0 to 1; restart needed |
| 12 | `uac.consent` | Explicit admin consent 0 to 5; preserve other nonzero modes |
| 13 | `installer.always_install_elevated` | Machine 1 to 0; no HKCU write; absent/safe preserved |
| 14 | `lsa.restrict_anonymous_sam` | Explicit 0 to 1; legacy anonymous enumeration can be affected |
| 15 | `lsa.limit_blank_password_use` | Explicit 0 to 1; console access preserved; no password inspection |
| 16 | `wdigest.use_logon_credential` | Explicit 1 to 0; stored configuration, not proof of cleared running credentials |
| 17 | `permissions.service.bits` | Narrow explicit broad-principal service grants, fixed native identity/simple supported ACL only |
| 18 | `permissions.service.wuauserv` | Same bounded repair for Windows Update only |

Defender writes require a recognized active provider and no tamper block. Leave tamper protection enabled. UAC/registry absence and safe settings are preserved. Local inherited firewall Block is now recognized without rewriting NotConfigured; missing/contradictory effective evidence remains reviewable. Firewall apply requires exact raw plus verified effective readback before sealing; undo restores the exact original even when it is intentionally less protective. Rules/outbound defaults are not reset.

Service repairs remove CHANGE_CONFIG, DELETE, WRITE_DAC and WRITE_OWNER from supported explicit ALLOW ACEs for Everyone, Authenticated Users and Builtin Users, mapping generic rights as needed. They preserve safe rights, other ACE bytes/order/padding, owner/group and supported flags. NULL/absent, deny/inherited/flagged/complex ACLs are ineligible. Only **BITS and wuauserv** are mutable, not arbitrary service executables, Windows files or third-party software. These controls reduce specific paths, not all privilege escalation.


### 20 extended hardening controls (catalog: `src/hardening.rs`)

One compiled table drives the engine, wire validation and the PowerShell backend (`src/platform/hardening.ps1` receives the spec as JSON, so Rust and PowerShell cannot disagree). State is a slice `{"items": {key: int|null}}`; `null` is "not configured". A write may only move a key between its recorded unsafe original and its fixed value; a key that drifted to anything else blocks the write and the undo. Absent values that equal Windows' own safe default count as protected. Dynamic controls (firewall rules, saved Wi-Fi networks) are compared only on the journaled keys. No child process is started: everything uses cmdlets, .NET, ADSI, registry or Reflection.Emit P/Invoke stubs. Controls marked ask are choices: never pre-selected, always shown with a one-line consequence.

| Control ID | Mode | Repair (unsafe original to fixed value) |
| --- | --- | --- |
| `defender.cloud_protection` | ask | MAPSReporting 0 to 2, block-at-first-sight on; strict Defender gate (tamper protection blocks) |
| `defender.pua` | auto | PUAProtection 0 or audit to Enabled |
| `defender.script_nis` | auto | DisableScriptScanning / DisableIntrusionPreventionSystem true to false |
| `defender.asr.standard` | auto | Rules 56a863a9 (drivers), 9e6c4e1f (LSASS), e6db77e5 (WMI) off or audit to Block via Add-MpPreference; never replaces the list; needs real-time protection; skipped under Configuration Manager |
| `defender.asr.web_script_email` | ask | Rules d3e037e1, 5beb7efe, be9ba2d9 off or audit to Warn; needs cloud protection |
| `lsa.run_as_ppl` | ask | RunAsPPL absent or 0 to 2 (never 1); only with Secure Boot on, Smart App Control off, no CodeIntegrity 3033/3063/3065/3066 events in 30 days and no third-party LSA packages; restart |
| `net.public_sharing_exposure` | auto | Built-in FPS-* / NETDIS-* inbound allow rules enabled on Public lose the Public profile (Public-only rules are disabled); no rule deleted, no network relabelled |
| `printer.point_and_print` | auto | Remove RestrictDriverInstallationToAdministrators=0, NoWarningNoElevationOnInstall=1, UpdatePromptSettings=2; vetoed when other Point and Print policy exists; Spooler untouched |
| `net.llmnr` | auto | Policies DNSClient EnableMulticast absent or 1 to 0; restart |
| `accounts.lockout_policy` | auto | Local lockout threshold 0 to 10 via ADSI; duration/window untouched |
| `autorun.disabled` | ask | NoDriveTypeAutoRun=255, NoAutorun=1; restart |
| `wifi.risky_profiles` | ask | Saved all-user open/WEP/WPA-TKIP profiles auto to manual via WlanSetProfile; nothing deleted, keys never read |
| `lsa.restrict_anonymous` | ask | RestrictAnonymous 1, EveryoneIncludesAnonymous 0, RestrictNullSessAccess 1; restart |
| `remote_assistance.disabled` | ask | fAllowToGetHelp 0 |
| `wsh.disabled` | ask | Windows Script Host Enabled 0 |
| `update.auto_policy_disabled` | ask | Remove local NoAutoUpdate=1, AUOptions=1, DisableWindowsUpdateAccess=1; vetoed by update-server policy |
| `ntlm.lm_compat_level` | ask | LmCompatibilityLevel to 5; restart |
| `accounts.builtin_administrator` | ask | Disable RID-500 only when another enabled administrator exists |
| `privacy.activity_history` | ask | Activity feed policies to 0; never counts against the score |
| `privacy.advertising_id` | ask | DisabledByGroupPolicy=1; never counts against the score |

Every control reuses the domain, MDM/enrollment, policy, RSOP and local-policy-artifact gates; preflight conditions (such as the LSA checks) apply to repairs only and never to undo. Not implemented because the value names are unconfirmed by official Microsoft documentation: `driver.vulnerable_blocklist`, `net.wpad`, `smartscreen.apps`, Nearby Sharing, KernelShadowStacks.

### 19 advisory findings

| Finding | Evidence boundary |
| --- | --- |
| Security providers | Registration is not provider health |
| Windows Firewall | ActiveStore profiles; no rule/reachability audit |
| Defender | Available runtime/signature/exclusion counts; not a malware verdict |
| Windows lifecycle | OS/build guidance; ESU entitlement unverified |
| Device encryption | Volume status; recovery backup unverified |
| Secure Boot | Firmware query; unsupported/inaccessible is unknown |
| Windows updates | Offline cached information; zero is not current patch compliance |
| Remote Desktop | Incoming preference; not NLA/listener/Internet exposure proof |
| SMB1 | Feature state; not negotiated signing/dialect assessment |
| SmartScreen | Guidance, no comprehensive effective browser probe |
| Local accounts | PasswordRequired flag count, not password presence/strength/reuse |
| Memory integrity | HVCI configured/running; driver compatibility untested |
| Management and mutation eligibility | UAC-family authority probe, not all-control authorization |
| Automatic logon | AutoAdminLogon plus DefaultPassword name presence only; no secret value reads |
| Service permissions: BITS | Bounded ALLOW candidate scan, not effective AccessCheck |
| Service permissions: wuauserv | Same; repair eligibility is separate |
| Service permissions: WinDefend | Audit only |
| Service permissions: Schedule | Audit only |
| Service permissions: SecblitzMonitor | Audit only; absent service is informational |

Engine recovery and assessment-failure messages are separate. Missing APIs, managed state and unreadable evidence must not be counted as confirmed vulnerabilities or successful repairs.

## Local data and monitor

The protected native ProgramData/Secblitz root holds schema-1 journals and `engine.lock`. Journals support 128 KiB lines, 1 MiB WALs and 2,048 transactions, with exact service-descriptor bounds. SIDs/permission history are sensitive operational metadata; keep raw originals local rather than publishing them.

Updater files, including protected `release-floor.json`, live in **ProgramData/Secblitz/Updates**. The engine permits that directory and exactly five legacy root updater files with type/link/ACL checks; they are not WALs or permission for arbitrary root files. The new floor records authenticated version, installer hash, target and timestamps, not credentials. It is re-read before worker acceptance and not reset when malformed.

The monitor writes native Program Files/Secblitz/Monitor/latest.json on startup/about every 15 minutes as LocalService, without repair/undo or administrator-WAL access. Reports now include typed firewall metadata and a separate readiness object. The live resumed report was **9,195 bytes**, below the 64 KiB cap; this is report size, not process-memory usage. Journal-volume readiness was explicitly Unknown under LocalService. Report writes remain in-place and are not authenticated evidence; the updater's atomic replacement does not imply monitor reports are atomic. Uninstall preserves reports/journals.

## Current validation and limits

Native 0.5.0 acceptance passed **138 library + 66 CLI tests**, a separate real-readiness smoke and **nine SYSTEM updater tests**. The ordinary library run ignored 11 cases; nine SYSTEM cases plus the readiness smoke were explicitly run, while the obsolete public-feed probe remained ignored. Two attended CLI probes remained ignored; the actual application UI was tested instead. The final text-only follow-up reran 66 CLI tests with unchanged core/library evidence. The live phase used **genuine published 0.4.3**, not a spoofed starting version:

- Owned SYSTEM task: `Installed` **18.865 s**, exact published 0.5.0 bytes/version; running LocalService monitor resumed with Auto startup preserved.
- Next check: `UpToDate` **1.336 s**.
- Real guide: `DeferredBusy` **1.005 s**, same PID responsive, no forced close.
- Audit: **18 results / 19 findings**, four readiness signals, typed inherited Block compliant. Baseline guide: zero recommendations, 18 protected, nine needing choice; ten informational findings excluded from that count.
- Authentic prior release floor advanced to **0.5.0**, matching installer hash and publication timestamp **1791022530**, with SYSTEM/Administrators-only protection; current/busy checks left its bytes unchanged.
- All 18 baseline states, original data and eight genuine journal copies unchanged; normal uninstall/cleanup restored the isolated lab's original state.

Only Windows 11 Enterprise Evaluation build 26200.9457 in `Secblitz-W11-UI-Test` was used. Windows 10, broader Home/Pro/hardware/accessibility coverage and complete original-standard-user broker execution remain untested. Authenticode signing, physical power-loss durability and comprehensive prevention efficacy are not established.

The public homepage is current **0.5.0**; its actual **0.3.1** Remotion/Windows footage remains illustrative, muted/looping, with no captions or playback controls. Media uses content-hashed public names; source/provenance/test code stays outside allowlisted publication. The recording is not current updater evidence or six-language runtime coverage.

See [security review summary](SECURITY-REVIEW.md), [security model](security-model.md), [update contract](update-contract.md), [live evidence](windows-v050-results.md) and [roadmap](ROADMAP.md). Recommended-batch approval, inherited-protection recognition, automatic verification and read-only readiness are implemented. DISM/SFC, general cleanup, broader app/Windows updating and backup provisioning remain **future work**, not hidden automatic features.
