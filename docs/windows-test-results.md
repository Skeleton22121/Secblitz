# Windows clone integration results

## Final acceptance - PASS (current; supersedes earlier blockers)

Only the authorized `Secblitz-W11-Test` clone was used. Final executable SHA-256: **`f126d1ee17901a2a80145ef7c729c5c4fcb8e71b1ab996920e9a55d0a754e25c`**. No application/installer source was edited by the runtime tester.

### Native runtime acceptance

- Rebuilt Windows test executables: **40 library + 13 CLI tests passed**, exit 0. English and Spanish help exit 0.
- Captured all 12 controls independently at every stage, including concrete firewall settings and exact UAC DWORD kind/bytes. Fixture originals: Public Enabled=True, inbound=NotConfigured, consent DWORD=5 (`BQAAAA==` little-endian bytes).
- Fixtures: Public Enabled=False, inbound=Allow, UAC consent=0. Audit correctly identified eligible firewall/UAC repairs; Defender remained tamper protected and was not modified.
- **Apply succeeded for five controls**:

  | Control | Before-image | Applied | Tool revert |
  | --- | --- | --- | --- |
  | Domain default inbound | NotConfigured | Block | NotConfigured |
  | Private default inbound | NotConfigured | Block | NotConfigured |
  | Public enabled | False | True | False |
  | Public default inbound | Allow | Block | Allow |
  | UAC administrator consent | DWORD 0 | DWORD 5 | DWORD 0 |

- Repeated apply retained the original before-images; all 12 independent values were identical to the first applied capture. Tool revert restored all five before-images. Repeated revert made no further changes. Before-fixture and both post-revert captures match byte-for-byte.
- Second apply succeeded. Induced consent drift **5 → 2** was preserved and reported as `conflict`; the other four controls were restored. After the tester explicitly returned consent to the recorded applied value 5, retry revert restored its before-image 0. The final retry capture exactly matches the fixture-before capture.
- Final cleanup restored the original Public settings and exact original consent DWORD. All 12 final controls are byte-for-byte identical to the initial baseline, including tamper protection true. The four Defender preferences, EnableLUA, and Domain/Private enabled state were not changed. **This does not claim all 12 controls were mutated.**
- Audit/apply/idempotent apply/revert/repeated revert/drift apply/conflict revert/retry each returned **exit 2 with valid JSON and empty stderr**, because review/advisory findings remain (and the induced conflict requires review). Exit 2 is expected report semantics, not an execution failure. History exited 0 and all three retained transactions, including the older recovered failure, are **reverted**; none is pending.
- Runtime evidence: `/tmp/opencode/secblitz-acceptance/` and `secblitz-acceptance.zip`. Exact stage statuses, before/after captures, reports, original bytes and unit-test output are preserved there.

### Exact final package acceptance

- Real Inno Setup **6.7.3** compiled the unchanged latest installer source with this exact release: **exit 0**.
- Fresh optional-monitor installation: **exit 0**, initially Stopped. Installed executable hash exactly matched the release above.
- SCM start produced a new schema-1 report: **12 observations, 13 findings, incomplete=false**, service Running. Individual unavailable assessments remain explicit; a complete scan is not a compliance verdict.
- Uninstall **while monitor running**: **exit 0**. Executable and service registration removed; monitor report retained.
- **Actual transaction journal preservation passed**: hashes of every file under ProgramData/Secblitz before install and after uninstall match exactly. These include the real reverted apply/drift transactions, not just a synthetic sentinel.
- All 12 independent preferences after the installer/service smoke match the original pre-fixture baseline. No application binary or service remains installed; retained reports, historical unrelated-file fixtures and real journals remain intentionally.
- Package evidence: `/tmp/opencode/secblitz-package-acceptance/` and `secblitz-package-acceptance.zip`, including compile/install/uninstall exit codes, scan summary, installed hash, journal hashes and final state.
- Earlier Wave 3 full lifecycle additionally verified running upgrade/resume and locked-file rejection/retry against the same installer source. The exact final package was separately smoke-tested as above; no fresh upgrade test is claimed for its new executable bytes.

### Final artifacts / limits

The exact guest-tested files replaced `dist`, and `sha256sum -c dist/SHA256SUMS` (run from the proper directory) verified both:

