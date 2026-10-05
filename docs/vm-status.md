# Windows VM readiness

## Latest unreleased 0.6.0 development validation

The isolated UI clone remains **powered off with all eight NICs `none`** after native foundation and TUI work. All 18 original controls and the original journal bytes, ACLs and attributes were verified unchanged. The user-owned original VM was untouched.

Evidence is in `target/windows-v060-candidate-validation/` (247 native library tests, 92 CLI tests and targeted trust/process fixtures), `target/windows-engine-native-validation/` (resolved fault-injection slowdown), and `target/windows-v060-ui-final/` (98 native CLI tests, actual ConPTY navigation/page keys/resize/mode restoration). One patching identity fixture required an unavailable interactive split-token account; it was not counted as passing. No actual Windows update installation or DISM repair is claimed.

Source is now 0.6.0, unreleased. A final resize-cache follow-up has a focused passing host regression and Windows build, with its artifact hash recorded in [candidate evidence](ROADMAP.md#candidate-evidence). This does not replace the published 0.5.0 artifacts or their historical acceptance below.

## Latest 0.5.0 live deployment gate

**LIVE E2E PASS:** genuine published 0.4.3 to final 0.5.0 through the owned
SYSTEM task, with the running LocalService monitor resumed and automatic startup
preserved. The floor advanced from an authentic saved 0.4.3 record to 0.5.0;
UpToDate, 18-result/19-finding typed audit, the 9,195-byte monitor report,
real-guide DeferredBusy and normal Esc exit passed. Eight recorded integration
gates completed. See [windows-v050-results.md](windows-v050-results.md).

Cleanup restored all 18 controls, original WALs, original directories and static
network settings. The UI clone is **powered off with NIC1/NIC2 `none`**, with no
app/service/task/shortcut/process left. The original user VM was untouched.
Published artifacts were not changed and no new release was published by the
native validator.

## Latest 0.5.0 native/UI handoff

The **copy-fixed final pre-release candidate is now promoted**. Its new native
CLI suite passed all 66 tests, including 13 localization and 15 UI/advice checks.
The unchanged native library/core retain the previous functional acceptance.
The new real installer passed installed-version and finish-launch guide smoke;
all controls/WALs were preserved and the test installation removed. Current
hashes and the resolved undo-copy note are at the top of
[windows-v050-results.md](windows-v050-results.md). UI clone is powered off and
offline; `dist/archive/0.4.3/` remains unchanged.

**0.5.0 native and real-UI acceptance PASS; exact artifacts promoted.**
138 native library tests, 66 CLI tests including all 13 localization checks,
the explicit readiness smoke and nine SYSTEM updater cases passed. Independent
Windows volume/power/MTA COM checks, typed firewall evidence, zero-WAL inherited
default behavior, the two-control recommended-batch apply/undo with automatic
verification, and a bounded failure path passed. Actual installer defaults and
finish launch, full lifecycle and LocalService readiness/metadata were verified.

All 18 controls and original WAL hashes are restored. Only the UI clone was used;
it is **powered off with NIC1/NIC2 `none`**, with no app/service/task/shortcut left.
The original user VM remains untouched. The tested 0.5.0 EXE/setup are in `dist`,
and 0.4.3 is preserved in `dist/archive/0.4.3/`. The subsequent published live
upgrade passed as recorded above. See
[windows-v050-results.md](windows-v050-results.md) for hashes, evidence and the
subsequently resolved legacy undo-footer copy observation.

## Current 0.4.3 build and VM routing

**Latest state: 0.4.3 LIVE E2E PASS.** The genuine 0.4.2 upgrade, new release
floor, UpToDate, audit and real-guide DeferredBusy passed. Cleanup restored all
18 controls, original WALs and network settings. The UI clone is powered off
with NIC1/NIC2 `none`; the original user VM remains untouched. See the live
section of [windows-v043-results.md](windows-v043-results.md).

**0.4.3 basic acceptance is now PASS and exact artifacts are promoted.** The
fixed loopback case, 116 native library tests and 52 CLI tests passed. Prior
nine SYSTEM updater tests and full installer acceptance remain valid because
release bytes did not change. All 18 controls and original journals match;
the UI clone is powered off with NIC1/NIC2 `none`. See
[windows-v043-results.md](windows-v043-results.md). This earlier basic acceptance
was followed by the successful live validation recorded above.

Only **Secblitz-W11-UI-Test**, UUID
`4b70288b-b64d-4796-a725-006da3162d0f`, is authorized for current native work.
The original **Secblitz-W11-Test** is user-owned and must not receive guest
commands, input, configuration, power or snapshot operations. All older helper
references below are historical and must not override this restriction.

The deleted temporary toolchain has been replaced by persistent user-local
configuration. No root packages or global shell configuration were changed.

```sh
source target/build-tools/cross-env.sh
cargo test --locked
cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings
cargo build --locked --release --target x86_64-pc-windows-gnu
cargo test --locked --target x86_64-pc-windows-gnu --no-run
```

Paths relative to the project root:

- Environment: `target/build-tools/cross-env.sh`.
- `RUSTUP_HOME`: `target/build-tools/rustup`, a persistent link to the existing
  `target/tools/rustup` cache. Rust **1.93.0**, Linux host and Windows GNU std.
- `CARGO_HOME`: `target/build-tools/cargo`. The official rustup initializer was
  downloaded over HTTPS and its published SHA-256 verified. Installation used
  `--no-modify-path`; the matching Clippy component was added locally.
- MinGW: `target/build-tools/mingw`, linked to the cached extracted Ubuntu
  packages in `target/tools/mingw`. GCC **13 POSIX**, binutils **2.45.90**.
- Linker wrapper: `target/build-tools/bin/x86_64-w64-mingw32-gcc`.
- Default `CARGO_TARGET_DIR`: `target/windows-release`.
- `TMPDIR`: `target/compiler-tmp`, on the persistent filesystem.
- UI-clone-only guest helper: **`target/windows-validation-tools/guest.py`**.
  It hardcodes the UI clone UUID and passes the existing credential path directly
  to VirtualBox with `--passwordfile`; it never reads or prints the credential.

The persistent cache already contained the compiler, target std and extracted
MinGW packages, so they were reused. The full 0.4.3 Windows release and test
builds verify that the restored linker/resource/compiler paths work. The system
Rust 1.93.1 installation remains unchanged.

Current build logs and frozen-input hashes are in
`target/windows-validation-v043/`. The initial locked build detected stale
0.4.2/indicatif 0.17 lock entries; a targeted `cargo update -p indicatif` refreshed
the generated lockfile to the requested 0.4.3/indicatif 0.18 manifest and removed
`number_prefix`. Formatting, full host tests, host Clippy, Windows Clippy and
cross-compilation passed afterward. No application logic was edited.

The evaluation guest can shut down at about one-hour uptime. Inspect active
fixtures and evidence before any retry, and restart only the UI clone. Windows
10 and the original user's VM remain outside this native acceptance phase.

Observed 2026-10-02 using local VirtualBox 7.2 tooling.

## v0.2.0 expanded-control handoff (current)

After acceptance testing, bidirectional **text clipboard** was enabled on `Secblitz-W11-Test` at the user's request and verified with VirtualBox. Clipboard file transfers remain off. This convenience change is outside the isolated benchmark configuration; the account password was not changed. Clipboard integration can require a signed-in desktop session.

See [windows-v020-results.md](windows-v020-results.md) for the authoritative new phase; earlier-version results below are historical.

- Snapshot before changes: `secblitz-pre-expanded-v020`, UUID `c680e300-1a4a-47e8-b048-7777855b67df`, created powered off/offline.
- **70 library + 16 CLI native Windows tests pass.** Italian help passes. All four new registry repairs and Windows Update DACL repair/byte-exact rollback passed; repeated apply/revert and safe DACL-drift conflict/retry passed.
- BITS baseline has a ContainerInherit-flagged ACE and is correctly skipped. Its injected broad dangerous grant was detected but not automatically repaired; no guard was bypassed. Both service fixtures were removed manually in final cleanup.
- Independent synthetic-standard-user Authz checks show Windows Update CHANGE_CONFIG/WRITE_DAC granted before and denied after repair; owner/group, flags, benign rights and other ACE bytes are preserved. SACL was neither requested nor set. No account/logon/exploit was used.
- All **18** final captures match baseline exactly, including original registry absence; all retained transactions reverted. Unsafe registry fixtures were restored before any reboot/session handoff. Defender/tamper stayed enabled.
- Exact v0.2.0 Inno package: compile/install/monitor scan/running uninstall pass. Monitor: **18 observations / 19 findings**, incomplete=false. Real journal hashes preserved. The later two-string Italian correction was recompiled with **zero warnings**, and the exact final installer passed `/LANG=it` optional-monitor install/uninstall. Binary unchanged; all 18 controls and journal hashes still match. Final installer SHA-256: `df0b0e81056176f7391cac6b566ca55b48b54995091af3029a6a2045d7a5348c`.
- Current artifacts and hashes are in `dist`; previous v0.1.0 files/checksums archived in `dist/archive/0.1.0/`. Website configuration was not changed by this task.
- Final VM: running (observed state change `2026-10-02T13:18:23.269000000` UTC), all NICs disabled, no shares, no installed application executable or monitor service, no outstanding unsafe fixture. Original/other VMs untouched. Windows 10 remains untested.

## Final acceptance handoff - PASS

Executable `f126d1ee17901a2a80145ef7c729c5c4fcb8e71b1ab996920e9a55d0a754e25c` passed real Windows 11 apply/idempotence/revert/retry and induced-drift handling. **Five firewall/UAC controls were actually changed and undone**; all 12 were independently captured and ultimately restored to the exact baseline. Defender tamper protection remained true; no Defender mutation is claimed. All transactions are reverted. **40 library + 13 CLI native tests pass**.

Exact final package compiled with Inno 6.7.3, installed with optional monitor, produced a complete schema-1 scan, and uninstalled while running: all relevant process exits 0. Actual transaction-journal hashes survived install/uninstall unchanged. The final executable/setup and verified SHA256SUMS are in `dist`; detailed hashes/evidence are in [windows-test-results.md](windows-test-results.md).

Final guest: running headless, all NICs disabled, no shared folders, **no service or installed application executable**, all fixtures restored. Real reverted journals and monitor reports retained. Original/other VMs untouched. Expired evaluation licensing remains an environment limitation; Windows 10 remains untested. These final results supersede all historical blockers below.

## Previous targeted runtime handoff (resolved)

The scoped-policy release established real eligibility and passed **40 library + 13 CLI native tests**, but apply failed on the Public Enabled setter: PowerShell Boolean cannot bind to NetSecurity `GpoBoolean`. Earlier Domain/Private inbound changes were successfully restored by **tool revert**, and the transaction is now **reverted**. All 12 independently captured controls and exact consent DWORD bytes match the initial baseline after cleanup. Tamper protection stayed true. No service was installed by this run. Only the clone was restarted after another evaluation-expiry shutdown; it is left running/offline.

See the current top section of [windows-test-results.md](windows-test-results.md) for candidate hash and reproduction. Full idempotence/undo/drift/package promotion is blocked pending the setter fix. Prior passing Wave 3 `dist` artifacts were preserved and their hashes verified; they are not the failing new candidate. No source code was changed by the runtime tester.

## Historical Wave 2 status

Latest results are authoritative in [windows-test-results.md](windows-test-results.md), Wave 2 section. **40 library + 12 CLI Windows tests pass**, help exits 0, real audit/apply/revert produce valid JSON with exit 2. Real mutation remains blocked by configured PolicyManager evidence; reversible fixtures were manually restored and all 12 controls independently verified unchanged. CLI monitor lifecycle and an actual read-only scan pass. A running installer upgrade/resume also passes, preserving the journal sentinel.

Fresh optional-monitor setup exits 20 because the cleaned environment omits PATHEXT and `$LASTEXITCODE` is unset. Silent uninstall is blocked by attempting to pin Inno's exclusively held `unins000.dat`; its error dialog caused a bounded timeout. Both failures have external reproductions without source edits. Current installer in dist is an actual Wave 2 build but still fails release validation. The guest-tested `dist/secblitz.exe` and `dist/SHA256SUMS` are also available.

Final state: running headless, all NICs disabled, no shared folders; **no monitor service**, original journal restored, all fixtures restored. Application/uninstaller files remain installed pending the uninstall fix. The original VM and all other VMs remain untouched. Pre-test snapshot remains `secblitz-pre-integration` (`09567c47-0d8e-4010-838b-cea299f2ac4d`).

The Windows evaluation license is expired and triggered a planned shutdown after about one hour. Restarted this clone at `2026-10-02T04:54:38.631000000` UTC; no licensing or clock changes were made. Account for this interruption risk when continuing tests. Windows 10 has not been tested.

The initial setup/readiness notes below are historical.

## Owned test VM

- Name: `Secblitz-W11-Test`
- UUID: `5f9b7272-286e-47ae-8bb5-e3713af70f5f`
- State: **running**, headless; state change `2026-10-02T02:49:40.151000000` UTC.
- Config: `/home/slay/Secblitz-W11-Test/Secblitz-W11-Test.vbox`
- Full independent clone of the current powered-off `SENTINEL-W11-BASE` state, created with `clonevm --mode machine --register` (no linked-clone option).
- Disk: `/home/slay/Secblitz-W11-Test/Secblitz-W11-Test-disk1.vdi`; UUID `31739d69-291f-49e0-9f83-5bfd84f133de`; parent UUID `base` (no differencing parent), 96 GiB virtual capacity, approximately 30 GiB allocated.
- Resources: 6 GiB RAM, 4 vCPUs, EFI.
- All eight network adapters disabled before first boot; no shared folders; clipboard, drag-and-drop, and VRDE disabled.
- Guest: Microsoft Windows 11 Enterprise Evaluation, build 26200; Guest Additions reports release `10.0.26200.9457`.
- Guest Additions: `7.2.6 r172322`, system service active, run level 2.
- Inherited guest hostname: `SN-W11-BASE`. Network remains disabled.

## Guest execution and build readiness

Authenticated `VBoxManage guestcontrol run` successfully executed PowerShell as the inherited local Administrator account (exit code 0). `guestcontrol copyfrom` successfully retrieved its nonsecret probe output. No network or shared folder is necessary for these operations.

The existing provisioning source `scripts/prepare_windows11.py` in the neighboring `virus-total-alternative` project identifies the baseline credential file. It was passed directly to VirtualBox via `--passwordfile`; its contents were not printed or copied into this repository. Reuse that local credential mechanism for this clone only.

Operational notes:

- This VBoxManage version prepends the executable automatically: arguments after `--` must start with `-NoProfile` for PowerShell, not another `powershell.exe`.
- Direct stdout/stderr retrieval warned `VERR_NOT_IMPLEMENTED`. Reliable observed workaround: write command results to a guest-local file, then use `guestcontrol copyfrom`.
- Successful probe: `C:\Windows\Temp\secblitz-vm-probe.txt`; retrieved host copy: `/tmp/opencode/secblitz-vm-probe.txt`.
- `rustc`, `cargo`, `rustup`, `cl`, `link`, and `git` were not on the guest Administrator PATH.
- `C:\Users\Administrator\.cargo\bin\rustc.exe` and `cargo.exe` were absent.
- Standard Visual Studio Installer `vswhere.exe` was absent from `C:\Program Files (x86)\Microsoft Visual Studio\Installer`.
- Native Windows Rust tooling is absent from the checked locations. Nonstandard toolchain locations have not been exhaustively searched. The verified host cross-build strategy below avoids guest toolchain installation.
- Secblitz was not installed. Await main-task source assembly before building and executing the application.

## Verified host cross-build strategy

Provisioned user-local MinGW from the host's configured Ubuntu package repository using `apt-get download` and `dpkg-deb -x`; no system package installation or guest network access was needed.

- Rust: existing `1.93.0-x86_64-unknown-linux-gnu` toolchain in `/tmp/opencode/secblitz-rustup/toolchains/`, with `x86_64-pc-windows-gnu` already installed.
- MinGW: GCC 13 POSIX and binutils 2.45.90, extracted under `/tmp/opencode/secblitz-mingw/root` with matching runtime/development packages.
- GCC name wrapper: `/tmp/opencode/secblitz-mingw/bin/x86_64-w64-mingw32-gcc`.
- Reusable environment: `/tmp/opencode/secblitz-cross-env.sh`. Sets explicit Rust compiler/doc paths, Cargo/Rustup homes, linker, PATH (including windres), and a separate output directory.
- Cargo output: `/tmp/opencode/secblitz-windows-target`.
- These are temporary host-local paths; retain them for the current build session.

Run from `/home/slay/projects/cybersec/windows-hardening-tool` once source assembly is complete:

```sh
source /tmp/opencode/secblitz-cross-env.sh
cargo build --locked --release --target x86_64-pc-windows-gnu
# Expected application output:
# /tmp/opencode/secblitz-windows-target/x86_64-pc-windows-gnu/release/secblitz.exe

# Compile Windows unit-test executables without attempting to run them on Linux:
cargo test --locked --target x86_64-pc-windows-gnu --no-run
```

Verification completed:

1. Compiled `/tmp/opencode/secblitz-cross-smoke.rs` into `/tmp/opencode/secblitz-cross-smoke.exe`, identified as a PE32+ x86-64 Windows executable.
2. Transferred that executable with `guestcontrol copyto` into `C:\Windows\Temp` on **Secblitz-W11-Test only**.
3. Executed it through guest `cmd.exe`; observed guest process exit code **0**.
4. Retrieved output: `SECBLITZ_WINDOWS_CROSS_SMOKE_OK arch=x86_64 os=windows` in `/tmp/opencode/secblitz-cross-smoke.txt`.
5. Attempted `cargo build --locked --target x86_64-pc-windows-gnu` against the in-progress project. Dependencies and the Windows resource/manifest build step compiled successfully. Application compilation stopped with `E0583` because `src/lib.rs` declares `service`, but `src/service.rs` / `src/service/mod.rs` was not yet present. `src/main.rs` was also absent at inspection time. No Rust source was modified.

Thus Windows executable generation and offline execution are **verified**, but the application binary and its tests remain pending source assembly. Transfer/run/copyback use the same authenticated `guestcontrol` mechanism described above. The clone remains running with networking disabled; no guest toolchain or Secblitz installation was performed.

## Scope and preservation

`SENTINEL-W11-BASE` was only inspected and cloned. Final observation: powered off, same state-change timestamp `2026-09-29T16:21:39.202000000` UTC and original network configuration. No other VM was modified. Only the new clone was started or used for guest commands.

Windows 10/11 is the requested application support scope. This environment establishes Windows 11 guest execution only; Windows 10 compatibility and application behavior on either version remain unvalidated.

## Integration handoff (supersedes initial readiness notes above)

The main source has now been assembled, cross-built, transferred, and executed. Detailed evidence and blockers are in [windows-test-results.md](windows-test-results.md).

- Powered-off pre-test snapshot: `secblitz-pre-integration`, UUID `09567c47-0d8e-4010-838b-cea299f2ac4d`.
- Current VM state: running headless since `2026-10-02T03:53:42.106000000` UTC, all eight NICs disabled, no shared folders.
- Latest Windows unit execution: 27 library and 8 CLI tests passed. English/Spanish help exit 0. First CLI monitor install/start/status/stop/uninstall cycle succeeded.
- Native audit/apply/revert currently exit 1 due to CLIXML progress stderr; independently verified global progress suppression resolves that specific transport issue. Clone also has enrollment evidence, so conservative mutation gates reject the safe reversible firewall/UAC fixtures. Fixtures were restored manually and all 12 controls independently checked unchanged.
- Official Inno Setup 6.7.3 is installed in the clone (`C:\Program Files (x86)\Inno Setup 6\ISCC.exe`). Its downloaded installer signature was valid, Pyrsys B.V. No guest network was enabled.
- `dist/secblitz-0.1.0-windows-x64-setup.exe` is an actual ISCC-produced **diagnostic artifact with known installation failure**, not a validated release. Latest setup fails Prepare because `Get-Acl` is unavailable with autoload disabled. Details and hashes are in the results document.
- Final guest has no installed Secblitz executable in Program Files and no registered monitor service. The journal lock is retained. Inno compiler and temporary testing artifacts remain for rebuilds.
- Guest runner helper: `/tmp/opencode/secblitz-guest.py` (`put`, `get`, `ps`); it passes the existing credential file directly to VirtualBox, never reads or prints its contents. Test scripts/results live under `/tmp/opencode/secblitz-*`.

## v0.3 UI preparation - separate clone only

The user is now personally using **Secblitz-W11-Test**. Do not send it guest commands, input, screenshots, configuration or power/restore operations. The old helper above targets that user VM and must not be used for v0.3 UI testing.

Prepared **Secblitz-W11-UI-Test**, UUID `4b70288b-b64d-4796-a725-006da3162d0f`, as a full independent clone of the immutable powered-off `secblitz-pre-expanded-v020` snapshot (`c680e300-1a4a-47e8-b048-7777855b67df`). Source runtime was not changed. Clone disk parent is `base`; path `/home/slay/Secblitz-W11-UI-Test/`. Allocated 4 vCPUs/4 GiB, disabled all eight NICs, clipboard/drag-drop/VRDE; no shared folders.

UI clone is **running** with an actual logged-on desktop at **1024×768**; Guest Additions and authenticated command execution work. Inno compiler is inherited and available. Defender real-time/tamper protection are enabled. Only this new clone received input. Secret login input was neither printed nor copied.

Independent v0.3 candidate cross-build succeeded in `/tmp/opencode/secblitz-v030-target`. Preliminary host tests: 79 library pass, 35/37 CLI pass; two in-progress localization failures recorded. Native read-only help/audit and actual-console scan/review/empty selection/default-No/exit were exercised. Guide exited 0; all 18 before/after controls match. Screenshots, candidate hash, command plan and limitations are in [windows-v030-results.md](windows-v030-results.md).

No installer installation, hardening apply, extra-tool action, monitor installation or standard-user fixture was performed. Current UI-clone desktop account is Administrator; non-admin UAC behavior awaits a suitable fixture/final handoff. Use only `/tmp/opencode/secblitz-ui-guest.py` and `/tmp/opencode/secblitz-ui-console.py` for further UI-clone work. No current `dist` artifact was replaced. Expired evaluation licensing may interrupt the UI clone; never restart the user's other VM in response.

### v0.3 final-candidate execution update (supersedes preparation above)

The requested final binary was executed in **Secblitz-W11-UI-Test only**. 83 native library + 40 CLI tests pass. Real-console empty/No choices, selected Public-firewall-only batch, second UAC-only batch, reverse-order undo and exact all-18 baseline restoration pass. Guided monitoring produced a fresh 18/19 report; quick scan returned with updated native completion timestamps. Offline Defender update accurately reported failure/uncertainty. Italian installer default desktop/monitor/finish-launch choices and scan-first launch were captured in screenshots.

**Release blocker:** silent optional-monitor install waits at `Press Enter to close` in its hidden native child, reaches the 180-second maintenance timeout and exits 20. A read-only `service status` reproduction hangs with inherited console stdin and redirected outputs, but exits 0 with stdin closed. Source pause handling needs correction; no product source was edited and no failing 0.3 candidate was promoted to current dist/website. Details and candidate hashes: [windows-v030-results.md](windows-v030-results.md).

Cleanup is complete: no UI-clone monitor, installed exe, shortcut, Secblitz process or disposable standard-user account. Original journal directory restored; all transactions reverted and all 18 controls match baseline. UI clone remains running/offline. The user's **Secblitz-W11-Test** was never restarted, modified or sent any input/guest command during this final phase.

### v0.3 pause-fixed final retest - PASS (current)

The subsequent narrow fix is validated on **Secblitz-W11-UI-Test only**. The inherited-console-stdin/captured-output regression now exits 0 in 647 ms without a prompt; closed stdin exits 0 in 41 ms. **83 library + 41 CLI native tests pass.** The exact production installer passes its full lifecycle: default desktop, opt-out, silent no-launch, fresh monitor opt-in, 18/19 report, running upgrade/resume, expected unsafe-uninstall rejection and successful retry. Optional-monitor installation completes in about 5.85 seconds.

Exact rebuilt GUI smoke also passes: Italian default checkboxes and Finish launch verified, installed hash matches, guide waits after scanning without changes, and uninstall returns 0. All 18 baseline values and original real-journal hashes remain identical. No app, service or shortcut remains. Existing selected-mutation evidence is retained from the earlier candidate; unchanged backend fixtures were not repeated.

Promoted current artifacts/checksums in `dist`: executable `5762aa166c789523e663b3eeb98867dc9f8ef6f5fc3ea1aa636e09933801d8d1`; installer `db4cdd4a5c824626fb91b43adf5b06eb454463b89114bd0ef60740324b711195`. Verified v0.2.0 archive: `dist/archive/0.2.0/`. [windows-v030-results.md](windows-v030-results.md) now starts with the authoritative PASS section and exact screenshots. Original-nonadmin broker and Windows 10 remain unvalidated.

Only the UI clone was restarted before this retest to avoid its evaluation-license shutdown interval. The user's original VM was never operated on. UI clone is left running, all NICs disabled, with no shared folders.
