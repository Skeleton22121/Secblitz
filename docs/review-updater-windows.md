# Independent review 2/4: native Windows updater

## Scope and result

Reviewed `src/updater/windows.rs`, with read-only inspection of its protocol,
platform, CLI, and installer dependencies. This review targets a standard-user
attacker; an already privileged administrator is outside the boundary. No guest
operations, upstream requests, or real installations were performed.

The native updater has been hardened, but Windows runtime acceptance remains
required. In particular, cross-compilation is not evidence that Windows sharing,
ACL, installer, or Task Scheduler behavior has passed.

## Fixes

* Require elevation and an enabled Administrators SID at both mutating native
  entry points. LocalSystem normally has this enabled SID. Reject non-x64 builds
  and non-local/noncanonical installed paths. Resolve Program Files using the
  native known-folder API, not environment variables.
* Independently inspect the staging root's ancestor DACLs and retain list-access
  directory handles denying delete sharing while permitting write sharing for
  legitimate child-entry mutations. The shared `state_dir()` implementation
  inspected during this review only checks ancestor ownership and retains
  attribute-only handles. Updater-owned pins close that gap without changing the
  shared helper. The root remains subject to that helper's protected,
  SYSTEM/Administrators-only inherited-tree policy; service SID access is rejected.
* Allow ordinary create-child and EA/attribute-write permissions only on
  non-strict directory ancestors, never on the protected root, staged files, or
  installed application's immediate directory. Preserve rejection of untrusted
  owner/DACL-write/delete rights, reparse points, and multi-linked files. Directory
  pins request `FILE_LIST_DIRECTORY` plus metadata/security access; payload pins
  request `GENERIC_READ` and share only reads, excluding writers and replacement.
* Clear the environment for the worker, installer, and installed-version probe.
  Supply only native Windows directory/drive values, the validated ProgramData
  parent as ProgramData/ALLUSERSPROFILE, a System32 search path, and the protected
  staging directory for TEMP/TMP. Caller COM/CLR/profiler and proxy environment
  settings are not inherited.
* Supply fixed typed installer arguments: `/VERYSILENT`, `/SUPPRESSMSGBOXES`,
  `/NORESTART`, `/SP-`, an empty `/TASKS=` value, and `/SECBLITZUPDATE=1`.
  `/TASKS=` is the argument-vector equivalent of command-line `/TASKS=""`;
  embedding literal quote characters in a typed argument is unnecessary.
  The marker selects the installer's existing saved-update-preference behavior.
* Remove forced installer termination. Wait while retaining both locks and the
  verified payload handle. An arbitrarily slow installer deliberately keeps the
  updater busy rather than allowing a competing installation or being killed
  halfway through a replacement.
* Check for a still-running fixed-path installer as well as installed application
  sessions, including an installer surviving worker failure. Compare full image
  paths: an inspectable portable UI elsewhere does not block installation.
  Uninspectable candidates remain conservatively busy; no UI is killed.
* Report `Installed` only after exit code zero, renewed installed-path trust
  inspection, and a successful fixed `--version` query matching both `secblitz`
  and the exact signed stable version. Bound the probe to 30 seconds and 257
  bytes of captured output; a failed/timed-out probe does not assert completion.

## Preserved trust and lifecycle checks

The feed is the compiled HTTPS origin plus `releases/stable.json`. The client
disallows redirects and proxies, including environment-derived proxies. This
also means configured enterprise proxies are not used. TLS validation is not
disabled. Both responses require successful status codes; manifest and payload
reads are capped, with connect/request/download deadlines.

Raw signed manifest bytes and the embedded public key are used to authenticate
the release before staging. Installer size and SHA-256 are checked before any
installer bytes are written. The worker verifies the staged manifest again and
checks installer size/hash immediately before execution. Its read handle denies
write/delete sharing through installer exit, closing the ordinary payload
replacement race. Fixed staging names and no runtime URI/path inputs are retained.

The installed executable starts a protected copy worker and returns. The worker
waits for the check's update lock, takes the engine lock without blocking, and
waits for installed sessions to leave. Both updater phases use update-then-engine
ordering. Read-only installed sessions also defer installation. The registered
monitor PID is excluded because installer maintenance owns its stop/restart.
No updater path kills sessions or requests reboot.

The worker must execute from the fixed protected worker path. Its bytes must
match the currently installed executable before installation; a retained worker
from an older build fails rather than downgrading a newer installation. Installed
image handles are dropped before launching Setup so they do not prevent file
replacement. The worker and verified installer handles remain held.

