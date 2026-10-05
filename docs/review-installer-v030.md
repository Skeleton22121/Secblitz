# v0.3.0 installer review - reviewer 4 of 4

Date: 2026-10-02. Scope: `installer/*`, `scripts/build-release.ps1`,
`.github/workflows/windows.yml`, and this report. Application entry points and
manifest were inspected read-only for integration. No VM/guest operations,
installation, service operations, or security-setting changes were performed.

## Result

The inspected installer source satisfies the requested default-on desktop
shortcut and default-on finish-page guided launch, with independent opt-outs.
The optional monitor remains unchecked. Native compilation and interactive/token
acceptance of the actual v0.3.0 package remain the native agent's responsibility.

| Contract | Review result |
| --- | --- |
| Desktop default and opt-out | `desktopicon` has no `unchecked` flag; the common-desktop icon is conditional on that task. `UsePreviousTasks=no` keeps defaults deterministic across installs. |
| Shortcut target and permissions | Desktop and Start Menu shortcuts target the fixed `{app}\secblitz.exe`, with no command arguments. Admin-mode Setup owns the common shortcut. Maintenance grants ordinary Users read/execute on the protected application root and executable. |
| Finish launch | One `[Run]` entry specifies the fixed executable and separate `Parameters: "guide"`; it is not a shell command. `postinstall` without `unchecked` selects the finish checkbox by default. |
| Silent behavior and user context | `skipifsilent` prevents the launch during silent setup; `runasoriginaluser` uses the original user context when Setup was initially unelevated. It cannot de-elevate an already-elevated invocation. |
| Failure gate | `CanLaunchSecblitz` returns `not PostInstallFailed`. The global Boolean's initial false value is acceptable: postinstall launch occurs after post-install maintenance, which first latches true and clears only after Secure, requested monitor installation, and requested resume succeed. A failure returns exit 20 and skips the finish page. |
| Guided application entry | `main.rs` defaults to `guide`; explicit `guide` follows the same path. Terminal validation precedes elevation and engine access, so redirected/non-TTY invocation rejects promptly rather than waiting for menu input. The manifest is `asInvoker`; the application can request initial UAC for protected-state access. Guided fixes require selection and confirmation; initial UAC is not apply consent. This is source inspection, not token-level runtime evidence. |
| Task CLI semantics | Inno `/TASKS` replaces the task selection rather than merging defaults. `/TASKS=monitor` selects only monitor; `/TASKS=""` selects neither. In a PowerShell `Start-Process -ArgumentList` array, `'/TASKS=""'` retains the literal double quotes needed in the joined Windows command line. `/TASKS=''` is not the documented Windows quoting form. |
| Uninstall ownership/retention | Inno's installation log owns shortcut and binary removal. There are no `[InstallDelete]` or `[UninstallDelete]` directives. `RemoveMonitor` validates/stops/unregisters the owned service without deleting report or journal data. Existing lifecycle assertions cover retained reports, journal hashes and unrelated files. |
| Version identity | `cargo metadata --locked --no-deps` reports exactly one `secblitz` package at `0.3.0`; CLI version comes from `CARGO_PKG_VERSION`. The release script derives names and `/DAppVersion` from that package. It now also requires exact `secblitz <version>` output from the copied release executable before packaging. |

## Corrections and coverage added

1. Release packaging previously checked only that `--version` exited successfully.
   It now rejects ambiguous package metadata and a binary/metadata version mismatch.
2. CI now executes `installer/check-locales.py`, which previously existed without
   a workflow invocation. The check covers all **six languages × four keys**
   (`DesktopIcon`, `LaunchSecblitz`, `Monitor`, `Failed`), task/launch contracts,
   and the bundled Italian resource path.
3. Source regression checks now also protect the post-install latch ordering,
   failure finish-page suppression, task-default reset, and absence of explicit
   install/uninstall deletion sections. **39 mutations** are rejected.
4. The native lifecycle test now includes a fresh `/TASKS=""` installation and
   uninstall, asserting no desktop icon, no monitor, no surviving app process,
   and unchanged journal data. Existing default and monitor-only cases remain.
5. Packaging documentation now identifies v0.3.0 computed artifact names and the
   existing guided CLI, explains initial application UAC and empty-list quoting,
   and distinguishes historical v0.2.0 acceptance from pending v0.3.0 acceptance.

## Checks executed here

- **PASS:** `python3 installer/check-locales.py` - six languages, four complete
  translated custom messages each, guided launch/shortcut contracts, 39 rejected
  regressions.
- **PASS:** locked Cargo metadata assertion - exactly one `secblitz` package,
  version exactly `0.3.0`.
- **Unavailable:** neither `pwsh` nor `powershell` is on this Linux host's PATH.
  PowerShell parser checks and Windows ACL/path fixtures were not run. CI retains
  parser validation and Windows PowerShell 5.1 fixture steps.
- **Deferred:** ISCC compilation, executable version validation on the native
  artifact, setup exit harnesses, lifecycle execution, interactive finish-page
  opt-outs, original-user token checks, and report/shortcut preservation on an
  actual Windows installation. The lifecycle process snapshot cannot detect a
  short-lived UI launch; source checks and interactive native acceptance are
  complementary evidence.

No UI, engine, or action implementation files were modified by this reviewer.

Reference: [Inno Setup command-line parameters](https://jrsoftware.org/ishelp/topic_setupcmdline.htm)
documents `/TASKS` replacement versus `/MERGETASKS` merging.