```text
f126d1ee17901a2a80145ef7c729c5c4fcb8e71b1ab996920e9a55d0a754e25c  secblitz.exe
82302c761df118c307110759dbc7d1d0db80a804dfddd312ca8a9eb96e25a04e  secblitz-0.1.0-windows-x64-setup.exe
```

Final VM: running, offline (all 8 NICs disabled), no shared folders, no monitor service or installed application executable, no outstanding fixture changes or unresolved transaction. Original/other VMs were not modified. The expired evaluation license can still interrupt long sessions. **Windows 10 was not tested.** These are unsigned GNU-cross-built development artifacts with recorded Windows 11 runtime/package evidence, not signed/MSVC release-provenance evidence.

## Previous targeted runtime - native firewall setter blocker (resolved above)

Tested the freshly supplied scoped-policy/localization release, SHA-256 `013de54c3fb1c06629e00e54ea0212040b0ff32009dfeb287b8fc32aa1fc9865`, only in `Secblitz-W11-Test`. Rebuilt Windows test executables with `cargo test --locked --target x86_64-pc-windows-gnu --no-run`: **40 library + 13 CLI tests pass**, exit 0. English and Spanish help exit 0.

Native audit now correctly establishes **firewall eligibility**, UAC-consent eligibility when explicitly zero, and preserves nonzero UAC baseline. Defender remains tamper protected and ineligible for mutation. No gate was bypassed.

### Blocking failure and successful recovery

1. Recorded all 12 preferences independently, including exact DWORD kind and four little-endian consent bytes (base64), and concrete firewall profile strings.
2. Fixtures: Public firewall Enabled=False, inbound=Allow, UAC consent=0. Audit returned valid JSON, exit 2.
3. **Apply exits 1** at `firewall.public.enabled`. Native error:

   ```text
   Cannot process argument transformation on parameter 'Enabled'. Cannot convert
   value "True" to type "Microsoft.PowerShell.Cmdletization.GeneratedTypes.NetSecurity.GpoBoolean".
   Invalid cast from 'System.Boolean' to '...GpoBoolean'.
   ```

   Backend splats a Boolean into the NetSecurity enum-valued Enabled parameter. Earlier Domain/Private inbound writes had already changed NotConfigured to Block; the engine correctly retained a pending transaction. This is a setter/type-conversion defect, not a management veto.
4. Put the never-written Public enabled fixture back to its recorded before-image (False), then ran **tool revert**. It reported Public enabled already original, **restored Domain and Private inbound to NotConfigured**, and closed the transaction. History now reports **reverted**, not pending. Revert returned review exit 2 with valid JSON.
5. Restored every original firewall field and exact original consent DWORD in cleanup; independent all-12-control capture is byte-for-byte identical to the original capture. Tamper protection remained true. The expired evaluation guest shut down before recovery; only this clone was restarted and recovery then completed.

This proves actual partial firewall mutation and tool rollback, but **the full apply/idempotence/revert/drift suite did not run to completion**. UAC mutation was not reached. No Rust/installer code was edited. Full-package rebuild/promotion is paused at this reproducible release blocker; the prior passing Wave 3 artifacts in `dist` were left intact and `sha256sum -c SHA256SUMS` passes. Those hashes do not identify this failing candidate.

Evidence: `/tmp/opencode/secblitz-final-runtime-recovered/` (`apply.err`, `recovery-revert.out`, `recovered-history.out`, `original.json`, `final-recovered.json`, `status.txt`, unit outputs). Guest scripts: `/tmp/opencode/secblitz-final-runtime.ps1` and `secblitz-final-recovery.ps1`. The real reverted transaction is retained in the original journal. No service was installed by this run; original/other VMs unchanged. Windows 10 remains untested.

## Wave 3 - earlier passing installer lifecycle (supersedes Wave 2 blockers)

Agent 3 exclusively owned guest installation testing for this wave. Only
`Secblitz-W11-Test` was used; the original and other VMs were not modified.

### Fixes verified against the actual installer

* Clean maintenance environment now includes fixed
  `PATHEXT=.COM;.EXE;.BAT;.CMD`. Native monitor commands additionally use explicit
  `ProcessStartInfo` with `UseShellExecute=false`, initialized exit status,
  bounded wait and checked child `ExitCode`, rather than ambient `$LASTEXITCODE`.
  A regression executable returned **0 and 37 correctly with PATHEXT absent**.
