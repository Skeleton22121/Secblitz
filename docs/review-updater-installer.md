# Installer and updater review: reviewer 3 of 4

## Scope and result

Reviewed and changed only `installer/setup.iss`, `installer/maintenance.ps1`,
`installer/check-locales.py`, `installer/test-lifecycle.ps1`,
`scripts/build-release.ps1`, and this report. Core updater, CLI, website, and trust
assets were not modified. Native Windows execution belongs to the runtime agent.

Source review found and fixed incomplete task ownership validation, insufficient
Users task rights, lost desktop opt-out on default upgrades, and locale-dependent
trigger date formatting. Release approval remains blocked on the native checks
below, not on the passing host checks.

## Fixes

- `Get-OwnedUpdate` now requires the exact root task path, an enabled task, and an
  existing executable in the previously pinned and validated installation tree.
  Action validation accepts only the exact installed executable with optional
  surrounding quotes, exact `update check` arguments, and exact working directory.
  Extra actions, principals, arguments, or execution elements fail closed.
- SYSTEM, HighestAvailable, and ServiceAccount remain mandatory. Omitted
  ServiceAccount XML is accepted only with the otherwise exact SYSTEM principal,
  matching Task Scheduler's persisted representation.
- Ownership validation now checks the operational settings and exactly one hourly
  time trigger. Extra trigger types, expiration, altered repetition, disabled
  state, changed execution limits, and unknown settings are rejected. Known omitted
  settings retain the Task Scheduler schema defaults. Trigger timestamps use the
  invariant calendar and format.
- Task registration and `SetSecurityDescriptor` both use SYSTEM/Administrators
  full control and Users read/execute, with no Users write/delete/ACL rights.
  Existing read-only Users ACLs remain acceptable for migration. Ownership checks
  require trusted ownership, a non-null DACL, safe ACEs, and privileged full access.
- Desktop selection is saved independently from updater selection. A default
  upgrade retains a saved desktop opt-out; marked automatic upgrades retain the
  saved selection despite their empty optional-task list.
- The maintenance environment now includes an absolute inbox `ComSpec`, alongside
  the existing allowlisted PATH, PATHEXT, module directory, and temporary paths.
  Execution still uses explicit executable paths, not shell lookup.
- Native lifecycle fixtures now preserve the owned hourly trigger so each foreign
  task mutation isolates its intended mismatch. Added disabled-task, interval,
  extra-trigger, and settings-conflict cases.
- The release build runs the locale checker and host-compatible ownership fixtures
  before compiling. Feed signer arguments match `scripts/sign-release.py`.

## Reviewed contracts

- Fresh installation defaults to desktop shortcut and hourly automatic updates;
  the optional read-only monitor remains unchecked. All six original locales
  provide the five required custom messages.
- `Prepare` validates the protected tree, monitor identity, and updater ownership
  before creating/changing the application directory or stopping the monitor.
  A foreign same-name task raises an error rather than being overwritten.
- Uninstall invokes the same ownership checks before deleting the root updater
  task. Only ERROR_FILE_NOT_FOUND means no task; other scheduler failures propagate.
  A missing task causes no task deletion. The updater preference is set to disabled.
- Existing monitor installation is preserved even when its checkbox is clear;
  a previously running monitor is resumed after upgrade or rollback. New monitor
  installation remains stopped until the normal service start path.
- Update opt-out is persisted in a protected HKLM key. Marked automatic upgrades
  and legacy silent upgrades with a saved preference preserve that preference.
  This compatibility rule intentionally means an empty silent task list alone is
  not an explicit updater opt-out for an existing installation.
- `/SECBLITZUPDATE=1` is not SYSTEM authentication. Any elevated Setup caller can
  supply it. It only preserves preferences and does not skip path/ACL/task checks.
  An unelevated caller still encounters Setup's administrator requirement.
- Ownership trust includes the protected installed path and exact task contract.
  Authenticode is optional for local builds. Current app resources contain no
  VERSIONINFO identity metadata, so this review does not claim publisher metadata
  or a signature independently authenticates an existing local executable.
- `Global\SecblitzSetup` coordinates installer instances. Pre-creation by another
  user can deny setup availability; the mutex is not an elevation boundary.
- Inbox modules, including ScheduledTasks, are imported by absolute manifest path.
  No Task Scheduler backing-file inspection or localized shell output parsing is
  used for ownership decisions.
- Cargo version was observed as `0.4.0`; the checked-in origin was observed as
  `https://beacons.lol`. The main agent owns those inputs.
- Release packaging selects the current Cargo-version executable and installer
  explicitly. No wildcard signs old dist files. Ed25519 signing occurs after the
  final Inno/optional Authenticode work and uses the pinned public key. The private
  key must remain outside the repository; its path is not printed by the build
  script's normal progress output. Existing older dist artifacts are not deleted
  or included in the new manifest and must not be glob-published as a release.

## Host verification

Using `/tmp/opencode/secblitz-powershell/pwsh`:

- Parsed `maintenance.ps1`, `test-lifecycle.ps1`, and `build-release.ps1` without
  syntax errors.
- Ran `installer/test-lifecycle.ps1 -OwnershipFixtures`: three valid ownership XML
  fixtures accepted and 21 altered fixtures rejected. These extract and execute
  the actual validator from maintenance source, not a duplicate implementation.
- Ran `python3 installer/check-locales.py`: six languages, five complete messages
  per language, and 48 rejected source regressions.

The host has `python3`, not `python`; the Windows release script retains its
existing `python` command convention.

## Release blockers and runtime handoff

1. Compile the final installer with Inno Setup on Windows. Verify its embedded
   PowerShell and saved desktop checkbox behavior, including a marked automatic
   upgrade. Host PowerShell parsing does not compile Pascal Script.
2. Run the native lifecycle suite using the final installer. Confirm real persisted
   task XML passes validation, especially omitted defaults and ServiceAccount;
   confirm Users RX and no write, SYSTEM/highest execution, root task path, delayed
   first run, and hourly repetition. Scheduler serialization compatibility is not
   established by synthetic host fixtures.
3. Confirm all foreign-task cases fail setup before app changes and fail uninstall
   without modifying the foreign task or its ACL. Also verify absent-task uninstall
   and a same-name task in a different scheduler folder remain harmless.
4. Exercise monitor upgrade/resume, saved update opt-out, desktop opt-out, all six
   locales, and a non-Gregorian Windows user locale. Preserve reports and journals.
5. The main agent must complete the silent CLI/update-command behavior, compile the
   final `0.4.0` binary with the final origin/key, and sign only the final installer
   bytes. No release build, native scheduled execution, or signing was performed
   in this review. Verify the resulting manifest against the final artifact before
   publication.
