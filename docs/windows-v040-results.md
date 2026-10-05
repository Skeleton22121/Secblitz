# Windows updater validation - historical 0.4.x acceptance log

**Final deployment gate: [0.4.2 LIVE E2E PASS](windows-v042-results.md).**
The genuine published 0.4.1 installer upgraded through the actual SYSTEM task
to 0.4.2; subsequent current checking, native audit, real-guide opening and
`DeferredBusy` all passed. The detailed results below are chronological evidence
of earlier blockers and fixes, not the current release verdict.

## 0.4.2 namespace/domain release: Windows basic acceptance PASS

The new build was taken from
`target/windows-release/x86_64-pc-windows-gnu/`, not the superseded temporary
cross-target directory. Exact inputs, guest results and promotion evidence are
stored persistently under **`target/windows-validation-v042/`**.
Guest workspace: `C:\Windows\Temp\SecblitzV042OfflineA`.

### Exact artifacts ready for signing/publication

| File | SHA-256 |
| --- | --- |
| `dist/secblitz.exe` (0.4.2) | `78c666e456b3f72fbe503e4aad6213b1baec4f7172e3e8cacd17f9bf893b4672` |
| `dist/secblitz-0.4.2-windows-x64-setup.exe` | `28c93869508923b2ea865267025dc6c0d9e92e5d343e5a78f175d32e5b1b83c4` |

Installer size: **3,813,017 bytes**. This is the exact guest-compiled and tested
installer; guest/host hashes matched and `dist/SHA256SUMS` verified both promoted
files. Previous 0.4.1 EXE, installer and checksums are preserved in
`dist/archive/0.4.1/`. All 12 existing older archive files were verified unchanged.

### Executed checks

- **106 native library tests passed**, with seven elevated tests initially
  ignored; **52 native CLI tests passed**, two attended menu probes ignored.
- All **seven elevated updater tests passed as SYSTEM (`S-1-5-18`), session 0**.
  This includes `update_directory_is_protected_reopenable_and_engine_lock_stays_shared`,
  sanitized child known-folder/preflight-status resolution, directory/payload
  pinning, staged replacement, hardlink/reparse rejection and worker locking.
- New engine compatibility tests passed:
  `updater_reserved_entries_coexist_with_exact_journal_roundtrip`,
  `updater_exceptions_reject_wrong_types_unknown_names_and_corrupt_wal`, and
  `updater_reserved_files_reject_hardlinks`.
- Native layout validation requires the exact `Updates` child and still points
  updater/engine contention at the base `engine.lock`.
- Actual Inno 0.4.2 compilation, SYSTEM upgrade with the eight trusted environment
  variables (working/temp directory under `Secblitz\Updates`), and the full
  permitted installer lifecycle passed.
- Lifecycle covers real hourly/delayed SYSTEM task/ACL, desktop on/off and
  updater preference retention, automatic-update flags, nine safe foreign-task
  variants, monitor report/start/upgrade/resume, uninstall rejection/retry and
  report/journal/unrelated-file preservation. The prohibited writable-task-ACL
  fixture remains excluded.
- Pure ownership XML fixtures: three accepted and 21 rejected.
- Installed offline check/status failure semantics and untrusted-worker rejection
  passed; no request to the still-0.4.1 live feed was made during this phase.

Frozen updater source hash:
`7f089cb3187d7ff3bf0f8213940af9e0f36e1d03ada94187c0b5ee868f59e994`.
Frozen engine source hash:
`79b4418ca49478eef06cdb710d7cc19a750606adbce65d0d5c4ccb664035ec86`.
The build embeds the new `https://secblitz.lol` origin. All input hashes,
including public-key/origin assets and native test executables, are recorded in
`frozen-inputs.json`. No source changes were made by validation.

### Restored state and next live phase

`lifecycle-results/final-state.json` and `handoff.json` confirm PASS, all **18
controls and original journal hashes unchanged**, original directories restored,
and no installed app, task, service, shortcut or Secblitz process left behind.
Temporary SYSTEM probe tasks were removed. The UI clone remains offline with
NIC1/NIC2 `none`; the original user VM was untouched.

Live 0.4.2 E2E awaits main's publication. The next starting point will be the
**genuine published 0.4.1 installer**, already preserved and hash-verified at:

```text
dist/archive/0.4.1/secblitz-0.4.1-windows-x64-setup.exe
SHA-256: 9713aff6c0f9d2029e8a0ee4b9c8c7013abd856a3f26717acb36cc6cab17bbcc
```

No rebuilt 0.3.99 fixture will be used for that run. It must validate the actual
old `beacons.lol` feed path, upgrade to 0.4.2 with legacy files retained, then
successful real-guide opening/`DeferredBusy` and current-version checking on the
new origin. Those live results are **not yet claimed**. No publication or signing
was performed in this phase.

## Latest live run: automatic installation PASS; full E2E BLOCKED by journal namespace collision