* Uninstall passes the exact active Inno `.dat` path derived from
  `{uninstallexe}`. Only that direct-child `uninsNNN.dat` uses metadata-only access
  while Inno holds it exclusively. Reparse, hard-link, regular-file, owner and
  DACL checks still run. App/monitor/other files and directories retain normal
  pinning. A different locked `unins001.dat` is rejected, proving there is no
  wildcard Inno-file exemption.
* Silent uninstall failures are logged and return failure without displaying
  the custom modal dialog. Exceptions in initialization also fail closed.

### Exact final repository lifecycle results

Ran the updated `installer/test-lifecycle.ps1` against the real compiled setup:

| Stage | Result |
| --- | --- |
| Fresh optional-monitor setup (`/TASKS=monitor`) | **exit 0** |
| Fresh service state | **Stopped**, expected LocalService identity and quoted image path |
| SCM start and actual scan | **Running**, valid **schema-1 report** produced |
| Upgrade while running, `/TASKS=""` | **exit 0**, monitor resumed **Running** |
| `/VERYSILENT` uninstall with a different locked Inno-lookalike file, without `/SUPPRESSMSGBOXES` | **exit 1**, no modal hang; app and running service retained |
| Unlock fixture and retry uninstall while service running | **exit 0**, app executable and service registration removed |
| Preservation | Monitor report, journal sentinel and both unrelated-file fixtures retained unchanged |

The separate in-place-original-data cycle also passed: install **0**, deliberately
rejected silent uninstall **1 in 1,652 ms**, successful retry **0 in 3,479 ms**.
Original `ProgramData\Secblitz` hashes were unchanged throughout that cycle.
For the fresh-install script, original data was moved aside intact and restored;
its hash was checked before/after. No real rollback transaction existed in that
directory; its original `engine.lock` remains unchanged. The synthetic journal
sentinel is preservation evidence, not evidence of a real preference mutation.

### Checks and evidence

* Windows native tests: **40 library + 12 CLI passed**.
* Windows-target all-targets Clippy with `-D warnings`: **passed**.
* Inno Setup **6.7.3** production compile: **exit 0**.
* ACL/path/native-command regression tests: **passed**, including exact locked
  Inno metadata validation, unsafe ACL rejection and no-PATHEXT exit handling.
* Post-install error harnesses remain passing: success **0**, Secure/
  InstallMonitor/ResumeMonitor/exception failures **20**.
* Independent Defender, firewall and UAC captures (all 12 preferences plus tamper
  status) were identical before/after installer testing. Tamper protection stayed
  enabled. No management gate was changed or bypassed.
* Host evidence: `/tmp/opencode/secblitz-agent3-wave3-build-results.zip`,
  `secblitz-agent3-wave3-lifecycle-results.zip`,
  `secblitz-agent3-wave3-final-results.zip`, and
  **`secblitz-agent3-wave3-final-lifecycle-results.zip`** (final repository script).
  Final read-only handoff checks are in `secblitz-agent3-wave3-handoff.zip`.
* Guest evidence and preserved prior test files:
  `C:\Windows\Temp\SecblitzAgent3Wave3`, including `PriorApp` and
  `FinalLifecycle\PriorRetainedApp`. Original journals are back in ProgramData.

### Final artifacts and state

The exact passing guest-tested artifacts replaced `dist`'s earlier diagnostic
installer; `dist/SHA256SUMS` was updated and verified:

* `dist/secblitz.exe` -
  `a02368265da9b8675508729f9685d3b028fb40e314d1ea4dcc9862fe5c543063`
* `dist/secblitz-0.1.0-windows-x64-setup.exe` -
  `d23d3da31b3085fd601c87c9f332b6b478873ad34912f4aafc9b7697538b97b4`

Final guest has **no SecblitzMonitor registration and no installed secblitz.exe**.
The retained app directory contains `Monitor\latest.json` and the intentional
unrelated fixtures `unrelated-preservation.txt` / `unins001.dat`. Original
ProgramData contents are restored. These unsigned GNU-cross-built development
artifacts now pass the tested Windows 11 installer lifecycle; this does not
establish signed/MSVC release provenance or Windows 10 compatibility. The
managed-baseline mutation/undo limitation and expired evaluation license noted
in Wave 2 remain environment limitations, not newly claimed test successes.
The guest is returned **running**, with all eight NICs disabled and no shared
folders. No installer or service test is left running; guest runtime ownership
is released back to the coordinating agent.

## Wave 2 - historical handoff (superseded by Wave 3 above)

