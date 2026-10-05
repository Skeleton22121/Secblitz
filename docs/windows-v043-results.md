# Windows 0.4.3 security patch: LIVE E2E PASS

## Security deployment gate: PASS

The genuine published **0.4.2 installation upgraded to published 0.4.3 through
the actual owned SYSTEM task**. Installed image/version verification, current
checking, protected release-floor persistence, valid status JSON, native audit,
real-guide deferral and final preservation all passed on 2026-10-03.

Persistent evidence: **`target/windows-live-v043/`**.
Guest workspace: `C:\Windows\Temp\SecblitzLiveV043Final`.
Only the UI clone was used. No rebuilt fixture, manual upgrade, production feed
change, signing-key access or application/source modification was involved.

### Exact live artifacts

| Artifact | SHA-256 |
| --- | --- |
| Genuine starting 0.4.2 installer | `28c93869508923b2ea865267025dc6c0d9e92e5d343e5a78f175d32e5b1b83c4` |
| Installed starting 0.4.2 EXE | `78c666e456b3f72fbe503e4aad6213b1baec4f7172e3e8cacd17f9bf893b4672` |
| Downloaded published 0.4.3 installer | `871f61695f30a1566bb77627b0c4378c106879110b8c2d19affb8b92e0305605` |
| Installed final 0.4.3 EXE | `7ec84de303d97a537b7d3324511ff63745f1911d2f29660e6ac8a8f436d5471d` |

The published new installer is **3,821,696 bytes**. Starting and final versions
were checked through the actual installed `--version` command. The current
release artifacts still match these hashes after cleanup.

### Runtime results

| Check | Observed result | Duration |
| --- | --- | ---: |
| SYSTEM task, genuine 0.4.2 to 0.4.3 | Installed; exact image/version | 15.821 s |
| Next SYSTEM task, current 0.4.3 | UpToDate; release floor written | 1.321 s |
| Captured current status JSON and both pipe EOFs | Exit 0, valid JSON, no stderr | 65.839 ms |
| Native `audit --json` | 18 results, 19 findings; expected exit 2 | 32.068 s |
| SYSTEM task while the real guide was open | DeferredBusy; same guide PID alive | 0.998 s |
| Captured busy status JSON and both pipe EOFs | Exit 0, valid JSON, no stderr | 71.263 ms |
| Guide scan/menu and exit | Scan completed, menu rendered, normal Esc exit | Passed |
| Baselines, journals and offline cleanup | All preservation checks passed | Passed |

The actual observed process chain was SYSTEM (`S-1-5-18`), session 0:

```text
secblitz.exe update check
  C:\ProgramData\Secblitz\Updates\update-worker.exe update install-staged
    C:\ProgramData\Secblitz\Updates\update-installer.exe
      /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /TASKS= /SECBLITZUPDATE=1
```

The updater used its pinned `https://secblitz.lol` origin and normal signed-feed
verification path. There was no endpoint override, redirect workaround or
signature bypass. DNS evidence was captured after clearing the cache before
checks; no decrypted HTTP capture or Cloudflare control-plane audit is claimed.

Persisted upgrade transitions included:

```json
{"checked_at":1791013094,"result":{"outcome":"worker_started","version":"0.4.3"}}
{"checked_at":1791013105,"result":{"outcome":"installed","version":"0.4.3"}}
```

Desktop selection and `AutoUpdatesEnabled = 1` remained intact. The task DACL
matched its pre-update value exactly. Default installation/update did not add
or start the optional monitor.

### Release-floor proof and normal atomic status behavior

The first new-version check persisted:

```text
C:\ProgramData\Secblitz\Updates\release-floor.json
```

```json
{"schema":1,"version":"0.4.3","sha256":"871f61695f30a1566bb77627b0c4378c106879110b8c2d19affb8b92e0305605","target":"windows-x86_64","published_at":1791012306,"expires_at":1798788306}
```

All identity and timestamp fields matched the production-authenticated staged
manifest. The floor hash binds the **installer**, not the installed EXE.
Its observed security descriptor was:

```text
O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)
```

The owner and protected DACL were checked, with no non-SYSTEM/non-Administrator
grantees. The file SHA-256 before and after the busy-session check was identical:

```text
f91796311c0bc734cdff07e8e2a78f2fc175b0601e86e6072180e7acad64e2e5
```

The floor therefore persisted unchanged across separate processes, audit/guide
use and the busy check. Status remained valid JSON after both normal current and
busy operations, with the expected outcomes and prompt pipe closure. The live
floor was only read by the validation harness; no lower-version feed, floor
rewrite or signing-key fixture was introduced. Lower-release rejection and
interrupted/failed-write preservation remain supported by the nine previously
passed SYSTEM tests described below, not by a destructive live fault campaign.

The actual guide completed its read-only scan and rendered the arrow menu.
During the task invocation, the same guide process, PID 8672, remained alive.
Esc then returned normally to the command prompt. No process was force-closed
and no hardening operation was selected.