## Remaining acceptance and limitations

* Test the production SYSTEM task and an elevated Administrator token, parent
  exit/worker handoff, simultaneous checks, active installed and portable UIs,
  monitor stop/restart, and preservation of the saved scheduler preference.
* Validate the final Windows command line and real Inno Setup behavior, including
  protected TEMP extraction and empty task selection. Installer changes are owned
  by another reviewer. Its `CloseApplications=no` and `RestartApplications=no`
  settings remain important if a UI starts after the last process snapshot.
* Run successful upgrade, zero-exit/no-replacement, mismatched-version, stale
  worker, failed/partial install, and crash-surviving-installer cases using real
  signed release artifacts when published. No fake upstream was introduced.
* Completion validation trusts the signed installer to supply the resulting
  executable. The protocol signs the installer hash, not the installed EXE hash.
  The version query validates the CLI product/version; it is not an independent
  Authenticode or PE FileVersion assertion. The current application has a CLI
  version contract, so no missing PE version resource is assumed to exist.
* This is not transactional rollback. A crash or failed install can leave partial
  state; errors do not claim installation succeeded. Surviving fixed-path Setup
  processes defer retries, but there is no durable recovery protocol for arbitrary
  orphaned installer descendants or machine failure. A hung installer retains
  locks until it exits or an operator intervenes.
* Protected status access still uses `state_dir()` and requires elevation when
  updates are configured; no new unprivileged status channel was added.

## Verification

### 0.4.2 follow-up: isolate updater artifacts from engine journals

The native agent reports that corrected 0.3.99 successfully installed signed
0.4.1 in about 15 seconds, with the correct installed hash and a subsequent
`UpToDate` result. The guide then failed with `Unexpected journal entry`: updater
files had been written beside the engine's journals. The environment fix worked;
the shared-directory content contract was the next blocker.

The 0.4.2 layout is now:

```text
<native ProgramData>\Secblitz\
  engine.lock                 shared with Engine, including older workers
  *.jsonl                     engine journals
  Updates\                    new updater-only protected directory
    update.lock
    update-status.json
    update-manifest.json
    update-installer.exe
    update-worker.exe
```

`update_root()` first obtains and validates `platform::state_dir()`, pins the base
and its ancestors, then atomically creates `Updates` with explicit descriptor
`O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`. Creation uses native security
attributes, not create-then-chmod. Existing objects are never repaired: the opened
object must be a trusted non-reparse directory with a protected DACL containing
exactly SYSTEM and Administrators full-control, propagating allow ACEs. The new
directory pin retains list access, read/write sharing, and no delete sharing.

Both updater mutation paths lock `engine.lock` at the **base**, not under
`Updates`. `engine_lock_root()` validates the exact `Secblitz\Updates` layout
against native ProgramData resolution. Child environment construction uses this
same validation and seeds ProgramData/ALLUSERSPROFILE from the **grandparent** of
the new update root. The eight-variable allowlist and worker preflight remain.

No legacy updater file is migrated, deleted, or overwritten by the new layout.
The separate engine change must accept the exact `Updates` directory and the
five legacy updater filenames with type checks. This updater patch does not
change engine code or its journal rules. During a real 0.4.1 → 0.4.2 installation,
the immutable old worker continues using its old base paths and can write its
final `Installed` record there. The new binary uses `Updates` for later checks;
historical status is not imported or used as a fallback.

Regression updates include native layout rejection tests, atomic protected
directory creation/reopening and regular-file rejection, shared base-engine-lock
contention, new-root duplicate update locks, corrected child environment checks,
and proof that repeated stage replacement does not create the fixture at base.
Existing pin/link tests now exercise the nested root. Engine/audit integration
with actual stage artifacts is owned by the engine reviewer.

Windows cross-build attempts for current 0.4.2 were blocked by host temporary
storage quota (`Disk quota exceeded`), including a retry with incremental output
disabled and `cargo check --locked --target x86_64-pc-windows-gnu --tests`.
Formatting completed; no successful cross-build or native pass is claimed for
this layout patch. Shared build artifacts were not deleted to recover space.

**Required acceptance:** run native suites and guide/audit integration, then an
actual **published 0.4.1 → signed 0.4.2** update. A corrected older fixture alone
does not cover the legacy-worker transition. Verify the old worker's final base
status, retained legacy files, functional guide/journals, and a new-code check
writing under `Updates` against the newly configured `https://secblitz.lol`
origin. Version/origin/publication and legacy engine compatibility are owned
elsewhere. No guest operations or credentials were used in this source change.