Tested only `Secblitz-W11-Test`, Windows 11 Enterprise Evaluation build 26200, using the newly supplied release and Windows test binaries. No repository Rust or installer source was changed by this VM testing task.

### Execution and controls

- English help and Spanish help: **exit 0**.
- Native Windows unit tests: **40 library + 12 CLI tests passed**, both exit 0. Includes actual backend read-only, native MDM API, journal link/locking, service ACL/path-pin, and localization tests.
- Ordinary `audit --json`, `apply --json`, `revert --json`: **exit 2**, valid JSON, empty stderr. The CLIXML/progress transport failure is fixed. Review findings include unavailable assessments; this is not process/JSON failure or a claim of compliance.
- Normal apply: 9 controls already at target, 3 firewall inbound controls skipped; no mutation transaction. All 12 independently captured preferences match before/apply/revert.
- Reversible fixtures: Public profile disabled and UAC administrator consent set to 0. Both are correctly read but remain **ineligible**: `Configured management/security policy: assessment only`. Fixture audit/apply/revert return 2. Independent captures prove apply/revert left both fixtures and all other controls unchanged. Manual `finally` restoration returned all 12 controls to their original values; tamper protection stayed true.
- Remaining gate evidence: `HKLM:\SOFTWARE\Microsoft\PolicyManager\providers` has 98 values across its subtree. Defender and WindowsFirewall policy subtrees have 0. No values/identifiers were disclosed or altered. The previous enrollment-key-count gate is no longer the blocker. No policy gate was bypassed; **successful real preference mutation+undo remains unverified on this managed baseline**.
- Evidence: `/tmp/opencode/secblitz-final-wave2/`, `secblitz-integration-wave2/`, `secblitz-fixtures-wave2/`.

### Service

- CLI install/status/uninstall/final status: **exit 0**.
- SCM start succeeded. Waited for a new schema-1 monitor report, rather than merely observing Running. LocalService identity and quoted image path independently verified.
- Independent all-12-control captures before scan, after scan, and after service removal are identical. Unknown/access-denied findings remain explicit under the restricted service account.
- Report evidence: `/tmp/opencode/secblitz-service-wave2/monitor-report.json`; service status and policy counts are in the same directory.

### Actual installer and reproducible blockers

- Latest `setup.iss` + `maintenance.ps1` + tested binary compiled with real Inno Setup 6.7.3: **exit 0**. Security-module import/Prepare now succeeds.
- Ran the repository's unmodified `installer/test-lifecycle.ps1` after moving only this test's prior app/data into `C:\Windows\Temp\SecblitzLifecycleWave2\PriorApp` and `PriorData`. Fresh optional-monitor setup **exits 20**, so the lifecycle script exits 1. The service registration is created despite the postinstall failure.
- **Blocker A - missing PATHEXT in cleaned environment:** reproducing the exact maintenance allowlist outside Setup yields `InstallMonitor exit=1`, error `The variable '$LASTEXITCODE' cannot be retrieved because it has not been set.` The service is nevertheless registered. The same source under the normal environment returns 0. Adding a fixed `PATHEXT=.COM;.EXE;.BAT;.CMD` only to the external diagnostic environment makes both RemoveMonitor and InstallMonitor **exit 0**. This diagnoses PowerShell native-command dispatch/exit handling; no source fix was applied here. Evidence: `/tmp/opencode/secblitz-cleanenv-wave2-after-cleanup/` and `secblitz-cleanenv-pathext-wave2/`.
- **Blocker B - uninstall pins Inno's live data file:** silent uninstall fails RemoveMonitor's recursive path pin because **`C:\Program Files\Secblitz\unins000.dat` is exclusively held by Inno**. A direct per-file open probe shows only that file blocked; after terminating the stalled owned `_unins.tmp` child it opens normally. The current error MsgBox is not suppressed, and the uninstall attempt timed out at 240 seconds. Evidence: `/tmp/opencode/secblitz-pin-wave2/`, `/tmp/opencode/secblitz-maintenance-wave2-uninstall.log`. A fix must preserve validation while accounting for Inno's own live file; this task did not bypass the check.
- Separately tested an already-installed **running monitor upgrade** with `/TASKS=""`: setup **exit 0**, monitor resumed **Running**, journal sentinel hash unchanged. Thus running-upgrade/resume passes even though fresh opt-in and full uninstall lifecycle remain blocked. Evidence: `/tmp/opencode/secblitz-upgrade-wave2/`.
- Final cleanup stopped and removed the service through the CLI (**exit 0**). Original ACL-protected journal directory restored. Lifecycle sentinel retained under `C:\Windows\Temp\SecblitzLifecycleWave2\RetainedLifecycleData`. Setup's application/uninstaller files remain in Program Files because complete uninstall is blocked; do not describe this as successful product uninstall.

