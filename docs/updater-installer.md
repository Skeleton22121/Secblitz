# Installer-managed automatic updates

## Installed behavior

Setup offers **Keep Secblitz up to date automatically (checks every hour)**,
selected on the first installation, including silent installation without a
`/TASKS` override. The checkbox is localized in English, Spanish, French,
German, Portuguese and Italian. The monitor remains initially unchecked;
desktop and guided-launch behavior retain their existing defaults.

The local root task `\SecblitzUpdate` runs as `S-1-5-18` (SYSTEM), highest
privilege, with precisely one executable action:

```text
Command:          "C:\Program Files\Secblitz\secblitz.exe"
Arguments:        update check
WorkingDirectory: C:\Program Files\Secblitz
```

The path is resolved from the native Program Files known folder, not an
environment variable. The task has one time trigger starting **one hour after
registration**, repeating indefinitely at `PT1H`. Multiple instances are
ignored; execution is limited to one hour. It may run on battery. Missed starts
are not immediately replayed. Setup never starts the task, calls the updater,
or contacts the feed. An unconfigured build can still register the task; the
core returns `NotConfigured` without making network requests.

Task registration atomically applies a protected DACL: SYSTEM and
Administrators have full control; Users have read access, without execute,
write, deletion, or ACL-modification rights. Maintenance imports the inbox
ScheduledTasks module using an absolute path, with module autoloading disabled.
It uses the local Task Scheduler COM API to supply the DACL at registration.
Setup's existing clean environment applies to that import as well.

## Preference and upgrade contract

The 64-bit registry DWORD
`HKLM\Software\Secblitz\AutoUpdatesEnabled` records `1` or `0`. Maintenance
validates an existing key's owner and write permissions before using it, and
protects the key with SYSTEM/Administrators full control and Users read access.
Only the updater checkbox uses this stored preference. `UsePreviousTasks=no`
remains in effect for the existing desktop and monitor choices.

| Installation path | Updater outcome |
| --- | --- |
| First install, normal defaults or silent without `/TASKS` | Enabled; first check in one hour |
| First install with `/TASKS=""` | Disabled; explicit opt-out recorded |
| Interactive upgrade with checkbox selected/cleared | Enable/disable respectively |
| Silent upgrade with unchecked task and saved preference | Preserve that preference |
| Any elevated upgrade with `/SECBLITZUPDATE=1` | Preserve preference regardless of task selection |
| Opted-out upgrade without task overrides | Remains opted out |
| Uninstall | Remove only the owned task; retain a disabled preference |

The marker is a behavior selector, not an authorization mechanism: Setup still
requires administrator elevation and all path, service, registry and task
ownership checks. New verified workers should invoke:

```text
/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /TASKS="" /SECBLITZUPDATE=1
```

For compatibility, an existing silent worker using `/TASKS=""` without the
marker also preserves the saved choice. Use interactive Setup to explicitly
disable an already-enabled updater; an empty silent task list is not a removal
request during upgrade. Uninstall retains the disabled value, so a later
reinstall honors it; selecting the checkbox enables updates again.

Before installation changes files or stops a monitor, maintenance checks any
existing same-name task. It requires the exact root task path and complete
action/principal XML subtrees, plus a trusted owner and non-writable user ACL.
The only principal serialization variation accepted is Task Scheduler omitting
`ServiceAccount` for SYSTEM. Additional actions, alternate commands/arguments,
working directories or principals fail closed. Creation uses create-only mode;
replacement/deletion rechecks ownership. A name collision fails Setup or
Uninstall without modifying the foreign task. Task Scheduler has no atomic
compare-and-swap API; administrators/SYSTEM remain trusted against concurrent
replacement. Uninstall performs this validation before service removal.

Post-install failures remain latched through `GetCustomSetupExitCode` (20),
suppress the guided launch and success page, and are not reported as success.
Pre-install rejection occurs before file replacement. The Inno named
`Global\SecblitzSetup` mutex serializes competing Setup processes.

## Core handoff

The scheduled command is exactly `secblitz.exe update check`, with no URL,
installer path or user-controlled parameters. CLI dispatch must treat it like
the service command: return its result without interactive prompts or a pause.
When the result is `WorkerStarted`, the parent must exit promptly to release
the installed executable. Core owns signed-manifest verification, installer
verification, protected staging, a copied worker outside the application
directory, re-verification before execution, engine locking, status reporting
and handling unsuccessful installer exit codes. Installer integration does
not implement or bypass those checks. See [the core contract](update-contract.md).

## Release build handoff

`scripts/build-release.ps1` accepts optional `-UpdateOrigin` and
`-SigningKeyPath`. Without an origin override, it reads
`assets/update-origin.txt`. A nonempty origin must be HTTPS and contain no
credentials, path beyond `/`, query or fragment. The script passes the
normalized origin through the compile-time `SECBLITZ_UPDATE_ORIGIN` variable
and restores the previous build environment afterward. The core pins
`assets/update-public-key.hex` through `include_str!`; configured builds reject
an empty, malformed or all-zero public-key asset. Runtime environment variables
and task arguments cannot change trust configuration.

After Inno has finalized the installer and optional Authenticode verification
has completed, `-SigningKeyPath` invokes the hosting-owned signer:

```text
python scripts/sign-release.py --key <external-private-key.pem>
  --public-key assets/update-public-key.hex --version <X.Y.Z>
  --installer dist/secblitz-<X.Y.Z>-windows-x64-setup.exe
  --output dist/stable.json
```

The secret must be outside the repository. The signer checks that its key
matches the pinned public key and signs the final installer hash/size. Signing
failure fails the release build. Authenticode and feed signing remain separate
options. The build does not upload artifacts. Hosting publishes `stable.json`
at `/releases/stable.json` and the unchanged installer under `/downloads/` on
the configured origin. No endpoint is invented when hosting is unconfigured.

## Verification

Local source check:

```sh
python3 installer/check-locales.py
```

On a **disposable elevated Windows runner with no existing installation, task
or updater preference**, run:

```powershell
.\installer\test-lifecycle.ps1 -SetupPath C:\staging\secblitz-<version>-windows-x64-setup.exe
```

The lifecycle script checks default registration, exact command/principal,
hourly repetition, delayed start, task ACL, opted-in and opted-out silent
upgrade retention (including the marker), foreign command/arguments/directory/
principal/extra-action and writable-ACL rejection for both install and
uninstall, owned-task removal, and the existing
monitor/shortcut/journal lifecycle. It never manually starts the updater.

Before shipping, isolated interactive checks must also verify all six localized
checkboxes, explicitly clearing/reselecting the checkbox across upgrades,
Users being unable to modify/delete/run the task, concurrent Setup rejection,
and a configured end-to-end signed update with correct failure status when
Setup fails. End-to-end network tests belong on that isolated runner after the
core and hosting work is ready. The user's VM is not part of these checks.
