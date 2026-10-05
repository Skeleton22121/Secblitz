# Explicit action API handoff

Integration: add `pub mod actions;` in `src/lib.rs` and wire only an
explicit user selection to `actions::run(Action)`. Never include these actions in
automatic hardening, assessment, or monitor scans. No Cargo/main/UI edits belong
to this change.

Public contract: `Action::{UpdateDefender, QuickScan, StartMonitoring,
OpenWindowsUpdate, OpenWindowsSecurity, OpenSignInSettings,
OpenEncryptionSettings}`; `run(Action) -> anyhow::Result<ActionResult>`;
`ActionResult { status: String, detail: String }` implements Serialize.

Service integration: `service::start() -> anyhow::Result<()>`, to be wired by
the main owner as `service start`. StartMonitoring installs only if absent,
then starts and waits for SCM Running. Running does not attest report health.

Narrow ownership exception: `src/platform.rs` and `src/platform/windows.rs`
expose `platform::support_action(id: &str) -> Result<()>` for exactly
`defender_update` / `defender_quickscan`, reusing the existing private runner.
The support script is embedded from `src/actions/defender.ps1`; it is not a
general script-execution API.

Caller must show network consent before UpdateDefender, explain scanning may
remediate threats according to existing Defender settings, and run blocking work
off the UI thread with progress. Update timeout is 120 seconds; scan timeout is
15 minutes. On timeout/error, Defender work may continue: do not claim rollback,
completion, or a clean machine. No action is automatically enabled/selected.

## Behavior and integration details

- `opened`: ShellExecuteW accepted one of the four fixed documented Settings
  URIs (`windowsupdate`, `windowsdefender`, `signinoptions`, `deviceencryption`).
  Windows edition/device availability and page visibility are not verified.
  Settings open in the calling user's context; no impersonation or elevation
  occurs here. The native boundary now rejects an elevated token: keep settings
  actions in the **non-elevated interactive desktop process**, not the elevated
  action worker. URI allowlisting alone does not prevent an HKCU protocol-handler
  override from executing with the caller's privileges.
- `returned`: Update-MpSignature or Start-MpScan returned without a command
  error. Neither means “fixed”, current signatures, completed scan, or no threats.
- `running`: SCM reported Running after service configuration, registration
  owner/DACL, executable owner/DACL, directory ACL and path validation. Already
  Running is idempotent after validation; StartPending waits; other pending or
  paused states fail. The wait has a 30-second deadline. Native Windows calls
  still depend on OS I/O completing. A failed start retains the installation.
- The existing installer configures automatic startup on future boots. Present
  this persistence before selecting StartMonitoring. `service::start` alone
  never installs or changes the startup type. This patch adds no stop command.
- Both Defender actions require an already elevated Windows x64 caller. The
  current active provider must be exactly the recognized Defender registration,
  and AMRunningMode must be Normal with service/antivirus flags true. Domain,
  MDM/cloud enrollment, relevant Defender policy, local policy artifacts, or
  unreadable authority/provider status veto the action. Tamper protection stays
  enabled if enabled; no Defender preferences or source override are written.
- Support actions reuse the existing clean-environment, native-Windows-directory
  inbox PowerShell runner, NoProfile, child job and output bound. The compiled
  backend bootstrap/helpers are reused before its dispatcher; an exact unique
  delimiter check fails closed if that boundary changes. Coordinate platform
  refactors with `platform::support_script` and its isolation test.