### Final cleanup and evidence

The test instance was uninstalled normally. All **18 independently captured
controls**, all original journal/file hashes and all eight copied transaction
journals remained unchanged. Original app/data directories were restored. No
installed EXE, shortcut, task, monitor service or app/worker/installer process
remained. The original static lab IP/DNS configuration was restored; the clone
was shut down normally with **NIC1/NIC2 `none`**. The original user VM was never
operated on. No credentials or private signing material appear in the evidence.

Key evidence under `target/windows-live-v043/`:

- `upgrade-results/`: signed manifest, before/after versions, exact hashes,
  SYSTEM process chain, status transitions, DNS evidence and upgrade timing.
- `post-checks/`: current/busy JSON, task XML/ACL, audit output, release-floor
  content/ACL/hash comparisons and status timing.
- `cleanup-evidence/final-state.json`: baseline, original WAL and journal-copy
  preservation with zero installation/process residue.
- `guide-menu-ready.png` and `guide-exit.png`: real guide scan/menu and normal
  exit after the updater deferred.

This completes the requested Windows security deployment gate. Windows 10,
the broader alternate-user RPC matrix and website control-plane security are
outside this run. The published 0.4.0 self-recovery limitation still requires
manual upgrade; success from genuine 0.4.2 does not change that fact.

## Prior basic acceptance and build provenance

The persistent build toolchain is restored. Host checks, Windows compilation,
the corrected native suites, all nine elevated updater tests and the real
installer lifecycle pass. **The exact tested 0.4.3 artifacts are promoted into
`dist` and ready for signing/publication.** No signing, publication or live
0.4.3 feed test was performed by this validation phase.

Evidence and candidate artifacts: **`target/windows-validation-v043/`**.
Guest workspace: `C:\Windows\Temp\SecblitzV043OfflineA`.
Only `Secblitz-W11-UI-Test`, UUID
`4b70288b-b64d-4796-a725-006da3162d0f`, was used. The original user VM was untouched.

## Native loopback blocker resolved

The protocol owner fixed the accepted socket's nonblocking mode. The exact
previously failing test now passes, followed by the complete native library
and CLI suites:

- Focused loopback case: **1 passed**, zero failed, 0.06 seconds.
- Full native library: **116 passed**, zero failed, 10 ignored.
- Full native CLI: **52 passed**, zero failed, two attended probes ignored.

The ten library ignores comprise the nine SYSTEM tests already passed in the
preceding phase and the deliberately opt-in live 0.4.2 feed probe. The live probe
was not enabled. No ignored case was reported as a failure or a new pass.

Current release EXE bytes exactly match the earlier installer-tested candidate.
The product source, installer source, Cargo manifest and lockfile also match the
frozen inputs. The test-only fix therefore retains the previous nine SYSTEM
tests and full installer lifecycle evidence without rebuilding the release.
New test hashes and checks are recorded in `retest-provenance.json`; outputs
and the independent preservation checks are in `retest-results/`.

Historical failure, retained for provenance:

```text
updater::tests::loopback_http_chunking_and_false_lengths_cannot_bypass_body_checks
src/updater/tests.rs:634
socket.read_exact(&mut byte).unwrap()

Os { code: 10035, kind: WouldBlock,
     message: "A non-blocking socket operation could not be completed immediately." }
```

The helper had set its listener nonblocking without resetting the accepted
socket before timed reads. Windows inherited that mode. The owner added
`socket.set_nonblocking(false)` before the timed I/O. The validator did not edit
application or reviewer-owned test source.

This was a native test-portability issue, not demonstrated acceptance of
malformed production payloads. The initial failure remains in
`native-results/native-lib.out`; the successful retest supersedes that gate.

## Exact promoted artifacts

| Promoted file | SHA-256 |
| --- | --- |
| `dist/secblitz.exe` | `7ec84de303d97a537b7d3324511ff63745f1911d2f29660e6ac8a8f436d5471d` |
| `dist/secblitz-0.4.3-windows-x64-setup.exe` | `871f61695f30a1566bb77627b0c4378c106879110b8c2d19affb8b92e0305605` |

Installer size: **3,821,696 bytes**. These are the exact guest-tested bytes;
guest, staged host and promoted SHA-256 values match. Both `dist/SHA256SUMS`
entries passed verification. The previous 0.4.2 EXE, installer and checksums are
preserved in `dist/archive/0.4.2/`; all 15 previously archived files were verified
unchanged. `promotion.json` records the handoff.

## Build restoration and dependency update

Use:

```sh
source target/build-tools/cross-env.sh
```

The existing persistent Rust 1.93.0/Windows GNU std and MinGW cache under
`target/tools/` was reused via stable links under `target/build-tools/`.
The official rustup initializer was downloaded and SHA-256 checked, installed
without modifying shell profiles, and used to add the matching Clippy component.
`CARGO_HOME` is `target/build-tools/cargo`; default output is
`target/windows-release`; temporary compiler files use `target/compiler-tmp`.
No root packages or global Rust configuration were changed.

