# Secblitz 0.5.0 security model

Reviewed against published 0.5.0 and 0.4.3 to 0.5.0 LIVE E2E PASS, 2026-10-03. See [security review summary](SECURITY-REVIEW.md) for fixed cases versus known boundaries, [features](FEATURES.md), [updater](update-contract.md), [permissions](permissions-design.md), [readiness review](readiness-security-review.md) and [roadmap](ROADMAP.md). Historical review-time compilation, deployment and replay/persistence blockers are not current release failures when superseded by recorded fixes/acceptance.

## Unreleased 0.6.0 additions

Candidate journals publish flushed snapshots atomically. Recovery preserves damaged bytes as evidence, uses only committed validated originals and refuses completed malformed records/interior corruption. A legal incomplete tail is not general media-corruption repair; old schema-1 journals lack authenticated integrity checksums.

New protected `operations` and `Patching` namespaces have durable typed records. Preference apply/undo, maintenance, quality updates and self-update share `engine.lock` and inspect each other's unresolved records. Losing a supervisor cannot turn an uncertain operation into idle merely because its lock was released. Audit/history remain accessible where journal validation itself permits. Interlock checks are read-only leaf functions under the caller's existing lock.

Maintenance accepts only compiled operation kinds, exact digest-bound approval and fresh ownership/readiness permits. System executables, configuration and imported module payloads are pinned with ACL/owner checks. Jobs supervise descendants; timeout/cancellation stops future phases without terminating live servicing. Recovery verifies rather than replaying mutations. SFC diagnostic completion is not a clean-integrity claim. Quality-update source, root and bundle IDs/revisions, metadata and EULAs are revalidated before submission; missing acknowledgments retain uncertainty, including service-hosted work.

The candidate terminal records consent visibility only after successful frame output. Resizing, clipping or output failure cannot create permission to apply unseen recap content. Color comes from typed roles, not translated text. Original-user inventory rejects elevated substitute accounts. The new protected maintenance commands do not infer authorization from a UAC prompt.