The corrected older fixture successfully updated through the **actual SYSTEM
scheduled task** and the production signed feed to **0.4.1**. A subsequent
scheduled check returned **UpToDate**. However, the real guide cannot open after
the update, so **do not record an unqualified LIVE E2E PASS**: real-guide
`DeferredBusy` remains blocked by the concrete failure below.

Evidence: `/tmp/opencode/secblitz-live-v041/`.
Guest workspace: `C:\Windows\Temp\SecblitzLiveV041A`.
Only `Secblitz-W11-UI-Test` was operated on; its original static lab network was
temporarily changed to NAT/DHCP and restored after testing.

### Exact versions, bytes and durations

| Item | Version / result | SHA-256 |
| --- | --- | --- |
| Installed starting fixture, rebuilt from corrected 0.4.1 source | 0.3.99 | `04780a49e9f983bbb896b601045736deede79d0ff42db7fb6bb4d6b135b4c0f8` |
| Production signed installer downloaded by updater | 0.4.1, 3,811,505 bytes | `9713aff6c0f9d2029e8a0ee4b9c8c7013abd856a3f26717acb36cc6cab17bbcc` |
| Installed executable after automatic update | 0.4.1 | `f686dbf3f40289a5e2153178ccf96c9c8eee7a6fd230f6b636f5f35ae6b6f253` |

- Trigger-to-installed verification: **15.122 seconds**.
- Subsequent SYSTEM scheduled check to `up_to_date`: **0.815 seconds**.
- Captured `update status --json` process **and stdout/stderr EOF**:
  **84.063 milliseconds**, exit 0, no stderr, valid `up_to_date` status JSON.
  No descendant kept the output pipes open.

This is explicitly a corrected-core test fixture, **not evidence that the
published broken 0.4.0 worker can upgrade itself**. Its corrected updater source
hash is `f2dd7de7b83a79591cff4867b14ad0ef0e2d02eb32e1106726173ff8c872018b`;
the fixture's manifest/lockfile version alone was changed in the external copy.

### Successful production execution

The fixture was installed through a real Inno 0.3.99 package built from the
accepted installer source. `--version` confirmed 0.3.99 before triggering
`Start-ScheduledTask SecblitzUpdate`. The updater used its compiled origin
`https://beacons.lol`, requesting `/releases/stable.json` and the signed
`/downloads/secblitz-0.4.1-windows-x64-setup.exe` package through its normal
verification path, with no endpoint override, signature bypass or manual
installer invocation in the upgrade.

Observed process chain, all **SYSTEM (`S-1-5-18`), session 0**:

```text
secblitz.exe update check
  update-worker.exe update install-staged
    update-installer.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /TASKS= /SECBLITZUPDATE=1
```

Persisted transitions:

```json
{"checked_at":1790997809,"result":{"outcome":"worker_started","version":"0.4.1"}}
{"checked_at":1790997819,"result":{"outcome":"installed","version":"0.4.1"}}
```

The installed `--version` result was `secblitz 0.4.1`; both installed and
downloaded hashes matched the immutable published artifacts. The desktop
shortcut remained present, `AutoUpdatesEnabled` remained 1, and task ACL matched
the pre-update ACL exactly. The next owned-task invocation returned `up_to_date`.

See `upgrade-results/observed-processes.jsonl`, `status-transitions.jsonl`,
`installed-status.json`, `upgrade-result.json`, and
`post-checks/up_to_date-result.json`.

### New blocker: updater files prevent the guide from opening

Launching the actual installed application in a real interactive Windows
terminal with `--lang en --no-animation --details` exits with:

```text
Operation failed
Details: Unexpected journal entry
```

The failure was reproduced after temporarily moving the test-only sentinel out
of `C:\ProgramData\Secblitz`. At that point the directory contained **only**:

```text
engine.lock
update-installer.exe
update-manifest.json
update-status.json
update-worker.exe
update.lock
```

`src/engine.rs::Engine::load()` (lines 439–456 in the tested source) skips only
`LOCK_NAME`, then requires every remaining entry to end in `.jsonl`. The updater
stores its metadata, lock and payloads in that same directory. Thus normal
updater-owned files fail the guide's transaction-directory validation. This is
not caused by the test sentinel or corrupt user journals.

Screenshot: `guide-without-sentinel.png`; supporting file inventory:
`post-checks/guide-root-without-sentinel.json`. No updater files were removed or
renamed to bypass the issue. The guide exited on its own; no application was
force-closed. An initial direct interactive task launch did not leave a live
guide; the decisive reproduction used the visible Windows terminal.

**Required follow-up:** coordinate a namespace integration fix and rerun the
real-guide busy-session case. Do not claim `DeferredBusy` with a live guide from
this run. No source, release artifact or feed was changed by validation.

### Cleanup PASS

The temporary sentinel was restored and its hash verified; the installed 0.4.1
test application was uninstalled normally. Test tasks and shortcuts were removed,
original app/data directories restored, and original static IPv4/DNS settings
restored. The clone was shut down normally and NIC1/NIC2 restored to `none`.