### Live E2E follow-up: SYSTEM clean-environment known-folder failure

The latest native evidence in `docs/windows-v040-results.md` and
`/tmp/opencode/secblitz-live-v040/environment-evidence/` supersedes the earlier
offline acceptance. The actual SYSTEM/session-0 check downloaded and verified the
production signed 0.4.0 release, recorded `WorkerStarted`, and launched its copied
0.3.99 worker. Installation never began. The diagnostic with the inherited
environment resolved the state directory; with the exact five-variable production
environment it failed at `SHGetKnownFolderPath(FOLDERID_ProgramData)` with
`0x80070003`. That failure precedes the worker's failure-recording block.

This source fix adds exactly three variables to the closed child allowlist:

| Variable | Source |
| --- | --- |
| `SystemDrive` | Local drive prefix of native `GetWindowsDirectoryW` output |
| `ProgramData` | Parent of the validated, pinned `update_root()` path |
| `ALLUSERSPROFILE` | Same validated ProgramData parent |

All production child launches use the same helper. The resulting eight-variable
environment still uses `env_clear()` and copies no caller environment entries.
No profile, COMSPEC/PATHEXT, ProgramFiles, CLR/profiler, DLL-search, or proxy
variables were added without evidence of need. Known-folder paths can expand
machine environment variables even when resolved through native APIs. The
recorded experiment isolates the five-variable failure but does not independently
establish which added variable is sufficient; SYSTEM runtime retesting is needed.

Before recording `WorkerStarted`, the parent now launches the pinned staged
worker with the fixed existing `update status --json` command and the same clean
environment. The parent retains update/engine locks and root pins. Status resolves
and validates the root before trying the update lock, then returns `DeferredBusy`
without waiting or executing an installer. A nonzero exit or 30-second timeout
fails staging and lets the parent record `Failed` at its already trusted path.
No fallback path is guessed by a worker unable to validate ProgramData. The
read-only preflight is not force-killed on timeout. This catches the demonstrated
startup failure; it is not a durable acknowledgement protocol for every possible
later worker crash. Worker stdout/stderr remain null.

The child-environment allowlist regression now requires exactly eight keys and
checks the ProgramData values. A new elevated native subprocess test reexecutes
the test binary using the actual `child_command()` environment, then calls
`update_root()`, native Program Files resolution, and production `status()` while
the parent holds the update lock. Its only extra environment key is a test-only
recursion guard. Run it both elevated and as SYSTEM/session 0, serialized with
the other updater tests. No PowerShell or alternate environment implementation
stands in for the actual child helper.

Cross-compilation/link validation passed:
`cargo test --locked --target x86_64-pc-windows-gnu --lib --no-run`.
The six elevated tests, actual sanitized Inno launch, and live signed-upgrade
completion await the native agent. No network requests, VM operations, private
keys, or publication credentials were used for this fix.

**Release boundary:** published 0.4.0 artifacts remain immutable. This source
correction is for the next release (version bump owned elsewhere). An installed
0.4.0 updater with the old child environment cannot self-heal through this broken
worker path; deploy the new installer manually. Rebuild the isolated 0.3.99
fixture from the corrected source before testing future automatic updates.
Success with that rebuilt fixture would validate the corrected updater, not
retroactively establish automatic-update support in the immutable 0.4.0 binary.

### Follow-up: directory sharing versus payload sharing

The subsequent native-agent run reported 102 library and 52 CLI tests passing,
but two elevated cases reached their filesystem operations and failed with
Windows error 32: hardlink creation under the pinned staging root and directory
rename after dropping the target directory's own pin. The ancestor pins were
still held, exposing a production staging problem rather than a fixture-only
failure.

`open_object()` now distinguishes object types. Directory pins request
`FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | READ_CONTROL` and share
`FILE_SHARE_READ | FILE_SHARE_WRITE`, with no delete sharing. List access keeps
the directory's own rename/delete exclusion effective; write sharing permits
authorized mutations of child entries. This is a handle-sharing change, not an
ACL grant. Trusted ownership, DACL validation, and reparse checks are unchanged.
Payloads retain `GENERIC_READ | READ_CONTROL` with read sharing only.

The native tests retain all `update_root()` pins throughout their operations:

