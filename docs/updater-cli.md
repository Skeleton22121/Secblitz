# Updater CLI integration review

## Closed commands

The 0.4.0 CLI exposes only these updater operations:

```text
secblitz update check
secblitz update status
secblitz update check --json
secblitz update status --json
```

The internal `secblitz update install-staged` command is accepted but hidden from updater help. There are no endpoint, path, token, installer, channel, force or consent-bypass parameters. Existing global language, details and animation flags do not change updater authority. JSON is rejected for the worker.

`check` calls `updater::check_and_stage`; `status` calls `updater::status`; the worker calls `updater::install_staged`. The CLI does not construct an update URL, read a signing key, select an installer path or implement a second update protocol. Origin, signature, version, staging, worker identity and protected status handling remain core responsibilities. The current fixed-origin deployment is configured separately for `https://beacons.lol`; no network availability or publication success is claimed by this integration.

## Elevation and process lifecycle

- Interactive `check` and `status` requests from a non-elevated terminal use the existing UAC launcher. Child arguments are rebuilt only from parsed fixed words: language, approved global flags, `update`, and exactly `check` or `status`. They satisfy the existing ASCII alphanumeric/hyphen argument policy.
- A UAC failure or cancellation is an operational failure. The updater path does not enter the guide's desktop broker loop or retry elevation.
- Noninteractive and JSON requests never open UAC. They require an already elevated caller. An already elevated SYSTEM scheduled-task invocation calls the core directly.
- `install-staged` never requests elevation, even if someone invokes it manually. Core path, privilege and worker checks remain mandatory; the CLI does not turn a worker command into permission to install.
- All updater commands bypass branding, progress animation, menus, the security engine and console pauses. The worker is always silent. Noninteractive requests without `--json` are silent, including failures.
- `WorkerStarted` is returned as soon as the core returns. The CLI neither waits for installation nor holds the old executable open with an exit prompt. Core installation/status persistence continues in the separate protected worker.
- Service commands remain non-pausing. The no-argument launch still opens the arrow-key guide. No timer, hourly loop or updater daemon was added to the interactive UI.

### Core handoff: worker output handles

At this review, `src/updater/windows.rs::child_command` nulls stdin but the `update install-staged` spawn still inherits stdout/stderr. The core owner should detach those two streams with `Stdio::null()` on the worker spawn. Otherwise the parent CLI process can exit promptly while a pipe-capturing caller still waits for worker-held output handles to close, and descendant output could enter the caller's JSON stream. This is a core spawn-boundary change, not a reason for the CLI to wait for the worker or mutate inherited Windows handles itself. It was not changed outside this review's file ownership.

## Output and exit codes

Interactive text output is one plain localized status line. Failed results add a short next-step hint; native diagnostic evidence is shown only with an explicit interactive `--details`. Consumer headings and messages are translated in all six languages. Core operational diagnostics are treated as technical evidence rather than expanding the entire core into the presentation catalog.

| Core outcome | CLI exit | Human meaning |
| --- | --- | --- |
| `NotConfigured` | 0 | Updates unavailable for this installation, not proof that it is current |
| `UpToDate` | 0 | Secblitz is up to date |
| `DeferredBusy` | 0 | Update deferred while Secblitz is busy; no successful installation claimed |
| `WorkerStarted` | 0 | Update ready and handed off; not yet an installation-success claim |
| `Installed` | 0 | Core reported installation success |
| `Failed` | 1 | Update failed, including a previously recorded failure returned by status |
| Core/elevation/I/O error | 1 | Operational failure, never ignored |

For a status record with `checked_at = 0` and `NotConfigured`, the human text is **No update information yet.** This avoids presenting a missing status file as either a completed check or a disabled configured origin. A failure record with timestamp zero remains a failure.

`check --json` serializes the returned `UpdateOutcome` directly. `status --json` serializes `UpdateStatus` with its `checked_at` and `result` fields. Enum tags, versions and other machine fields are not translated. No branding, progress, menus or native error chain is mixed into JSON. If the call itself returns an error, the CLI writes a generic structured failure with exit 1:

```json
{"outcome":"failed","reason":"The update could not be completed."}
```

This generic error does not expose native request URLs or operational metadata. Successfully read core status records retain the core's serialized fields. `--details` does not append native text to JSON. Writer failures also return exit 1. Worker diagnostics and persistent status remain owned by the core's protected logging/status mechanism.

## Installer and scheduling boundary

The installer owns the user-consented scheduled task and its default selection. The task invokes the installed executable's fixed `update check` command as SYSTEM. This CLI change neither creates that task nor silently opts a user into it. A busy check can be retried by the next configured task run or by an explicit later check. Updating the program does not run the security guide or apply hardening settings.

## Verification scope

Added host tests cover the closed command grammar and hidden worker, arbitrary argument rejection, canonical UAC arguments, refusal to elevate JSON/background requests, SYSTEM dispatch, worker dispatch without any elevation shortcut, UAC cancellation, exact JSON shapes, failed-outcome exit codes, redacted error JSON, silent background/worker results, all-six-language human status and updater/service no-pause behavior.

These tests inject updater/elevation results; they do not fetch a release, install a package, modify a scheduled task or access a guest. Live protocol and Windows scheduler/installer validation remain with their owners. No private keys or deployment credentials are involved.

### Executed results

Using `source target/build-tools/cross-env.sh` against the integrated 0.4.0 source:

- `cargo test --locked`: **93 library tests passed; 52 binary tests passed, 2 existing interactive menu probes ignored; 0 failures**. All 12 localization checks passed within the binary suite. Doc-tests passed with no cases.
- `cargo clippy --locked --all-targets -- -D warnings`: **passed**.
- `cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings`: **passed**. This is cross-target checking, not guest execution.
- Formatting check for only `src/main.rs` and `src/i18n.rs`, with module recursion disabled: **passed**.
- A brief shared Cargo build-lock wait resolved normally. No unresolved compiler or cross-owner API error remained in these runs.
