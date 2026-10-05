# Wave 1 service / installer review (agent 3)

## Wave 3 - actual runtime blockers fixed and lifecycle passed

Both Wave 2 runtime blockers were reproduced from the recorded evidence and
fixed. Full details, exact exit codes, artifact hashes and guest state are in
the **Wave 3** section of `docs/windows-test-results.md`.

* Added fixed PATHEXT to the clean environment and replaced native `& $exe` /
  `$LASTEXITCODE` handling with explicit non-shell ProcessStartInfo execution,
  initialized exit state and checked child completion. Registration is queried
  after successful install, so an exit 0 without the service is also rejected.
* Added an exact active-uninstaller-data parameter sourced only from Inno's
  `{uninstallexe}`. That one direct-child data file receives metadata-only access;
  regular-file/link/owner/DACL checks remain mandatory. All other files and
  directories retain normal pins. This is not a recursive wildcard bypass.
* Quiet uninstall failure now returns nonzero without a modal dialog in silent
  mode, including caught initialization exceptions.
* Regression helpers test the exclusive data handle, unsafe ACL on that file,
  a different locked lookalike and real native exits 0/37 without PATHEXT.
* The final repository lifecycle script now tests actual scan output, running
  upgrade/resume, silent rejection while preserving a running service, successful
  uninstall retry, and journal/report/unrelated-file preservation.

Guest lifecycle **passed twice**, including the final updated script. The
additional original-data-in-place cycle also passed. Final process exit sequence
was **0 install, 0 running upgrade, 1 intentional silent rejection, 0 uninstall**.
No app/service remains installed; journals were restored and hash-verified.
40 library and 12 CLI Windows tests, all-targets Clippy, Inno compilation and
regression harnesses passed. The exact passing artifacts are now in `dist`, with
updated checksums. Prior wave limitations below are historical where superseded.

## Wave 2 API handoff - experience/main owner

The runtime evidence in `docs/windows-test-results.md` supersedes the earlier
wave-1 bootstrap success claim: missing `Microsoft.PowerShell.Security` import
blocked real Prepare. It is now explicitly imported from the system module path.
Post-install failures are latched and reported through Inno's
`GetCustomSetupExitCode` as **20** if Inno otherwise reports success. Prepare's
native failure exit remains **7**.
The normal finished/success page is skipped after a latched post-install failure.

**Experience agent: API is ready for main integration.** `install()` and
`uninstall()` retain `Result<()>` and now print nothing. `status()` retains
`Result<()>` for compilation compatibility and also prints nothing. Switch the
status presentation branch to `service::status_details() -> Result<String>`.
This returns a stable JSON diagnostic string for the localized diagnostic heading
already added by the experience agent. For a fully typed/localized state display,
`service::query_status() -> Result<StatusDetails>` additionally exposes:

* `state: MonitorState`: `NotInstalled`, `Stopped`, `StartPending`, `StopPending`,
  `Running`, `ContinuePending`, `PausePending`, `Paused`.
* `win32_exit_code: Option<u32>` and `service_exit_code: Option<u32>` distinguish
  the exit-code namespaces; both are `None` when absent.
* `checkpoint: u32`, `wait_hint_ms: u64`.

Both types implement Serialize; states serialize to stable snake_case keys.
Main may map the enum to localized labels and render numeric diagnostics.
Install/uninstall can use existing localized success output. No deprecation
attribute was added to the compatibility API, so `-D warnings` does not break
the existing main pending its update. Service failures still return anyhow
errors for the caller's error presentation path.

### Wave 2 results

Read the latest runtime evidence before patching; added the missing explicit
Security import. `test-maintenance.ps1` now executes the production module
bootstrap before loading helpers, so ambient autoloading cannot conceal this
class of failure again.

Verified in isolated `C:\Windows\Temp\SecblitzAgent3Wave2` artifacts:

* Cold Windows PowerShell 5.1 `maintenance.ps1 -Action Validate`: **exit 0**,
  empty stderr. This reads existing path/service metadata but performs no
  install/start/stop/remove operations.
* Production bootstrap + ACL helper tests: **PASS**.
* `test-setup-exit.ps1` no-install harnesses using production post-install and
  custom-exit handlers: success **0**; injected Secure failure **20**;
  InstallMonitor failure **20**; ResumeMonitor failure **20**; thrown maintenance
  exception **20**. The harness bypasses Prepare's directory check because
  CreateAppDir=no changes `{app}` semantics; no app directory or service is made.
* Production installer source compiled successfully with Inno **6.7.3**, exit 0.
* Windows service tests: **5 passed**. Linux service tests: **3 passed**.
* Windows-target library Clippy with `-D warnings`: passed. Initial all-targets
  Clippy was blocked by a concurrent experience-owned `src/ui.rs` constant
  assertion test (`assertions_on_constants`); no UI source was changed here.
* All five owned PowerShell scripts parse successfully.

Evidence: `/tmp/opencode/secblitz-agent3-wave2-results.zip`. Shared guest app,
service, journal and security-setting state were not modified. Full actual
installer lifecycle retest remains with the runtime owner after rebuilding the
application and installer together. Guest compilation here establishes installer
source compatibility, not a fully tested/rebuilt release payload.

Scope: `src/service.rs`, `src/service/*`, `installer/*`,
`scripts/build-release.ps1`, `.github/workflows/windows.yml`.

## Coordination