* The prefix test creates, renames, and removes a child while the directory's own
  pin is held; renaming that directory must fail with sharing violation. Once
  only its own pin is dropped, its rename must succeed with ancestor pins intact.
* The hardlink/reparse test must create its actual fixtures and receive the
  `Unsafe update object` inspection error, not merely any sharing failure.
* A new production `replace()` test creates a stage and replaces it twice with
  ancestor pins held. An intervening payload pin must reject replacement and
  preserve bytes; dropping that payload pin permits replacement again.
* The existing payload write/delete/rename exclusion test now also retains
  `update_root()` pins.

Windows test cross-compilation/link validation passed with
`cargo test --locked --target x86_64-pc-windows-gnu --lib --no-run`.
The five elevated tests require fresh native-agent execution; no native pass is
claimed for this change. Installer lifecycle acceptance remains pending. No VM,
application ACL, or worker stdout/stderr changes were made in this follow-up.

### Follow-up: default ProgramData ancestor permissions

The native run recorded in `docs/windows-v040-results.md` passed payload pinning
and lock exclusion, but the two link/prefix tests failed at `update_root()` before
their assertions. The recorded `acl-evidence.json` shows SYSTEM-owned ProgramData
granting Users an applicable `0x116` ACE: child creation, `FILE_WRITE_EA`, and
`FILE_WRITE_ATTRIBUTES`. The original ancestor mask incorrectly rejected `0x110`.

The correction adds only EA/attribute writes to the existing
`!strict && directory && ancestor` branch. Trusted ownership, reparse rejection,
data/list-access pins, and protected-root/files policies remain required. These
ancestor rights neither change the child DACL nor grant replacement of the
protected Secblitz directory. `WRITE_DAC`, `WRITE_OWNER`, `DELETE`,
`FILE_DELETE_CHILD`, generic write/all, and unknown rights remain rejected.
The recorded drive-root ACL has no applicable untrusted delete-child grant;
its inherit-only generic-write/delete ACE does not apply to the pinned ancestor
and cannot propagate into the protected Secblitz root.

The service validator was inspected: it pins Program Files ancestors using list
access and similarly rejects delete/ACL/owner rights. It does not validate
ProgramData. The journal helper checks ancestor ownership, then imposes the
protected SYSTEM/Administrators-only policy on its own root/tree. Neither helper
was changed. The worker's null stdout/stderr configuration is preserved.

Three non-elevated Windows regression tests now exercise the production ACL
validator using native SDDL parsing, without changing filesystem ACLs:

* Accept the exact recorded default ProgramData SDDL only as an ancestor; reject
  the same descriptor with an untrusted owner and accept the protected root DACL.
* Reject added DACL/owner/delete/delete-child/generic-write/all/maximum-access
  rights on ancestors.
* Reject EA/attribute writes on protected roots, staged files, installed files,
  and the immediate installed directory, including inconsistent ancestor flags.

Windows cross-compilation/link validation passed after the correction using
`cargo test --locked --target x86_64-pc-windows-gnu --lib --no-run`.
The native agent must rerun these tests and all four elevated tests with fresh
artifacts. The existing hardlink/reparse and directory-prefix tests still enter
through `update_root()`; their assertions were not bypassed. Actual native
pass/fail and the blocked installer lifecycle gates remain unconfirmed for this
patch. No VM changes were made during this follow-up.

### Initial cross-build

Windows GNU cross-build environment: `/tmp/opencode/secblitz-cross-env.sh`.

* `cargo check --target x86_64-pc-windows-gnu --tests` passed.
* `cargo test --locked --target x86_64-pc-windows-gnu --lib --no-run` compiled
  and linked the Windows test executable.
* Added version-output rejection and privileged-child configuration tests.
* Added elevated native tests for hardlink/reparse rejection and data-access
  directory pins blocking prefix renames. Existing payload write/delete/replace
  exclusion and duplicate-lock tests remain.

Native tests have not been executed here. Run updater tests on Windows, with
the protected-root cases elevated and serialized:

```powershell
cargo test --lib updater::windows::tests -- --test-threads=1
cargo test --lib updater::windows::tests -- --ignored --test-threads=1
```

The first Cargo validation automatically refreshed `Cargo.lock` for the new
dependency graph. This was an incidental build-tool change outside the two
assigned source/review files, not a protocol or dependency-design edit. Other
reviewers should reconcile that lockfile; it was not blindly restored over
potential concurrent work.