`cleanup-evidence/final-state.json` verifies all **18 control baselines and
original journal hashes unchanged**, sentinel preserved, no original directory
left moved aside, and no app, updater/guide task, monitor service, shortcut or
Secblitz/worker/installer process left behind. The original user VM and signing
secrets were untouched.

## 0.4.1 recovery release: native SYSTEM environment and installer acceptance PASS

**Release handoff is ready. Live 0.4.1 signed-feed E2E is not yet claimed.**
The production feed was still 0.4.0 during this phase, so the clone remained
offline and the newly corrected older fixture was not installed or run against
that feed.

### Exact promoted artifacts

| File | SHA-256 |
| --- | --- |
| `dist/secblitz.exe` (0.4.1) | `f686dbf3f40289a5e2153178ccf96c9c8eee7a6fd230f6b636f5f35ae6b6f253` |
| `dist/secblitz-0.4.1-windows-x64-setup.exe` | `9713aff6c0f9d2029e8a0ee4b9c8c7013abd856a3f26717acb36cc6cab17bbcc` |

Installer size: **3,811,505 bytes**. These are the exact guest-tested bytes;
guest and host SHA-256 matched and `dist/SHA256SUMS` passed verification.
The old 0.4.0 EXE, installer and checksum file were preserved unchanged in
`dist/archive/0.4.0/`; all nine pre-existing older archive files were verified
unchanged. No signing or publication was performed by this validation phase.

### Executed checks

Evidence: `/tmp/opencode/secblitz-v041-stage/`.
Guest workspace: `C:\Windows\Temp\SecblitzV041OfflineA`.

- **102 native library tests passed**, six elevated tests initially ignored.
- **52 native CLI tests passed**, two attended menu probes ignored.
- **All six elevated updater tests passed under SYSTEM (`S-1-5-18`), session 0.**
  The exact new case is
  `updater::windows::tests::clean_child_environment_resolves_native_update_paths`.
  Its recursively launched child passed with the production sanitized
  environment plus the test-only `SECBLITZ_TEST_KNOWN_FOLDERS_CHILD` recursion
  guard. Native ProgramData and installed-program path resolution succeeded;
  status returned `DeferredBusy` while the parent held the update lock.
- Payload pinning, directory-prefix pinning, hardlink/reparse rejection,
  staged replacement under pins and duplicate-worker lock tests passed under
  the same SYSTEM runner.
- Production task XML fixtures: three valid definitions accepted and 21
  mutations rejected.
- Real Inno Setup 0.4.1 compilation and full permitted installer lifecycle
  passed: actual delayed hourly SYSTEM task/ACL, desktop on/off retention,
  updater preferences, marker/legacy automatic upgrade flags, nine safe
  foreign-task variants, monitor installation/report/upgrade/resume,
  quiet uninstall rejection/retry, final uninstall and data preservation.
- **An additional real Inno upgrade succeeded as SYSTEM/session 0 with exactly
  the eight production child-environment variables**, without inheriting the
  host environment: `SystemRoot`, `WINDIR`, `SystemDrive`, `ProgramData`,
  `ALLUSERSPROFILE`, `PATH`, `TEMP`, `TMP`. Fixed automatic-update flags were
  used, plus a diagnostic log path. Exit code was 0; the updater preference and
  task remained valid afterward. See `lifecycle-results/system-installer.json`
  and `system-installer-inno.log`.
- Offline installed check/status still report structured failures; untrusted
  temporary worker dispatch rejects with exit 1.

`lifecycle-results/final-state.json` confirms PASS, all **18 control baselines
unchanged**, original journals unchanged, original app/data restored, and no
task/service left. `handoff.json` independently confirms no installed EXE,
desktop/start-menu shortcut or Secblitz process. Temporary SYSTEM probe tasks
were removed. Both VM NICs remain `none`; the original user VM was untouched.

Frozen `src/updater/windows.rs` SHA-256:
`f2dd7de7b83a79591cff4867b14ad0ef0e2d02eb32e1106726173ff8c872018b`.
The installer source remained the previously accepted preference-caching build,
SHA-256 `8cb458a776009cb22537632e5726b3fbad3b2e4ea51659d53b29a888d90d3262`.
All executable/test/source input hashes are in `frozen-inputs.json`.

### Corrected older fixture for the next live test

Use this **new** external-copy build, based on corrected 0.4.1 source but with
only the isolated project's package version changed to 0.3.99:

```text
/tmp/opencode/secblitz-v0399-fixed041/target/x86_64-pc-windows-gnu/release/secblitz.exe
SHA-256: 04780a49e9f983bbb896b601045736deede79d0ff42db7fb6bb4d6b135b4c0f8
```

Its origin/key assets match the corrected production source. Offline locked
cross-compilation passed in its own target directory; repository Cargo files
and normal release outputs were not modified. The earlier fixture under
`secblitz-v0399-fixture` contains the broken worker environment and **must not be
used for the corrected live E2E**.