### Artifacts and final VM state

- `dist/secblitz.exe`: exact guest-tested release binary copied back from the clone.
- `dist/secblitz-0.1.0-windows-x64-setup.exe`: newly compiled Wave 2 installer, replacing the old diagnostic build. **Still not a passing release installer** due to the two blockers above.
- `dist/SHA256SUMS`:
  - executable: `a02368265da9b8675508729f9685d3b028fb40e314d1ea4dcc9862fe5c543063`
  - setup: `516552ef09eabdeefda7f5a0e42a7068a0cefb9d9e2799cac317ee81bcbff5ef`
- Last observed VM state: **running**, all 8 NICs disabled, no shared folders; no SecblitzMonitor registration. All 12 controls equal the pre-test capture. Original/other VMs untouched.
- The guest unexpectedly shut down between test stages. System event 1074 explicitly reports **expired Windows evaluation licensing**, LicenseStatus 5, grace remaining 0. Restarted only this clone at `2026-10-02T04:54:38.631000000` UTC; prior shutdown occurred after approximately one hour. Long test sessions can be interrupted by this environment issue. No license/time modification attempted.
- **Windows 10 was not tested.** No raw device identifiers or credential contents are recorded here.

---

## Historical Wave 1 evidence

Only `Secblitz-W11-Test` is used. Powered-off restore point: `secblitz-pre-integration`, UUID `09567c47-0d8e-4010-838b-cea299f2ac4d`. All guest NICs disabled; no shared folders.

## Initial release blocker

The supplied release executable starts and displays English and Spanish help correctly. Actual elevated audit/apply/revert do not complete: the native backend rejects PowerShell stderr containing `#< CLIXML` serialized **progress** records despite PowerShell exit code 0. Example progress activities are `Get-MpPreference` and `Get-NetFirewallProfile -PolicyStore ActiveStore`. This is runtime evidence, not a successful JSON report. Application exit-code capture in the first harness was blank; rerun with a retained process handle is required for exact codes.

Independent native snapshots before/apply/revert: all four Defender Disable* preferences false; Domain/Private/Public firewall Enabled=True and persistent inbound=NotConfigured; EnableLUA=1; ConsentPromptBehaviorAdmin=5. Tamper protection remains true. Evidence: `/tmp/opencode/secblitz-integration-results/`.

## First unit/service/installer pass

- Windows unit executables: **20 library tests + 8 CLI tests passed**, both exit 0.
- CLI service install/status/uninstall/final-status: exit 0. SCM start succeeded; independently observed `Running`, LocalService, correctly quoted Program Files executable path. SCM stop completed within 150 seconds. Direct console `service run` returned 1 (expected dispatcher rejection outside SCM).
- Official Inno Setup 6.7.3 download from jrsoftware.org's linked GitHub release was copied into the offline guest. Authenticode `Valid`, publisher Pyrsys B.V.; installation exit 0.
- ISCC compiler exit 0. Default setup and uninstall exit 0.
- **Optional-monitor setup failure incorrectly returns exit 0**: `install-monitor.log` records `Secblitz maintenance failed: InstallMonitor, code 1` and runtime exception. No monitor was registered. This first installer contains the earlier release binary, preceding the service/installer ACL compatibility fix now visible in source. Requires rebuild and retest; exit-code failure propagation remains independently important.
- Uninstall preserved `C:\ProgramData\Secblitz\engine.lock`; no transaction JSON existed yet because backend failure prevented completion.
- Evidence: `/tmp/opencode/secblitz-extra-results/`, `/tmp/opencode/secblitz-installer-results/`.

## Reversible fixture pass

Temporarily disabled only the clone's Public firewall persistent profile and set `ConsentPromptBehaviorAdmin=0`. Defender and tamper protection were not changed. Native reads verified both fixtures. Backend eligibility probes for both returned `eligible=false`, reason **Enrollment or cloud-management evidence: assessment only**. Management evidence was not removed or bypassed.