- Scan uses the documented synchronous `Start-MpScan -ScanType QuickScan`, not
  an undocumented asynchronous WMI contract or a detached PowerShell job.
  Microsoft documents [Start-MpScan](https://learn.microsoft.com/en-us/powershell/module/defender/start-mpscan)
  as starting a scan and separately documents `-AsJob`; command return alone is
  deliberately not treated as verified completion.

## Verification

- Linux library suite: 74 tests passed.
- Isolated actions-module harness: 2 tests passed (exact URI mapping and all
  seven actions rejecting unsupported platforms). The harness lives under
  `/tmp/opencode/secblitz-actions-check`; it avoids changing `src/lib.rs` ownership.
- `src/actions/defender_tests.ps1`: 75 host-only mocked gate cases passed,
  including unknown IDs, management/probe failures, inactive/passive providers,
  protected-but-active Defender, and command failure without acknowledgement.
- Windows x64 GNU check and linked test builds passed for both the library and
  isolated actions harness. The library includes native registration-security
  rejection tests; those tests were cross-compiled, not executed on Windows.
- No guest modifications or live Defender/service actions were performed.

## Independent reviewer 3 of 4 - 2026-10-02

### Fix and integration handoff

- Fixed elevated Settings protocol dispatch in `src/actions/windows.rs`. The
  token check is enforced at the native boundary; only the four literal Settings
  URIs pass validation. No generic executable, user arguments, URI suffixes or
  alternate schemes are accepted. `opened` still means dispatch accepted only.
- **Main/UI owner:** route Settings requests through a non-elevated interactive
  caller. Existing elevated guide/worker calls now return an explicit error
  instead of resolving a user-writable protocol handler with elevated rights.
  This reviewer did not change engine, UI, i18n, or guest state.
- Extracted monitor-start configuration validation for native regression tests.
  Behavior remains exact quoted installed path plus `service run`, LocalService,
  and OWN_PROCESS only; extra arguments, another executable/account, shared or
  interactive process flags are rejected. Registration and filesystem owner/DACL
  validation precede even the already-Running success path.

### Review conclusions

- Actual support script is `src/actions/defender.ps1`, not
  `src/platform/support.ps1`. Both entry points validate the two IDs before script
  interpolation. The compiled backend dispatcher is excluded; support operations
  cannot fall through to a reversible control write.
- `defender.support` selects Defender policy/RSOP scope, not the helper's firewall
  fallback. Configured current/provider Defender policy vetoes execution; schema
  defaults and unrelated firewall state are not broad vetoes. Native MDM probe,
  domain/cloud evidence, provider identity and Normal/active Defender checks fail
  closed on unknowns. Tamper protection is never disabled.
- Inbox PowerShell path and working directory come from the native Windows
  directory, with a cleared environment and trusted module paths. The stdin gate
  is released only after job assignment. Failure closes/kills the child; bounded
  output and 120/900-second deadlines remain. This cannot cancel an actual scan
  already executing in Defender's separate service: timeout means outcome unknown,
  not scan canceled or rolled back.
- Update success does not verify latest signatures. Quick-scan command return
  does not verify completion or a clean machine. The caller must explain that
  scanning can remediate under existing Defender settings before consent.
- Installer and service agree on the fixed Program Files/Secblitz executable and
  shared Users RX ACL. Install configures LocalService with only
  SeChangeNotifyPrivilege, sets automatic startup, and leaves the service stopped;
  StartMonitoring installs only if absent and then explicitly starts it. Startup
  never repairs/replaces a mismatched registration. Failed startup retains the
  installation. `running` reports SCM state, not report freshness or health.

### Verification performed in this review

- `source /tmp/opencode/secblitz-cross-env.sh && cargo test --lib`: **79 passed**,
  including all action variants failing on unsupported hosts, closed URI/token
  validation, support-ID isolation, and status serialization.
- `/tmp/opencode/secblitz-powershell/pwsh -NoLogo -NoProfile -File
  src/actions/defender_tests.ps1`: **120 mocked gate cases + 3 production
  policy-scope cases passed**. Added cloud/OMADM, unreadable registry/OS/status,
  unsupported OS, duplicate/malformed provider cases and actual policy-area tests.
- `cargo test --lib --target x86_64-pc-windows-gnu --no-run` using the same cross
  environment: **linked successfully**. Native direct-platform rejection tests,
  service command/account/process-flag tests and registration ACL tests are built,
  not executed here.
- Main reviewer owns Windows interactive Settings checks (standard user and
  elevated rejection), actual inbox Defender command compatibility and SCM
  lifecycle/timeout checks. No Windows runtime result is claimed by this review.