### Recovery limitation

Existing **0.4.0 installations cannot self-heal through the broken automatic
worker**. They require a **manual upgrade to 0.4.1**. Publishing the fixed
installer alone does not repair the environment used by an already installed
0.4.0 check process. The corrected older fixture is for forward-path testing,
not evidence that the shipped 0.4.0 worker can upgrade itself.

The corrected worker now seeds native-derived known-folder environment values
and performs a fixed read-only preflight before recording `WorkerStarted`.
The native tests above validate the environment and locked-status path; a
complete production-fed 0.4.1 installation, subsequent `UpToDate` and real-guide
`DeferredBusy` checks still await main's 0.4.1 publication and the next live run.

## Live E2E: BLOCKED - sanitized worker environment cannot resolve ProgramData

The production-fed automatic upgrade was attempted on the UI clone only.
It **did not install 0.4.0**. Do not interpret the earlier installer acceptance
or the live `worker_started` record as completed updater acceptance.

Evidence root: `/tmp/opencode/secblitz-live-v040/`.
Guest root: `C:\Windows\Temp\SecblitzLiveV040A`.
Pre-live snapshot: `secblitz-pre-live-v040`, UUID
`4029fa21-9520-41e7-8422-71af3aeeec0d`.

### Executed production path

1. Compiled a real **0.3.99** Inno fixture using the final accepted installer
   source and isolated older binary. Installed with normal defaults, including
   the owned updater task. Native `--version` confirmed `secblitz 0.3.99`;
   installed hash was
   `5002623b71effdfb5c20b44000e00429e297939ca5667f51bf84a9b702c0bf18`.
2. Enabled temporary NAT on the clone. The first task attempt correctly failed
   because the guest retained static lab address `10.10.120.80/24` with no
   gateway or DNS. After capturing that configuration, temporary DHCP restored
   connectivity; no product change was made for this network issue.
3. Triggered the actual owned `SecblitzUpdate` task with `Start-ScheduledTask`.
   Process evidence confirmed `"C:\Program Files\Secblitz\secblitz.exe" update
   check` ran as **SYSTEM (`S-1-5-18`), session 0**.
4. The production check reached `worker_started`, version **0.4.0**. It staged
   the signed manifest and exact published installer from the fixed HTTPS origin
   `https://beacons.lol`, through the normal signature/hash/size checks.
5. After a bounded **15-minute** observation, no `installed` outcome appeared.
   The original installed binary remained 0.3.99. A subsequent process-start
   trace confirmed creation of `update-worker.exe`; the check parent exited 0.
   No `update-installer.exe` launch was observed.

Staged evidence:

| File | SHA-256 | Bytes |
| --- | --- | ---: |
| `update-installer.exe` | `8292b3a87bbc30130ffdcb80cfa2cf1fc0bfcb680dfd12ece3000a3fbfaec37d` | 3,810,746 |
| `update-manifest.json` | `81f509a8f5646ad268cfad11b081042d333b34ab3cf1122290e8d7878141b5e4` | 450 |
| `update-worker.exe` | `5002623b71effdfb5c20b44000e00429e297939ca5667f51bf84a9b702c0bf18` | 4,690,944 |

The staged worker byte-matches the installed older fixture. All three staged
objects have the protected SYSTEM/Administrators-only DACL. Invoking the staged
worker with `--version` succeeds, so the binary itself is loadable.

### Exact blocker and diagnostic evidence

A separate diagnostic binary, built only in the temporary fixture project,
called the same public library APIs under SYSTEM. It is outside the fixed
worker path and therefore cannot pass the install-staged identity check or
install anything. Comparing inherited environment against **exactly** the five
variables set by production `child_command()` produced:

```text
Inherited environment:
  elevated: Ok(true)
  state_dir: Ok("C:\\ProgramData\\Secblitz")
  status: Ok(UpdateStatus { ..., result: WorkerStarted { version: "0.4.0" } })
  worker_entry: Err(Installer must run from the fixed protected worker)

Production worker-clean environment:
  elevated: Ok(true)
  state_dir: Err(Cannot resolve ProgramData known folder (0x80070003))
  status: Err(Cannot resolve ProgramData known folder (0x80070003))
  worker_entry: Err(Cannot resolve ProgramData known folder (0x80070003))
```

The clean environment contains only `SystemRoot`, `WINDIR`, `PATH`, `TEMP` and
`TMP`, exactly as `src/updater/windows.rs::child_command()` supplies. The failing
native call is `SHGetKnownFolderPath(FOLDERID_ProgramData)` in
`src/platform/journal.rs::program_data()`. HRESULT **`0x80070003`** is the observed
error; which additional native-derived environment value is required has not
been guessed or patched during this validation.

`install_staged()` calls `update_root()?` before entering its result/failure
recording block. Consequently this early failure leaves the parent's
**`worker_started` status stale**, instead of persisting a worker failure.
Both environment construction and early failure reporting need updater-owner
review. A diagnostic SYSTEM wrapper keeping the check's task parent alive did
not resolve the issue; it was removed during cleanup.