Rebuilt application audit/apply/revert each returned **exit 1**, still blocked by CLIXML progress on stderr. Independent all-12-control captures remained identical before/after attempted apply and revert. Both fixtures were manually restored in a `finally` block. `diff` confirmed the complete original and restored capture files are identical, including tamper protection true. Evidence: `/tmp/opencode/secblitz-fixtures-results/`.

## Installer source revision retest

An intermediate in-progress setup source failed ISCC with `Unknown type 'NativeInt'`; the subsequently revised LongWord declaration compiled successfully (exit 0). The resulting updated installer fails default and optional-monitor installation in Prepare, **exit 7**. Direct maintenance diagnostic establishes `Get-Acl` is not recognized: module autoloading is disabled and `Microsoft.PowerShell.Security` is not explicitly imported. `Set-Acl` requires the same module. Evidence: `/tmp/opencode/secblitz-installer-results-v3/` and `/tmp/opencode/secblitz-maintenance-diagnostic.err`. Only the v3 `status.txt` entries describe that run; older uninstall log files were carried in the test directory and are not new v3 uninstall evidence.

## Isolated backend fix verification (no source changes)

Executed the same native Defender observation through an EncodedCommand wrapper twice: original wrapper **exit 0, stderr 420 bytes** (CLIXML progress); wrapper prefixed with `$global:ProgressPreference='SilentlyContinue'` **exit 0, stderr 0 bytes**, valid observation JSON. Setting the preference in the outer/global scope fixes the demonstrated leak; the backend script's current inner-scope assignment does not. Evidence: `/tmp/opencode/secblitz-progress-diagnostic.status` and `/tmp/opencode/secblitz-progress-global.out`.

## Latest handoff state

- Rebuilt Windows unit executables ran successfully: **27 library + 8 CLI tests**, exit 0, including Windows ACL and inbox PowerShell boundary tests. English and Spanish help both exit 0. Evidence: `/tmp/opencode/secblitz-final-results/`.
- Exact first management gate trigger: `HKLM:\SOFTWARE\Microsoft\Enrollments` exists with **33 immediate subkeys**. OMADM Accounts has 0; CloudDomainJoin JoinInfo is absent. No enrollment values or identifiers were collected, altered, or bypassed.
- All 12 independent control values are identical before/after installer testing and after manual fixture restoration. Defender tamper protection is true throughout.
- Final guest has **no SecblitzMonitor service**, no `C:\Program Files\Secblitz\secblitz.exe`, and retains `C:\ProgramData\Secblitz\engine.lock`. Inno compiler and temporary test artifacts remain in this disposable clone.
- VM is **running**, all eight NICs disabled, no shared folders. Original/other VMs were not modified.
- Compiled installer copied to `dist/secblitz-0.1.0-windows-x64-setup.exe`. **Diagnostic build, NOT a passing release:** latest default/monitor setup attempts fail Prepare with exit 7 as described above. Unsigned project installer; the Inno compiler download itself was signature-verified.
- Installer SHA-256: `4af4cd9e17d6df25d5609b855891a11ebd812483e5adc1f3153a09f7b85909f5`.
- Latest cross-built application SHA-256 at handoff: `4b0722dbc3ef6470e1a0c198d56e500ed4aaadb15e49b9590ae2c470623b7963`.
- Pending reviewer fixes/rebuild: outer/global PowerShell progress suppression; explicit security-module import in maintenance; failure exit propagation after postinstall maintenance exceptions. Then rerun full audit/apply/revert and installer optional-monitor lifecycle. Successful real mutation/revert is **not demonstrated** on this managed baseline, and retained transaction-data testing remains blocked (only the lock file exists).
- Windows 10 was not executed; Windows 11 Enterprise Evaluation build 26200 is the only tested guest OS.



- Supported `Set-MpPreference` real-time-disable request completed, but independent state remained real-time=true, disable preference=false, tamper=true. No active interactive desktop was available for a supported UI change. No bypass, service hack or exclusion was applied.
- **Completed benchmark runs: 0. Weakness counts unavailable, not zero.** The proposed later run is `servicesinfo notcolor quiet dont-check-hostname`; bundled systeminfo was excluded after source review found credential-oriented collection inside it. No standard-user fixture was created yet.
- Explicitly requested protection restoration, cleanly powered off the clone, restored the pre-assessment snapshot, restarted it, and verified all captured Defender preferences/runtime flags match baseline exactly. Guest network stayed disabled, no shares were added, and original/other VMs were untouched.