Service and installer corrections are complete. The runtime/main guest-test
owner must rebuild the app and installer from these sources before testing the
optional monitor. This agent used isolated
`C:\Windows\Temp\SecblitzAgent3` compiler/test artifacts and temporary ACL
fixtures only; it did not install, start, stop or remove the guest's service/app.

The shared application root and binary now accept an optional Users **exact RX**
ACE; the Monitor directory/report still reject Users. New standalone installs
also grant Users RX on the app root/binary. Protected DACLs and trusted owners
remain required. Windows-only descriptor regression tests are included.

## Findings and fixes

1. **Shared application ACL incompatibility:** fixed the Users RX mismatch as
   described above. Owner checks, protected DACL checks, mandatory SYSTEM/Admin/
   LocalService grants and exact access masks remain fail-closed.
2. **Metadata-only handles did not actually pin Windows paths:** reproduced in
   the Windows guest: a file opened with READ_CONTROL/read-attributes could still
   be renamed despite omitted FILE_SHARE_DELETE. Service ancestor/directory
   handles now request list-directory access as well. Installer pins request
   read-data/list-directory and retain handles through maintenance. Regression
   tests verify rename fails while pinned and succeeds after release.
3. **Unsafe elevated script trust:** uninstall previously executed an installed
   script before checking its tree. Setup and Uninstall now embed the same script
   at compile time, extract it into Inno's protected temporary directory, and
   never execute an existing installed script. The launcher uses an absolute
   system PowerShell path, system working directory, `-NoProfile`, and an
   environment allowlist. This removes inherited module search paths and CLR
   profiler/runtime injection variables before process creation. The script then
   disables autoloading and imports inbox modules by absolute path.
4. **Installer path/ACL checks:** reject hard links, reparse points, untrusted
   owners, null DACLs, write/delete/DACL/owner grants and malicious inherit-only
   grants. Validate/pin ancestors from volume root downward. Create a new app
   directory with its protected DACL atomically, then validate even if creation
   raced with an existing directory. Only the known monitor report permits
   LocalService write access; no recursive ACL repair or deletion was introduced.
5. **SCM setup/rollback exposure:** create the service disabled, finish its DACL,
   quoted command, restricted privilege list and description, then enable auto
   start. Installation requests only CHANGE_CONFIG/DELETE/WRITE_DAC/WRITE_OWNER.
   Existing registrations are never overwritten. Rollback only removes newly
   created resources and retains files if registration removal fails.
6. **Upgrade and stop state handling:** handle StartPending and StopPending with
   a bounded wait instead of unconditionally sending Stop. Preserve whether an
   existing monitor was running; resume it after upgrade and attempt resume on
   cancellation/failure. New installations remain stopped. Service StopPending
   checkpoints remain monotonic and workers are joined before Stopped.
7. **Uninstall elevation ordering:** the pre-elevation InitializeUninstall call
   returns successfully; elevated initialization performs maintenance. Failure
   to stop/remove the owned service aborts before app deletion. Journals and
   monitor reports are outside installer file-removal entries.

`scripts/build-release.ps1` was audited and syntax-checked; no source changes
were needed. It retains locked Rust builds/tests/lint, static CRT import checks,
publisher-verified compiler download, optional signing and artifact hashes.

## Verification performed

* Sourced `/tmp/opencode/secblitz-cross-env.sh`; Windows GNU test executables
  compile successfully (`cargo test --locked --target x86_64-pc-windows-gnu
  --no-run`).
* Windows-target all-targets Clippy passes with `-D warnings`.
* Linux service tests: **2 passed** (path validation and unsupported platform).
* Windows 11 guest Rust service tests: **4 passed** (path aliases, shared-app
  adversarial ACLs, strict monitor ACLs, real directory pinning).
* Windows PowerShell 5.1 `installer/test-maintenance.ps1`: **PASS** for safe RX,
  actual rename pinning, hard links, junctions, write/DAC/owner/delete grants,
  inherit-only writes and untrusted ownership.
* Full actual installer compiled successfully with **Inno Setup 6.7.3**, including
  all five languages and the embedded script. The compile caught and corrected a
  Pascal type incompatibility (`NativeInt`; default x86 Setup uses `LongWord`).
* A separate no-install Inno harness exercised the production launcher with
  poisoned PSModulePath, COR_ENABLE_PROFILING, COMPLUS_Version and a custom
  environment marker. All were removed; actual maintenance `Validate` completed
  successfully under the clean environment. Harness exit **0**.
* All four owned PowerShell scripts parse successfully. Evidence is in
  `/tmp/opencode/secblitz-agent3-check.zip`; guest artifacts are under the isolated
  directory above. These are unsigned development artifacts, not release proof.

## Runtime handoff / checks still required

`installer/test-lifecycle.ps1` now automates fresh optional-monitor installation,
starting it, upgrading while running with the monitor task unchecked, confirming
it resumes, uninstalling while running, and checking report/ProgramData journal
sentinel preservation. CI runs it after compiling the actual installer and also
runs the ACL helper tests under Windows PowerShell 5.1.

The lifecycle script deliberately refuses existing app/data/service state. It
was **not run against the shared guest**, to avoid interfering with the runtime
agent's active tests. The runtime owner should test equivalent upgrade/uninstall
cases with the rebuilt artifacts and preserve existing journal evidence. CI
lifecycle execution, Windows 10 runtime behavior, non-admin UAC relaunch, signed
uninstaller verification, failure-injected rollback and Inno 6.4.x execution are
not claimed as completed here. The successful local compiler was 6.7.3.