Primary evidence directories:

- `upgrade-retry-results/`: 15-minute bound and `worker_started` transition.
- `worker-evidence/`: staged hashes, protected ACLs, manifest and old installed hash.
- `trace-evidence/`: check-parent and worker creation events.
- `environment-evidence/`: exact inherited-versus-clean library diagnostics.
- `cleanup-evidence/`: successful uninstall and restored-state checks.

No production binaries/source/feed were changed and no version was published.
Current-version `UpToDate`, installed 0.4.0 hash verification and real-guide
`DeferredBusy` checks remain pending because automatic installation never
completed. No GUI was force-closed.

### Cleanup and isolation

The old fixture was uninstalled normally, its owned task and shortcuts removed,
and temporary diagnostic tasks removed. Staged files and logs were retained in
the isolated guest evidence directory. Original application/data directories
were restored; neither is left moved aside. The original static IPv4 address
and empty IPv4 DNS configuration were restored. The clone was shut down
normally and adapter 1 restored to **`none`**; temporary NAT is gone.

`cleanup-evidence/final-state.json` reports:

- **All 18 baseline controls unchanged.**
- **Original journal hashes unchanged; live sentinel preserved.**
- No installed EXE, desktop/start-menu shortcut, updater/diagnostic task,
  monitor service or Secblitz/worker/installer process remains.

The original user VM was never operated on. No private signing key entered the
guest.

## Final result: installer passed and exact artifacts promoted

The desktop preference caching fix passed the full permitted installer
lifecycle on the offline `Secblitz-W11-UI-Test` clone. Original VM untouched.
Evidence: `/tmp/opencode/secblitz-v040-final/`; guest workspace:
`C:\Windows\Temp\SecblitzV040OfflineD`.

### Release handoff

| Promoted file | SHA-256 |
| --- | --- |
| `dist/secblitz.exe` | `40cf5fc714e6dcff15de2186832b37f15f8ec6e4d6bc581a641fd53dd9ad4e16` |
| `dist/secblitz-0.4.0-windows-x64-setup.exe` | `8292b3a87bbc30130ffdcb80cfa2cf1fc0bfcb680dfd12ece3000a3fbfaec37d` |

Installer size: **3,810,746 bytes**. The installer is the exact guest-built,
guest-tested file, not a subsequent rebuild. Guest SHA-256 values matched the
downloaded host files. `dist/SHA256SUMS` was updated and both entries passed
`sha256sum -c SHA256SUMS` after promotion and again after fixture compilation.

Previous v0.3.0 executable, installer and checksums were archived under
`dist/archive/0.3.0/`; all six existing v0.1.0/v0.2.0 archive files were verified
unchanged. Existing versioned older installers in `dist` were retained.
No signing or publication was performed in this validation phase.

### Final tested inputs and coverage

- Rust executable unchanged from the directory-sharing fix: prior **102 native
  library + 52 CLI + 5 elevated updater tests** remain applicable. These were
  not needlessly rerun for the installer-only change.
- Final `installer/setup.iss` SHA-256:
  `8cb458a776009cb22537632e5726b3fbad3b2e4ea51659d53b29a888d90d3262`.
  Maintenance and lifecycle source hashes remain those in the initial run.
- Real Inno 6.7.3 compilation succeeded. Production XML fixtures again accepted
  3 valid definitions and rejected 21 mutations.
- Actual Scheduler XML, SYSTEM/highest identity, exact action/path, hourly
  repetition, first trigger approximately +1 hour and no immediate task run
  passed. Actual DACL denies Users write/delete/ACL-modification rights.
- Default desktop shortcut and updates selected, monitor absent; silent setup
  does not launch the app.
- **Desktop-off → marker-based automatic upgrade with empty tasks → default
  silent upgrade remains off.** The previously failing assertion now passes.
- Added explicit desktop-on coverage: marker-based empty-task upgrade and a
  subsequent default silent upgrade preserve the selected desktop shortcut.
- Updater preferences retained for opted-in and opted-out upgrades, including
  `/SECBLITZUPDATE=1` and the legacy empty-task worker invocation.
- Nine foreign-task variants rejected without XML/ACL changes; owned-task
  uninstall succeeds. The previously prohibited writable-ACL fixture remains
  excluded from this permitted lifecycle run.
- Fresh optional monitor installs stopped under LocalService, produces a
  schema-1 report when explicitly started, survives an upgrade and resumes
  running. Locked lookalike-file uninstall fails quietly without changing the
  running service; retry succeeds after releasing the fixture lock.
- Final uninstall removes app/service/shortcuts/task and preserves the monitor
  report, journal sentinel, unrelated file and Inno-lookalike file.
- Installed offline `update check --json` and `update status --json` both
  return structured failures with exit 1, with a nonzero persisted check time.
  Untrusted temporary-copy `update install-staged` rejects silently with exit 1.