The initial `--locked` check exposed a stale Cargo.lock. A targeted
`cargo update -p indicatif` regenerated only the required lockfile changes:

- Root package 0.4.2 to **0.4.3**.
- indicatif 0.17.11 to **0.18.6**.
- Removed **number_prefix 0.4.0**.
- Added transitive console 0.16.6 and unit-prefix 0.5.2.

The direct console dependency was not rewritten. No UI compatibility patch was
needed. The original lockfile, update output and package delta are retained as
`Cargo.lock.before`, `lock-refresh.out` and `lock-changes.json` in the evidence
directory. This proves removal of the identified dependency; a new full advisory
database audit was not part of this execution.

The persistent guest helper is `target/windows-validation-tools/guest.py`, pinned
to the UI clone UUID and using the existing password file directly through
VirtualBox. It never reads or prints the credential. See
[VM readiness](vm-status.md) for exact persistent paths and commands.

## Executed checks

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| Full host `cargo test --locked` | 107 library + 52 CLI passed; one live probe and two attended probes ignored; doc-tests passed with zero cases |
| Offline updater protocol tests within host suite | 24 passed |
| Host Clippy, all targets, warnings denied | Passed |
| Windows Clippy, all targets, warnings denied | Passed |
| Windows GNU release and test builds | Passed |
| Ordinary native Windows library suite, after owner test fix | **116 passed, 0 failed, 10 ignored** |
| Native Windows CLI suite | **52 passed**, two attended probes ignored |
| Explicit elevated updater suite | **9 passed** as SYSTEM/session 0 |
| Production ownership XML fixtures | 3 accepted, 21 rejected |
| Actual Inno 0.4.3 compilation | Passed |
| Full permitted installer lifecycle | Passed |
| Actual Inno upgrade with eight-variable sanitized environment as SYSTEM | Passed |

The ignored native library cases are nine elevated tests plus the explicitly
opt-in live 0.4.2 probe. Only the elevated tests were subsequently enabled. The
live probe remained ignored, as the 0.4.3 feed had not been published.

The initial library failure was reported first. Independent CLI, elevated and
installer checks then passed. Once the owner supplied the test-only fix, the
focused case and full native suites were rerun successfully before promotion.

## Security regressions verified under SYSTEM

The nine elevated cases include the existing clean environment, protected
layout/shared engine lock, payload/directory pins, link rejection, replacement
and duplicate-worker exclusion, plus both new persistence regressions:

### Atomic status replacement

`failed_or_interrupted_atomic_status_write_preserves_previous_json` passed:

- An injected partial write failure preserved the exact previous JSON bytes.
- The failed invocation's temporary file was removed.
- A separately stranded, partial protected temporary file did not become status.
- Successful replacement did not remove another invocation's temporary file.

This is fault injection and stranded-temp simulation, not a physical power-loss
campaign. Native payload-pin replacement protection also continued to pass.

### Protected release floor

`protected_floor_is_durable_and_write_or_parse_failure_stops_advancement` passed:

- An observed higher version remained stored after payload verification failed.
- A lower version was refused afterward.
- Pinning the floor against replacement prevented advancement and preserved the
  prior value.
- Releasing that pin allowed the valid higher advancement.
- Corrupt floor JSON caused failure and was not silently reset.

The protocol cases covering restart, failed download, clock rollback, immutable
version/hash binding, metadata rollback and corrupt-floor independence from
engine audit also passed outside the failing loopback framing case. Native
floor fixtures were confined to disposable protected test directories, not the
original journal state.

## Installer and preservation

The actual installer passed default hourly SYSTEM task registration with delayed
first run, task ACL checks, desktop on/off and updater preference retention,
automatic-update flags, nine safe foreign-task rejection variants, monitor
report/start/upgrade/resume, quiet uninstall rejection/retry and final uninstall
with report/journal/unrelated-file preservation. The prohibited writable-task-
ACL fixture remains excluded. Offline installed check/status failures and
untrusted worker-path rejection behaved as expected.

`lifecycle-results/final-state.json` confirms the lifecycle PASS and exact
preservation of all **18 controls and original journal hashes**. Original app
and data directories were restored. `handoff.json` independently confirms no
installed app, shortcut, updater task, monitor service or Secblitz process.

The final retest independently reconfirmed all 18 controls and original journals
unchanged, with no installed app, task, service or Secblitz process. The UI clone
was shut down normally with **NIC1/NIC2 `none`**. No signing key, production
website configuration or original user VM was accessed.

The later genuine installed 0.4.2 to 0.4.3 signed-feed test is now complete and
passed, as recorded at the top of this document. The preceding sections retain
the build and basic-acceptance provenance for those exact release bytes.