See [candidate scope and evidence](ROADMAP.md#060-development-candidate). These source additions are peer-reviewed and partly native-tested; they are not a completed external audit or evidence of live servicing/update acceptance. The remaining sections retain their **published 0.5.0** scope.

## Purpose, assets and assumptions

Secblitz is a bounded Windows security assessment/repair tool, not a replacement antivirus, malware-removal engine or complete privilege-escalation prevention system. The compiled catalog remains **18 repair controls and 19 advisory findings**. The automatic updater changes Secblitz software, not this catalog's Windows security settings. Optional Defender scans/updates are separate explicit actions.

Protect: the fixed mutation scope; exact before-images and transaction ownership; user access/recovery; administrative policy authority; installed/staged executable integrity; and confidentiality of secrets and sensitive metadata.

Assume a trusted Windows kernel, authorized administrator, executable/build and inbox modules, with functioning local ACL enforcement. Hostile administrator/SYSTEM, compromised kernel/firmware/hypervisor or maliciously substituted build artifacts are outside the journal's local trust boundary. Unprivileged file substitution, malformed input/state, stale observations, interrupted operations, concurrent instances and policy/provider conflicts are in scope.

“Reversible” means attempting exact restoration of recorded settings under current state/authority checks. It does not mean all side effects can be undone, a whole-PC snapshot, or atomic all-or-nothing changes across Windows APIs. A repair and an external policy write can still race.

## Architecture and privilege separation

```text
Original desktop CLI
  -> Guide: typed assessment/readiness -> exact recommended/selected batch approval
     -> Elevated engine: fixed IDs, scoped gates, exact originals, newest-first undo
        -> Platform backend: 16 static controls and 14 findings
        -> Native permissions wrapper: 2 fixed DACL controls and 5 findings
  -> Fixed requests 23..27 back to unelevated original parent for renewed consent

Optional LocalService monitor -> observations only, no journal/remediation
Installer-owned SYSTEM task -> fixed signed updater -> protected worker -> setup
```

The guide and audit/history require elevation for protected journal access. Assessment does not change security preferences but can create/open state and lock files and read machine identity. It is not an unprivileged zero-side-effect audit.

### Keyboard consent and reports

No arguments selects `guide`, not apply. stdin/stdout/stderr must all be interactive before guide elevation/work. Arrow keys navigate and Space toggles unchecked checklist entries; Enter submits for review. The repair review defaults to Back and accepts **one Enter** on Apply. Undo/extra-action confirmation defaults to No and accepts one Enter on Yes. Space/text never approves confirmation. Esc cancels/backs out; Ctrl+C remains OS-managed, not a guaranteed cleanup path.

Fix recommended and individual selection share a deduplicated candidate builder from the current snapshot: compiled ID, attention outcome, recognized repair advice and no pending recovery. The recommended path previews exactly those IDs with **Apply / Change selection / Back**, default Back. Only one Enter on Apply approves. Change selection starts unchecked and replaces the previous subset. A snapshot generation check precedes dispatch; UI generation never replaces live engine authority. `apply_selected` rejects empty/duplicate/unknown IDs before WAL work and repeats gates. Advanced `apply` remains the deliberate full-catalog path, not an automatic action on launch.

Every attempted guided apply/undo captures **exactly one fresh audit before any result rendering**, including failures, partial reports and no-ops. The operation result/error, verification error and last completed operation remain separate; a good post-check cannot erase failed work. Failed verification leaves no actionable snapshot and retains Undo. Back/Esc/empty/decline does not write or trigger extra audit; newly found recommendations require new consent. Extra tools remain separate. Human tables retain opt-in native details; JSON adds typed optional evidence/readiness without modifying raw WAL originals. Terminal sanitation is not JSON escaping for downstream consumers.

For audit/apply/revert, non-success/review statuses or **confirmed readiness storage blockers** produce exit 2; operational failure is 1. Unknown/power/reboot readiness alone does not create a new failure rule. Guide/history can exit 0 after displaying review items. Informational findings are now More information and excluded from issue/protection counts; readiness has its own Device check. Counts still are not independent vulnerability totals or a whole-PC security verdict.

### Typed firewall evidence: implemented correction

Exactly six firewall IDs may carry typed `effective` Enabled(bool)/Inbound(Block|Allow) and `authority` Local/Managed/Unknown. Missing metadata is representable for compatibility but cannot prove protection. Managed/Unknown with eligible=true is rejected by model validation; wrong-ID/kind metadata is rejected. Effective evidence never replaces the stored raw preference or schema-1 before-image.

Eligible Local raw NotConfigured plus effective Block is protected/unchanged: no setter or WAL. Genuine Allow/effective Allow remains repairable. Explicit contradictory raw/effective state, missing profile, duplicate/wrong profile, stopped services or unreadable authority cannot become a protection claim. Managed tagging uses exact structured gate metadata through bounded wrapper traversal, not arbitrary exception text; uncertain vetoes remain Unknown.

Native observation refreshes effective evidence after authority checks; pre-setter observation repeats raw/effective/gate consistency. Apply requires exact raw target plus effective eligible Local protection before Applied/Sealed. Failed effective readback retains pending recovery. Undo deliberately restores raw originals, including Allow/false/NotConfigured, without falsely requiring a newly protected state. Existing owned raw-target drift is still a conflict even if the replacement setting is effectively protective. Native 0.5.0 evidence confirms zero recommendations and no new WAL for all three inherited Block profiles, resolving the old 0.4.2 false-fix case.

### Read-only readiness boundary

Four independently typed probes report system-volume and journal-volume available bytes/read-only flags, power, and Windows Update RebootRequired. Unknown is distinct from Known(false), Known(0) and not-attempted/null. `GetDiskFreeSpaceExW` uses caller/quota-aware `u64` availability; volume read-only is a native flag, **not ACL/write-access proof**. Power sentinel validation preserves unknown AC, absent battery and valid percentages separately.

Paths come from native Windows/ProgramData resolution, not caller strings/environment fallback. Existing directory components are opened without following reparse points and pinned before volume queries; remote/device/reparse paths conservatively return Unknown. The collector does not create files/directories, mutate registry/settings, start children, access the journal, request network/search/download or perform repairs. Native Windows internal caching/logging is not claimed absent.

The WUA COM helper reads only the fixed SystemInformation RebootRequired property. It requires validated high/system integrity, machine REG_SZ registration pinned to the documented CLSID, in-process activation and canonical VT_BOOL. Lower/unreadable integrity or errors return Unknown before unsafe fallback; no HKCU, shell/moniker, alternate class or elevation fallback. Objects/apartment/VARIANT lifetimes are bounded and cleaned on the worker. A two-second receiver wait and single outstanding-worker slot prevent repeated hung COM-worker growth; synchronous volume calls have no universal two-second deadline. A stuck worker is not force-killed.

**Only confirmed system/journal read-only state or zero journal available bytes blocks new repairs.** Unknown and power/reboot facts remain informational; zero system bytes alone is not a new gate. Already-owned no-ops retain unchanged outcomes and originals. Readiness blocking occurs before new WAL/Prepare/write; it never gates Undo. Actual journal storage/authority failures still apply and can precede readiness reporting. The live LocalService report correctly had Unknown journal-volume readiness, not zero or a green result.

### Original-user broker and optional actions

The elevated guide can return only fixed request codes: **23 Bitwarden; 24 Windows Update; 25 Windows Security; 26 device encryption; 27 sign-in**. No arbitrary URI, executable, path or installer arguments cross that protocol. The waiting original parent checks interactive/unelevated context and asks again. Decline/Back/Esc or input cancellation ends the handoff; only explicit Return requests UAC again. Starting already elevated gives manual instructions instead of launching user protocol handlers as administrator.

`actions::run` accepts a closed enum. Settings dispatch allows only `ms-settings:windowsupdate`, `windowsdefender`, `deviceencryption`, `signinoptions`, and independently rejects the actual elevated token. Allowlisting alone is insufficient because protocol handlers can be user-writable. Opening a page does not prove page availability or remediation. Full original-standard-user, over-the-shoulder UAC/broker execution is still untested end to end.

Defender update/quick scan use fixed support operations through trusted PowerShell, provider/management gates and existing Defender policy. These do not require disabling tamper protection and never alter it. A command return is not independently verified latest signatures, scan completion or threat absence. Timeout/error may leave work in progress. Scan remediation and downloads are outside preference rollback.

Monitor start installs only if absent, then explicitly starts the owned service. Running is not freshness; failure can leave installation in place. Bitwarden requires original non-elevated desktop-user identity, registered App Installer resolution, expected Microsoft WinGet repository metadata, fixed exact user-scope package arguments, hash checks and post-install detection. No automatic vault creation, credential import or extension/MFA enrollment occurs.

## Fixed mutation gates

Every static write repeats supported x64 client/build, domain, native MDM registration, OMADM/cloud join and relevant policy checks. Native MDM uses `IsDeviceRegisteredWithManagement` with strict result/BOOL interpretation and no optional UPN collection. Enrollment templates or unrelated provider defaults alone are not active policy.

Policy scope is per family: UAC options; Defender/ADMX Defender; firewall/MDM store; Installer/ApplicationManagement; individual LSA options; WDigest authentication. Relevant staged/orphaned entries or ambiguous metadata veto. Explicit inactive current metadata is narrowly recognized; internal provider registry semantics are not claimed exhaustive. Local Registry.pol/gpt.ini and relevant security-template/preference artifacts remain conservative vetoes. Only exact typed missing initial RSOP namespace can mean absent optional evidence; access/class/provider failures do not.

Defender preference writes require its recognized active provider, strict Boolean runtime state and no tamper block. Do not disable tamper protection to become eligible. Firewall writes preserve rules/outbound policy and require services, resultant policy and effective readback. UAC repairs explicit zero only; the four registry controls repair explicit unsafe binary DWORDs only. Absent/safe values and unsupported types are not coerced.

The management advisory uses a UAC-family gate, not an authoritative all-control inventory. No lifecycle-entitlement, complete workflow, network reachability or peripheral-compatibility gate exists. NAS, printers, VPNs, developer tools, gaming and accessibility dependencies still require user context.

### Native service-DACL scope

Only `permissions.service.bits` and `permissions.service.wuauserv` are repairable. WinDefend, Schedule and SecblitzMonitor are audit-only. Service executable/DLL/registry/filesystem ACLs, arbitrary third-party services and other principals are outside repair scope. Production findings examine dangerous ALLOW candidates, not effective AccessCheck results.

Repair requires trusted owner, recognized LocalSystem shared-process system-svchost identity, supported descriptor semantics, and repeated `permission_gate` authority checks. The service branch checks SystemServices plus BITS/Update policy areas and actual resultant computer policy, not firewall eligibility. Binary metadata/pinning is not Authenticode or complete executable-chain integrity proof.

For supported explicit ALLOW ACEs on Everyone/Authenticated Users/Builtin Users, remove CHANGE_CONFIG, DELETE, WRITE_DAC and WRITE_OWNER, mapping generic rights if necessary. Preserve safe rights, unrelated ACE bytes/order/padding, owner/group and supported descriptor flags. NULL/absent, deny/inherited/flagged/object/callback or unknown semantics are ineligible. Only DACL is written; SACL/owner/group/service configuration/start-stop are not changed.

Native write validates the exact forward repair or inverse restoration relationship, repeats state/identity/gate checks, and requires exact readback. The protected engine journal supplies rollback provenance; an inverse-transform match alone is not authorization for arbitrary unsafe grants. There is no SCM compare-and-swap; SACL is not a drift fingerprint, and identical delete/recreate across restarts is not distinguishable.

## Trusted execution boundary

The platform resolves inbox Windows PowerShell under the native Windows directory, clears inherited environment, restricts module paths/autoloading and uses a fixed bootstrap/private script pipe. Caller input is only compiled action/ID plus typed Boolean, enum or strict registry state. No caller script/path is accepted. Execution-policy bypass applies to the child, not persistent machine policy.

A kill-on-close one-process job and roughly 90-second post-spawn/2 MiB output bounds limit PowerShell operations. Synchronous process creation is outside that deadline. Genuine stderr/nonzero exits/invalid JSON fail; global progress suppression prevents the previously observed CLIXML leak without hiding errors. Some modules requiring helpers can yield unknown findings. This is not a sandbox against compromised trusted modules.

Writes check stored state plus available Defender runtime flags/firewall effective state. Archive lacks an independent runtime flag. NotConfigured undo restores the recorded persistent preference, not historical effective behavior. UAC/WDigest deferred effects need restart/sign-out verification. The engine independently observes exact target/original equality before durable completion. Findings transport failure becomes unknown while preserving completed operation results; post-Prepare mutation failures retain recovery intent.

## Journal, batching and crash behavior

Native ProgramData/Secblitz is an administrator/SYSTEM-owned protected local directory. Owner/DACL, reparse/hardlink and entry checks reject untrusted existing state rather than repairing/adopting it. Ancestor/root handles reduce replacement races. There is no journal encryption/signature; trust comes from OS ACLs and strict parsing under the privileged-user assumption.

Schema remains **1**, with machine identity, canonical sequence/UUID filenames, typed before-images and bounded ordering. Limits: **128 KiB per line, 1 MiB per WAL, 2,048 transactions**. Service snapshots are canonical `dacl-v1:<hex>` with owner/group/flags/exact DACL; decoded bound 16 KiB, native query bound 8 KiB. The catalog sentinel is never a write value. All service recovery targets derive from durable originals, not fresh current state. Legacy static-control journals remain compatible.

The entire active stack is validated. Each unreverted control has exactly one owner; only the newest active batch can be incomplete/reverting. Selected apply may add independent work after earlier batches seal. Preflight selected owned controls against original-derived targets; drift/probe failure blocks new work in that selection. Matching owned controls retain originals. Selected pre-Prepare errors can be per-control; post-Prepare failures retain pending intent. Full-catalog `apply` keeps conservative existing-transaction behavior.

Each mutation follows durable Prepare, fresh gate/state check, native write/readback, independent engine verification, then Applied/Sealed. Undo of everything handles the newest unreverted batch in reverse order, then the one before it. A person can also put back one or several chosen controls, each to the value recorded before Secblitz changed it. That restore is written inside the batch that owns the control, as its own restore records, and the batch is closed as reverted once every control in it is back. A restored control no longer owns anything, so it can be fixed again later. The same checks apply as for undo of everything: the control is read fresh, and a value the person changed since is left alone and reported. For a chosen restore, the pending-restore record is only written after that fresh read passes, so a conflict leaves the journal untouched. Memory integrity waits for stack protection, so choosing memory integrity while Secblitz still owns stack protection skips it and leaves it as it is. An older build cannot read a journal that used this rule and stops safely instead of guessing, and the updater cannot install an older build, so this cannot happen through updates. Current original means no write; current expected target plus authority allows restoration; third state means conflict. Newer conflicts block older undo. Reused durable restore intent avoids unbounded duplicate retry records. Exact UAC/registry preservation reasons allow narrow restore exceptions only after successful gates; permissions have no such exception.

File identity/validated-length checks guard live appends and lock ownership. They do not detect all same-length privileged edits or a valid historical prefix truncated before reopening. MachineGuid is not hardware attestation. Locks serialize cooperating instances, not external policy/settings writers. There is no atomic cross-provider transaction or guaranteed ABA detection.

**Torn/malformed WAL fails closed before replay and can make Undo unavailable.** No automatic truncation, deletion or salvage is implemented, and a damaged file is never edited. The open fails with a typed damage error (partial, total, or history from another PC), and the elevated GUI offers one choice: after two explicit confirmations it moves every journal file whole into `Damaged/<UTC time>` (same administrator-only DACL, SHA-256 of each file in `damage.json`, at most five sets kept), under `engine.lock` and the updater interlock, and rolls the moves back if any step fails. Earlier fixes then stay applied with no recorded undo, and History records it. This path is reachable only from a click in the elevated GUI, never from the service, the broker or the command line. Do not delete a bad file to get a green result. Flushes/logical crash tests are not physical power-loss certification. Full WAL/retention limits can constrain recovery. Future diagnostics/maintenance need separate operation semantics, not a fiction that this preference journal reverses everything.

### Updater coexistence

Current updater data, including `release-floor.json`, stays under protected **ProgramData/Secblitz/Updates**, with SYSTEM/Administrators access. The engine still recognizes only that directory and five exact legacy root updater files, with wrong-type/link/unknown-name/corrupt-WAL rejection. A corrupt updater floor blocks updating but is isolated from WAL parsing. These exceptions do not admit arbitrary root files.

Updater operations use `Updates/update.lock` but share the **base engine.lock**. Native tests cover layout/type/hardlink/pinning and shared contention. In the real migration, old root files and the new child coexisted with eight genuine journals, and the guide opened normally. The earlier namespace collision is resolved, not a current vulnerability.

## Updater and installer trust

The [update contract](update-contract.md) is authoritative. Origin **https://secblitz.lol** and Ed25519 key are compiled, not runtime-selected; legacy beacons.lol feed/download routes remain direct while roots/www redirect safely. Client HTTPS redirects are forbidden. Exact signed-byte, schema/version/time/size/hash checks repeat before staging and worker launch. Since 0.4.3, protected **highest-seen release state** additionally rejects observed-release rollback and same-version installer hash/target substitution; it is not full TUF.

Both check and worker reload a strictly parsed, bounded floor under the update lock and atomically persist advancement before download/launch/equality success. Failed download cannot erase observed higher metadata. Same-version publication/expiry cannot move backward; identical metadata is idempotent and valid unchanged-content renewal can advance after the old record expires. Corrupt/unsafe floor state fails closed, never silently resets. The local floor itself is not expired away. Trust still excludes privileged state erasure and signer/build compromise; hosting can withhold delivery or replay eligible releases never superseded locally. Trusted time, root/key rotation and thresholds are not solved by this fix.

Manifest maximum 16 KiB, payload 8 KiB, installer 64 MiB; hosting imposes 25 MiB. Metadata validity is at most 90 days with ten-minute tolerance and needs renewal before expiry. Network connection timeout is 15 seconds, with a **shared 120-second download budget**. This is not an installation timeout.

The fixed protected worker must byte-match the current trusted installed image, use a sanitized trusted environment and fixed silent setup flags, and retain update/base-engine locks plus the verified payload pin while setup runs. **It waits for setup exit without a 15-minute force kill.** It does not release locks around a still-running installer. Installed requires successful setup and version validation. A hung process can retain locks; there is no transactional binary rollback promise.

Status/metadata/staging replacement now creates an exclusive protected same-directory temporary file, writes/flushes it, validates destination security and uses **MoveFileExW replacement/write-through** with no unlink-first or cross-volume fallback. Failed write/pre-switch validation/replacement preserves the prior destination. Only the invocation's temp is cleaned; stranded protected temps are not adopted. Payload pins continue denying write/delete/replacement. This corrects the tested missing/partial-state window, not hardware power-loss or transactional installer rollback. Generic status reasons omit native URLs/details; detached worker output preserves parent JSON EOF. Busy UI defers without force-close.

Setup defaults desktop and automatic updates on, monitor off, Finish guide launch on. Silent setup skips launch; `runasoriginaluser` cannot de-elevate an already elevated installer. The owned SYSTEM task starts one hour after registration, repeats hourly when available, ignores overlapping instances and has a one-hour task limit. It may run on battery; **no StartWhenAvailable catch-up** is configured. Task execution limit is distinct from worker installer waiting. No hourly installation deadline is promised.

Task path/action/principal/security and saved preference are validated before replacement/removal. The measured task DACL grants Users read/run authorization but not XML/data write, delete, owner or DACL modification. Requesting the fixed signed-update action is not arbitrary SYSTEM code selection; alternate-user Scheduler RPC was not established by the runner. Interactive/silent-upgrade preferences are preserved. Uninstall removes owned registration/files and retains journals/reports. Embedded protected maintenance, fixed paths/clean environment and narrowly scoped Inno metadata handling remain; no wildcard deletion or ACL reset.

Published binaries remain **Authenticode-unsigned**, explicitly accepted as a preview because no real certificate was available. A first browser download and same-site checksum are not independent publisher trust if hosting is compromised. Genuine installed-client Ed25519 verification is a different boundary. `-RequirePublisherSignature` supports a future real-certificate/timestamp gate; no self-signed substitute or completed signed release is claimed. ASLR/high-entropy ASLR/DEP and stripped symbols are defense in depth, not anti-copy protection. No UPX packer was added: it cannot prevent copying and can impair antivirus/compatibility. The old published 0.4.0 worker still requires manual upgrade.

### Publication and credentials

Only an explicit allowlist is staged into `dist/pages`; source README/tests/video authoring code are outside publication. The prior exposed README/test material was informational, with no secrets found in the reviewed material. Source responses persisted in an inner cache after purge, so the owner applied a narrow WAF block for the exact retired paths on primary/legacy hosts and removed four old Pages deployments. Retired paths return 403; fresh missing paths return the script-free custom 404. Current content-hashed media and direct update endpoints remain available. See [security summary](SECURITY-REVIEW.md) for precise evidence/attribution rather than interpreting old source-review pending notes as current exposure.

Cloudflare deployment authority and offline release signing are separate. No global credential/key/email belongs in repo/site/binary/reports. Operator credential rotation to least-privilege tokens remains recommended after the work; this document does not claim rotation occurred. Private Ed25519 material is external/protected and was not read for documentation or copied to the guest. A compromised authorized signer can authorize harmful bytes; verification does not certify benign content.

## Monitor, privacy and retained data

Secblitz has no telemetry and sends no usage data. It uses the internet only for: automatic updates (optional at install); web protection block lists and DNS forwarding, only when web protection is turned on, with a Quad9 fallback when the network has no DNS server; and things the user asks for, such as installing Bitwarden with winget, opening the Microsoft Store, or following GitHub links. The privacy policy is at https://secblitz.lol/privacy.html.

The LocalService monitor remains read-only on startup/about every 15 minutes; it does not repair, open administrator WALs or expose remediation IPC. It now collects readiness once and emits typed optional firewall metadata plus separate root readiness. Unknown is an unavailable completed probe, not automatically incomplete/healthy; null means not attempted. Readiness counts toward the existing between-call budget, but synchronous native calls are not preempted. The live report was **9,195 bytes**, with 18 observations/19 findings and Unknown journal-volume readiness, below the unchanged 64 KiB limit. Its in-place report write is not made atomic by the separate updater fix; other LocalService processes can affect it.

No existing passwords, hashes, browser cookies/vaults, tokens, Wi-Fi keys, passkey private material, BitLocker recovery secrets or LSASS content are read. Automatic logon reads only its nonsecret flag/password-value-name presence; no LSA-secret inspection. WDigest changes only a fixed DWORD, not live credential contents.

Machine identity, SIDs, DACL originals, paths, installed-provider metadata and errors are **sensitive operational data**, though not credentials. Keep originals local/protected; redact shared evidence. No arbitrary reports belong in the strict journal root. There is no report-upload/telemetry integration; the updater, user-approved tools and Windows components have independent network behavior. Research keys are not application configuration and must not enter source, logs or docs.

## Evidence and open limits

- Prior 0.4.3 security acceptance: **116 native library + 52 CLI**, nine SYSTEM cases. Native adversarial history includes 53 standard-token checks (49 expected denials/four positive controls), real worker rejection of wrong signatures/keys/corrupt payload/old-image replay, and no confirmed bypass in tested cases. These are bounded, version-attributed results, not exhaustive proof.
- 0.5.0 functional acceptance: **138 library + 66 CLI**, separately invoked readiness smoke and nine SYSTEM updater cases; ignored probes are explicitly excluded. Exact two-control approved apply/undo, inherited Block zero-WAL baseline, automatic post-check including failure, typed LocalService reporting and installer lifecycle passed. Final copy-only follow-up reran CLI tests without claiming repeated core mutation fixtures.
- Live genuine published **0.4.3 to 0.5.0**: Installed **18.865 s**, UpToDate **1.336 s**, DeferredBusy **1.005 s** with the real guide responsive. Running monitor resumed as LocalService/Auto and emitted a fresh typed 9,195-byte report.
- Protected floor advanced from authentic 0.4.3 to **0.5.0**, installer hash matching signed metadata, publication **1791022530**, expiration **1798798530**, and exact SYSTEM/Admin-only ACL. Busy/current checks did not change its bytes. No live corruption/replay injection was used; prior SYSTEM fixtures supply that evidence.
- Live audit remained 18 results/19 findings plus four readiness signals, all inherited inbound settings compliant with zero candidates. All 18 baselines, original file/WAL hashes and eight real journal copies remained unchanged through update/audit/UI/uninstall. No repair, password generation or secret reading occurred in this live phase.
- Only the isolated Windows 11 Enterprise Evaluation build 26200.9457 UI clone was used; the user's VM was untouched. Final cleanup restored original directories/network, removed task/service/app/test residue and shut down the clone normally.
- Historical guide selected-batch/undo, Defender action and v0.2.0 registry/wuauserv tests retain their original scope. BITS native repair was skipped on a flagged baseline.

Still unvalidated: Windows 10, broad Home/Pro/hardware/accessibility coverage, complete original-standard-user broker/UAC, eligible native BITS repair, post-reboot WDigest effects, hardware power-loss durability, actual publisher signing/native-MSVC provenance and broad prevention efficacy. No bypass found in tested cases does not mean unbreakable. The [roadmap](ROADMAP.md) marks completed first-batch features and security fixes while keeping DISM/SFC, backups and broad app updates proposed. Documentation refresh adds no runtime feature and requires no redeploy.
