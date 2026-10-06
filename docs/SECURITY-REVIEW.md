# Security review and release closure: Secblitz 0.5.0

**2026-10-03. Current release is live at [secblitz.lol](https://secblitz.lol/).** This summary consolidates source reviews, native adversarial evidence, the owner's deployment closure and genuine published 0.4.3 to 0.5.0 LIVE E2E PASS. It performs no new audit, credential access, deployment or guest operation.

**Verdict: no acceptance/authority bypass was found in the tested cases. Confirmed scoped defects were fixed and verified.** This is not a claim of exhaustive security, an unbreakable product, a malware-free PC or completed publisher certification. Historical review-time blockers are superseded only where the later evidence explicitly closes them.

## Unreleased 0.6.0 peer-review follow-up

Multiple implementation and independent agent-review passes added and corrected journal recovery, maintenance/patching supervision and delivery. This is **agent peer review, not the requested external security assessment**. The published-release verdict above does not automatically extend to all candidate functionality.

Fixed candidate findings include missing reciprocal durable interlocks, forgotten installer uncertainty, descendants outliving locks, incomplete module-payload pinning, changing bundle/EULA metadata, ambiguous update-source coercion, and a release-tool weak-key signature discrepancy. The Rust client already rejected that weak-key case. Native diagnostics also exposed and fixed LocalAccounts manifest resolution and numeric storage-health serialization.

UI review found a resize/Enter race that could credit unpainted consent text, incorrect nested Back behavior, console-mode capture after a mutating color probe, and missing native Windows PgUp/PgDn mapping. Fixes record review only after successful painting, require matching rendered geometry, retain parent menus, capture original modes first and map page keys directly. Host regression/PTY tests pass; native follow-up must be attributed to its final binary record.

The native follow-up at `target/windows-v060-ui-final/` passed 98 CLI tests and three explicit ConPTY fixtures, including real page keys, nested navigation, resize-before-approval and exact stdin/stdout/stderr mode restoration. One final resize round-trip cache fix then passed its focused host regression and Windows compile/release build; no additional native run or review is claimed for that last patch. [Roadmap evidence](ROADMAP.md#candidate-evidence) records its distinct artifact hash.

The prior full frozen candidate passed 247 native library and 92 native CLI tests, plus targeted trust/process fixtures, with all 18 settings and journal bytes/ACLs preserved. Native Windows Update installation, split-token/over-the-shoulder cases, DISM remediation, Home/Pro/Windows 10 and new live delivery still need acceptance. Current app-upgrade execution is deliberately unavailable rather than offering an obsolete target. See [candidate scope](ROADMAP.md#060-development-candidate) and `target/windows-v060-candidate-validation/` for attribution and remaining work.

## Current release and acceptance

The final installer is **3,850,954 bytes**. Exact current hashes are in dist/SHA256SUMS and the native artifact record. The executable hash is `036d8b69367cb7422ca5d6e821e73749f1c36ce35df437ac47b7d83ba829a6d9`; installer hash is `c17a543fccbb0ec1c37c487aeb8da2e7bfd8a832e04996f6ead32e6dd2b77f3b`.

| Evidence layer | Observed result | Scope |
| --- | --- | --- |
| 0.4.3 security acceptance | 116 library + 52 CLI tests; nine elevated SYSTEM cases | Release-floor/atomic-state fixes and existing update boundaries |
| 0.5.0 functional acceptance | 138 library + 66 CLI tests; separate native readiness smoke; nine SYSTEM updater cases | Typed evidence/readiness, approved batch, post-verification, service/installer behavior |
| Final copy-only follow-up | All 66 CLI tests rerun, including 13 localization checks | Core/library bytes unchanged; no invented repeat of mutation fixtures |
| Published SYSTEM upgrade | Genuine 0.4.3 to 0.5.0 `Installed` in **18.865 s** | No spoofed starting version, manual upgrade or signature bypass |
| Current and busy checks | `UpToDate` **1.336 s**, `DeferredBusy` **1.005 s** | Same real guide PID responsive to Down-arrow/Esc, no forced close |
| Resumed monitor | LocalService, Auto/Running, fresh **9,195-byte** complete report | 18 observations, 19 findings, separate typed readiness; size is not RSS |
| Preservation | All 18 baseline controls and original/eight copied WAL hashes unchanged | Live phase performed no repairs or credential access |

The floor advanced from authentic retained 0.4.3 state to **0.5.0**, matching signed installer hash, published_at **1791022530** and expires_at **1798798530**. Its protected owner/DACL allowed SYSTEM/Administrators only and remained byte-identical after the busy check. Live testing did not inject destructive floor/corruption fixtures; preceding SYSTEM regressions are that evidence.

Only a disposable Windows 11 VM, Windows 11 Enterprise Evaluation build 26200.9457, was operated on. Original directories/network were restored and app/task/service/process residue removed. The user's VM was untouched. Ordinary-suite ignored cases are not counted as passes: native readiness and nine SYSTEM cases were separately exercised; the obsolete public-feed probe and two attended unit probes remained ignored while actual UI acceptance was conducted separately.

## Fixed cases and deployment closure

| Case | Verified correction | Remaining boundary |
| --- | --- | --- |
| Public operational README/test source | Publication now stages only an explicit allowlist into **dist/pages**, not website source. Documentation/tests/video authoring stay in nonpublished docs/scripts. | Reviewed exposed README/tests were informational; no secrets found there. This is not exhaustive secret-leak proof. |
| Retired responses persisted after purge | Owner observed inner-cache retention, added narrow exact-retired-path WAF blocks on primary/legacy apexes and removed **four** old source-bearing Pages deployments. | Only retired paths are blocked, not updater/download routes or arbitrary content. |
| Misleading missing-route fallback | Script-free custom 404 for fresh missing paths; retired blocked paths return **403**, not 404. | Do not mistake these expected differences for failed remediation. |
| Replay of a lower release after a higher signed observation | Protected **highest-seen release floor**, strict immutable same-version hash/target and nondecreasing timestamps, re-read by worker before acceptance. | Cannot remember releases never observed, survive malicious privileged erasure or defeat a compromised authorized signer. |
| Unlink-first updater persistence | Exclusive protected temp, write/flush, destination validation and **MoveFileExW replacement/write-through**. Failure retains prior state; no cross-volume fallback. | Not transactional installer rollback or hardware power-loss certification; stranded protected temps are not trusted/adopted. |
| Unmaintained transitive dependency | indicatif upgraded to **0.18.6**, removing number_prefix; latest coordinating Cargo audit: **214 dependencies, zero vulnerabilities, zero warnings**. | Time-bound advisory result, not proof all code/toolchains are vulnerability-free. |
| Inherited firewall Block offered as a repair | Typed raw/effective/authority contract recognizes local NotConfigured + ActiveStore Block as protected, with zero setter/WAL. | Contradictory/missing/nonlocal evidence remains reviewable; no rule/exposure audit claim. |
| Batch approval and verification ambiguity | Exact recommended preview defaults Back; Change selection replaces unchecked subset; one Enter applies. Every attempt audits once before output. | New candidates need new consent; verified current state cannot erase an earlier operation failure. |
| Readiness misclassification/activation risks | Strict Known/Unknown/native types, reparse rejection and high-integrity machine-pinned COM property-only query. | Readiness is not authorization, disk health, online WUA search or universal two-second operation deadline. |

### Public-route and media details

The owner's confirmed retired set is the source README route (normalized `/readme.md`, previously `/README.md`), `/test_video.py`, `/assets/secblitz-demo.mp4` and `/assets/poster.webp`. Narrow primary/legacy blocks return 403 without the old source responses. Fresh unknown paths return the custom script-free 404. The WAF closure and old-deployment removals were owner operations; this documentation refresh did not repeat them or access Cloudflare.

Current media is `/assets/intro-6bb434a9c067.mp4` and `/assets/preview-33b342ab21fb.webp`, with unchanged verified media bytes. It is actual **0.3.1 Remotion/Windows recording**, muted/looping with no captions or playback controls, not a current 0.5.0 scan. Source, test code and detailed delivery provenance remain outside the published allowlist. Legal font material is preserved, not rewritten.

Primary `https://secblitz.lol` remains canonical. Legacy root and primary www redirects are fixed/safe; legacy feed/download endpoints remain direct for installed clients. Mail configuration was not changed. The existing static CSP, no raw HTML injection sink, fixed checksum copy and signed-feed integrity checks supplied positive review evidence; earlier browser renderer crashes limited that specific review's playback/framing coverage and are not relabeled as passes.

## Adversarial evidence, with honest attribution

[Native security review](security-review-native.md) used a real ordinary-user logon/impersonation token: **53 checks, 49 expected denials and four positive controls**. Protected app/data/staging/task-file modification, deletion, owner/DACL changes and protected-directory link creation were denied. Actual task-descriptor AccessCheck allowed read/run but denied modification: requesting its fixed signed action is not permission to choose SYSTEM code. **Alternate-user Scheduler RPC and the complete original-user UAC/broker flow were not successfully established**, even though token/access checks passed.

Real worker tests rejected wrong signature, wrong key, corrupted authenticated payload and old-worker/current-image replay, with **zero installer starts in those rejection fixtures**. Seven earlier SYSTEM pinning/namespace/locking cases passed; the later nine-case suite adds atomic-write/floor failures. Engine wrong-type/unknown-name/hardlink guards remain enforced despite updater namespace compatibility.

[Protocol review](security-review-update-protocol.md) passed **24 offline groups** and bounded thousand-case mutation/streaming campaigns covering signed-byte binding, parsing, arithmetic, paths, time, replay, signatures and framing. No tested acceptance bypass or panic was found. Local fixture-signed arbitrary bytes can satisfy integrity checks: that correctly demonstrates **trust in the signer**, not a malware scanner or protection from signing-key compromise.

The [release review](security-review-release.md) added explicit staging, path/type/link checks, output preservation, signer input hygiene and atomic signed-feed output. Production signing material remains external, owner-protected and exact POSIX mode 0600 where applicable; that is not a substitute for Windows ACL validation. The docs task did not read private signing material.

## Known boundaries, not claimed fixes

- **Unsigned initial bootstrap:** the user confirmed neither a certificate nor signing service was available, so the preview stays explicitly Authenticode-unsigned. A same-site executable and checksum do not independently authenticate the publisher after hosting compromise. Genuine installed clients verify a pinned Ed25519 key, but that cannot bootstrap trust in a replaced first download.
- **Real publisher gate is ready, not fulfilled:** `-RequirePublisherSignature` requires an authorized real certificate, expected signer/timestamp and verification. No self-signed trust substitute was created. Native ASLR/high-entropy ASLR/DEP, relocations and stripped-symbol checks provide defense in depth, not certification or anti-copy protection.
- **No UPX or anti-copy claim:** packing cannot prevent copying/reverse engineering and can hurt compatibility/antivirus reputation. No packer was added; any future byte change must precede signing/hashing and be retested.
- **Time, signer and host trust:** TLS/no redirects, pinned Ed25519, exact hash/size, local protected floor and atomic persistence are implemented. Root-key rotation, thresholds and independent trusted time are not full TUF. Operators must renew metadata before the **90-day** interval expires using unchanged bytes and valid monotonic timestamps. A signer-compromised installer can still be authorized. The installer worker waits with locks/payload pin, not a 15-minute forced kill.
- **Recovery:** WALs still fail closed when torn/corrupt and can make Undo unavailable; never delete a bad WAL to force progress. Concurrent external writes can race checks. Updates, scans, future servicing and user files are not universally reversible.
- **Coverage:** Windows 10, broader Home/Pro/hardware, full ordinary-user broker, native eligible BITS repair and broad prevention are not certified. No extra antivirus, SFC/DISM or cleanup engine was added by 0.5.0.
- **Credentials:** no hosting credential, account email or private key value is written in this summary or used as application configuration. Reviewed material and targeted staging scans found no secrets; marker scans are not proof against every encoded/unlabelled secret. The operator should rotate the temporary broad Cloudflare credential to least-privilege scoped tokens after the work. **No rotation is claimed.** Hosting and signing authority should remain separate; documentation/public verification needs no secret access.

## Evidence map and next work

The authoritative closure is windows-v050-results.md. Prior security deployment evidence is windows-v043-results.md and [security-adversarial.md](security-adversarial.md). Independent review histories: [website](security-review-website.md), [release](security-review-release.md), [dependencies](security-review-dependencies.md), [protocol](security-review-update-protocol.md), [native](security-review-native.md), and [readiness](readiness-security-review.md). Their earlier pending statements retain review-time meaning; the later native/live result resolves only the cases it actually retested.

[ROADMAP.md](ROADMAP.md) now marks the reviewed-plan/effective-protection/post-check/readiness tranche and security floor/atomic persistence as implemented. Next proposals are Windows integrity diagnosis through a separate operation engine, backup/recovery readiness and trusted exact app upgrades. Those proposals are not public promises or current functionality. **No bypass found in tested cases is the bounded conclusion, not “all P0 risks gone” or “unbreakable.”**