`lifecycle-results/safe-lifecycle.out` records the completed lifecycle PASS;
`status.jsonl` records exit 0 for the lifecycle and expected exit 1 for the
untrusted worker. The temporary harness still creates its disposable journal
directory with the production protected DACL; production sources were not
modified by validation.

### Restored state

`lifecycle-results/final-state.json` reports `Passed`, `BaselineSame` and
`JournalsSame` all true, neither original directory left moved aside, and no
task/service present. Independent `handoff.json` also confirms no installed
EXE, desktop/start-menu shortcut or Secblitz process. All 18 controls and
original journal hashes match. The test-only updater preference value was
removed after uninstall to restore its original absence.

### Prepared later E2E fixture

An isolated source copy and separate Cargo target directory were used to build
**0.3.99** for the later live signed-update test:

```text
/tmp/opencode/secblitz-v0399-fixture/target/x86_64-pc-windows-gnu/release/secblitz.exe
SHA-256: 5002623b71effdfb5c20b44000e00429e297939ca5667f51bf84a9b702c0bf18
```

The fixture uses identical origin and public-key assets. Only its temporary
Cargo manifest/lockfile version was changed; repository Cargo files, normal
release build outputs and promoted products were not used as fake-version
outputs. Offline, locked cross-compilation passed. The fixture has not yet been
installed or used for a network update.

Live signed-feed upgrade remains **pending publication**, as requested. This
acceptance does not claim live HTTPS/feed success, a Windows 10 run, or a fresh
attended walkthrough of all six localized installer pages.

## Previous retest: native fixes passed; desktop opt-out regression blocked promotion

Evidence: `/tmp/opencode/secblitz-v040-dirfix/`; unique guest workspace:
`C:\Windows\Temp\SecblitzV040OfflineC`. UI clone only, offline throughout.

Tested release EXE SHA-256:
`40cf5fc714e6dcff15de2186832b37f15f8ec6e4d6bc581a641fd53dd9ad4e16`.
Frozen `src/updater/windows.rs` SHA-256:
`d51652b9a1136b3e78ea100e65efb3e36ad0d9dbae068d3d231f08b0bfecde9a`.
Installer source hashes remain those recorded in the initial run below.
All input hashes are retained in `frozen-inputs.json`.

### Passed

- **102 ordinary native library tests**, **52 CLI tests**, and **all five
  elevated updater tests**. The two attended menu probes remain ignored.
- The elevated tests cover prefix rename denial, protected payload write/delete
  denial, hardlink/reparse rejection, staged replacement under ancestor pins
  while respecting a payload pin, and duplicate-worker lock exclusion.
- XML ownership validator: 3 accepted fixtures, 21 rejected mutations.
- Actual Inno 0.4.0 compilation and default installation.
- Persisted Scheduler XML is accepted by production maintenance, including
  SYSTEM principal serialization without a `LogonType` element.
- Actual task is `\SecblitzUpdate`, SYSTEM/highest, one quoted installed EXE
  action, exact `update check` arguments and pinned working directory, one
  `PT1H` trigger beginning approximately one hour after registration. Before
  the explicit offline CLI check, task result was **267011** (not yet run),
  with zero missed runs.
- Actual task DACL:
  `O:BAG:BAD:PAI(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x1200a9;;;BU)`.
  SYSTEM/Administrators have full access; Users have no write/delete/ACL-change
  rights.
- Default desktop shortcut created; optional monitor not registered; silent
  install did not launch the interactive app.
- Opted-in update preference retained with `/TASKS="" /SECBLITZUPDATE=1`
  and legacy silent `/TASKS=""`.
- Nine foreign-task variants rejected by both setup (exit 7) and uninstall
  (exit 1), preserving XML and ACL: command, arguments, directory, principal,
  extra action, disabled task, interval, extra trigger and settings. The unsafe
  writable-ACL fixture was excluded.
- Owned-task uninstall succeeded and retained the journal sentinel.
- Fresh `/TASKS=""` install opted out; updater opt-out remained intact across
  marker-based and default silent upgrades.
- Offline installed `update check --json` and `update status --json` both
  returned exit 1 with structured failures. Status persisted a nonzero
  `checked_at` and `result.outcome: "failed"`, reason
  `Update check or staging failed`. This is not a live-feed success. The redacted
  output alone does not establish the underlying native transport error.

### Current blocker: desktop choice lost across automatic/default upgrades

The real lifecycle failed at production `installer/test-lifecycle.ps1:193`:

```text
Default upgrade lost the saved desktop opt-out.
```

Reproduction after successful uninstall:

1. Install with `/VERYSILENT /SUPPRESSMSGBOXES /SP- /NORESTART /TASKS=""`:
   no desktop shortcut, no monitor, updates disabled.
2. Upgrade with the same flags plus `/SECBLITZUPDATE=1`: exits 0.
3. Upgrade silently without `/TASKS`: exits 0 but creates
   `C:\Users\Public\Desktop\Secblitz.lnk` unexpectedly. Updates remain disabled.

The relevant source is `installer/setup.iss:124–142`, especially preservation
through `GetPreviousData('DesktopSelected', '1')` inside
`RegisterPreviousData`. The exact point at which the saved value changes has
not been instrumented; the observed regression is reproducible from the logged
sequence above.

Detailed evidence is under
`cleanup-results/LifecycleLogs/SecblitzLifecycle-b68c77ad557f4744999bb065bd52e71b/`:
`no-tasks-install.log`, `opted-out-automatic-upgrade.log`,
`opted-out-default-upgrade.log` (lines 62–65 record unexpected shortcut creation),
and `status.txt`. `cleanup-results/safe-lifecycle.err` contains the assertion.

### Harness correction and final cleanup

The first attempt paused at an added offline-status assertion because the
existing lifecycle fixture creates an inherited, user-writable ProgramData
sentinel directory. The updater correctly rejected that unprotected fixture.
Only the temporary harness was adjusted to create its disposable directory
with production's SYSTEM/Administrators-only protected DACL before writing the
sentinel. Original state remained archived during the test. The same compiled
installer was then used for the resumed lifecycle; no production source changed.

After the desktop regression, the test installation was explicitly uninstalled,
its test-only preference value removed, and original directories restored.
`cleanup-results/cleanup-state.json` and `cleanup-PASS.txt` verify:

- All 18 control baselines unchanged; original journal hashes unchanged.
- No installed executable, desktop/start-menu shortcut, updater task, monitor
  service or Secblitz process remains.
- Original application directory is not left moved aside.

No artifacts were promoted to `dist`. Remaining monitor upgrade/uninstall
stages and attended installer-option checks were not reached. Live feed E2E
still awaits publication after installer acceptance.

## Previous retest: ancestor ACL fix accepted; sharing violations remained

The rebuilt artifacts were frozen into `/tmp/opencode/secblitz-v040-fixed/`
and tested in the unique guest directory
`C:\Windows\Temp\SecblitzV040OfflineB` on the same offline UI clone.
The original user VM was not touched.

| Rebuilt input | SHA-256 |
| --- | --- |
| Release executable | `0a742262a3b91b2c5a28e9261592b2fb8ee3b0b2e9d28e60232e8daa111a736d` |
| Native library executable | `cb9e7415010724b314caf817e947b24d79ef782011ee5749dcf2f60bdc795b6e` |
| Native CLI executable | `5bf8ec304e47b464bd26eb237dc084e186c81a08432bc5a658c9f3f983bc303c` |
| `src/updater/windows.rs` | `9d83e116e2c8c9b637454f18ca5b4f3dd086997ecae7d16608a682151fe85899` |

Installer sources are unchanged from the hashes recorded below. The rebuilt
release executable hash was checked again inside the guest.

Results:

- **102 library tests passed**, 0 failed, 4 elevated tests initially ignored.
  The three new native SDDL tests passed, including the ProgramData ancestor
  exception and continued rejection of ACL/replacement rights.
- **52 CLI tests passed**, 0 failed, 2 attended menu probes ignored.
- Production XML fixtures: **3 accepted, 21 rejected**.
- Real Inno Setup 0.4.0 compilation passed using the rebuilt executable.
- Explicit elevated updater suite: **2 passed, 2 failed**, exit **101**.
  Payload pinning and duplicate-worker lock exclusion passed.

The previous `Untrusted update write/execution rights` failure is resolved.
Both remaining tests successfully return from `update_root()`, then fail with
Windows error **32**, `ERROR_SHARING_VIOLATION`:

```text
staged_objects_reject_hardlinks_and_reparse_points
src/updater/windows.rs:931
fs::hard_link(&base, &hard).unwrap();

data_access_directory_pin_blocks_prefix_rename
src/updater/windows.rs:951
fs::rename(&path, &renamed).unwrap();
```

The second failure occurs at the expected-success rename **after** dropping the
direct directory pin; its preceding expected-failure assertion passed. Both
tests still hold the `_pins` vector returned by `update_root()`. This is the
next concrete native-test blocker. Whether it requires fixture-lifetime changes
or production pinning changes remains for the updater owner to determine;
no Rust source or ACLs were changed during validation.

Evidence is in `lifecycle-results/updater-native-elevated.out` and
`lifecycle-results/final-state.json` under the new host evidence directory.
All 18 control snapshots and original journal hashes still match. Original
application/data directories are restored; no updater task or monitor service
is present. Failed-test scratch objects are retained outside the restored
original state directory for inspection.

Installer execution and scheduler lifecycle remain pending: the harness stopped
on the elevated test failure before installing. No artifacts were promoted to
`dist`, and no feed publication or live E2E test was attempted.

## Initial run (historical evidence)

## Scope and isolation

Validation ran on 2026-10-03, only on `Secblitz-W11-UI-Test`
(`4b70288b-b64d-4796-a725-006da3162d0f`). The original user VM was not used.
The UI clone remained offline. A powered-off snapshot was taken before testing:
`secblitz-pre-updater-v040`, UUID `a9c4abb2-3a56-4743-91d2-3cde1c1318a7`.

No Defender disabling, password generation, or unsafe privilege fixture was
performed. The prepared lifecycle harness excludes the writable-task-ACL
fixture; execution stopped before reaching any installer lifecycle fixture.

## Frozen inputs

Host evidence and harnesses: `/tmp/opencode/secblitz-v040-tests/`.
Guest workspace: `C:\Windows\Temp\SecblitzV040OfflineA`.

| Input | SHA-256 |
| --- | --- |
| Release executable | `686cf4e93d22cf4d80a006df7f1136019c3e9f9281a62b94d7dba308c7392adf` |
| `installer/setup.iss` | `da9dc9bea1ab285ceaf8fd4664c21688c64028b49476fe576e7c02b134d96e98` |
| `installer/maintenance.ps1` | `ea9b88c5ffd93ac19fdf2fe717c2ec0c964a79ca219f3f31052bdc57d4a92e39` |
| `installer/test-lifecycle.ps1` | `fafc3da20a7390b0fb1e9ac3c20470d5034dbd04e5214091ecde0d4c1042422f` |

The release executable hash was independently checked inside the guest.
Native test executables were frozen from `secblitz-ed4c5dfdab55058b.exe`
(library) and `secblitz-bc97024b47eb89fc.exe` (CLI).

## Executed results

| Check | Result |
| --- | --- |
| Native library suite, serial | 99 passed, 0 failed, 4 elevated updater tests initially ignored |
| Native CLI suite, serial | 52 passed, 0 failed, 2 attended menu probes ignored |
| Production pure XML ownership validator | 3 valid fixtures accepted; 21 mutations rejected |
| Real Inno Setup compilation, version 0.4.0 | Passed; installer produced inside guest |
| Elevated payload pinning test | Passed |
| Elevated duplicate-worker lock test | Passed |
| Elevated directory-prefix pinning test | Failed opening update root |
| Elevated staged hardlink/reparse rejection test | Failed opening update root, before exercising links |

The elevated test command was:

```text
native-lib.exe updater::windows::tests:: --ignored --test-threads=1
```

It exited **101**: 2 passed, 2 failed, 99 filtered out.

## Runtime blocker

Both failed tests returned:

```text
called `Result::unwrap()` on an `Err` value: Untrusted update write/execution rights
```

Locations in the tested source:

- `src/updater/windows.rs:834`: `data_access_directory_pin_blocks_prefix_rename`.
- `src/updater/windows.rs:816`: `staged_objects_reject_hardlinks_and_reparse_points`.
- Both fail at `update_root()`, before their intended assertions.

Read-only ACL evidence identifies a mismatch in the ancestor ACL allowlist:
`C:\ProgramData` grants Users (`S-1-5-32-545`) an applicable
ContainerInherit allow ACE with mask **`0x00000116`**. In
`inspect()` (`src/updater/windows.rs:130–145`), the non-strict ancestor
allowlist permits read/execute and add-file/add-subdirectory rights but excludes
`FILE_WRITE_EA` (`0x10`) and `FILE_WRITE_ATTRIBUTES` (`0x100`). Consequently this
ACE has disallowed bits `0x110` and triggers the observed error.

The original state directory and the fresh test state directory both have the
protected DACL `O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`. No ACL was weakened
to make the tests pass. Any fix must be reviewed by the updater owner and then
retested with fresh frozen artifacts; this report does not recommend blindly
broadening the allowlist.

Evidence:

- `lifecycle-results/updater-native-elevated.out`: exact native failures.
- `acl-evidence.json`: ancestor and state-directory owners, SDDL and ACE masks.
- `native-results/status.jsonl`: successful native suites, XML fixtures and build.
- `lifecycle-results/final-state.json`: cleanup and preservation checks.

## Preservation and final state

The harness restored the original application/data directories after the test
failure. Independent before/after snapshots show all 18 control baselines,
including service security descriptors and Defender/tamper observations,
unchanged. Original journal/file hashes also match exactly.

Final harness evidence reports:

```json
{
  "Passed": false,
  "TaskPresent": false,
  "OriginalDataMovedAside": false,
  "BaselineSame": true,
  "OriginalAppMovedAside": false,
  "JournalsSame": true,
  "ServicePresent": false
}
```

## Pending release gates

Installer execution, actual task XML/DACL/defaults, hourly delayed start,
upgrade preference retention, foreign-task rejection, uninstall/report
preservation and installed offline transport/status behavior have **not yet
been tested for v0.4.0**. Installer compilation alone is not lifecycle acceptance.
The prepared lifecycle harness stopped at the preceding elevated native test
failure.

No artifacts were promoted into `dist`; no signing or publication was performed.
Live signed-feed end-to-end upgrade remains a later coordinated phase after
the local blocker is fixed and publication is confirmed.
