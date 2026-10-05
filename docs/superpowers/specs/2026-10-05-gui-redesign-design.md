# Secblitz 0.7.0 — Native GUI, tray, background checks, history, debloat

Status: approved in conversation 2026-10-05. Replaces the terminal UI.

## 1. Intent

Secblitz becomes a polished desktop app, comparable in feel to a consumer
antivirus dashboard, for **non-technical users who barely know how to use a PC**.
Everything visible must be simple, calm and plain-language. No developer terms
(no "control id", "journal", "transaction", "DISM", "SFC", "provisioned",
"registry", "digest", "exit code") on primary surfaces. Technical evidence
stays available only behind a "Technical details" expander.

Success: a user can open Secblitz, understand in five seconds whether the PC
is safe, fix what's recommended with one button and a confirmation, undo it,
remove junk apps with sensible defaults, and see history — all in a single
self-contained `secblitz.exe` that works on machines without a GPU.

All existing capabilities remain functional (checks, selected fixes with
post-verification, undo, history, readiness, Windows repair operations,
Windows quality updates, diagnostic profiles, Defender update/scan, password
generator, Bitwarden install, Windows Settings shortcuts, monitor service,
signed self-updater). New: tray icon + alerts, background checks toggle,
protection score + history timeline, debloater.

## 2. Technology

- **iced 0.14**, `default-features = false`, features `tiny-skia` (CPU renderer,
  verified in the no-3D VM), `canvas`, `svg`, `image`, `advanced`, `thread-pool`.
  No wgpu. Cross-compiles with the existing MinGW toolchain (Rust 1.93).
- Bundled **Inter** font (OFL, `assets/fonts/`), icons as inline SVG
  (Lucide, ISC licence; attribution in `assets/ICONS-LICENSE.txt`).
- Executable built with `#![windows_subsystem = "windows"]`.

## 3. Process model

| Invocation | Privilege | Role |
|---|---|---|
| `secblitz.exe` (no args) | standard | **Launcher/broker.** No window. Requests UAC for `gui --broker <id>` **every time** the app is entered, then serves broker requests until the GUI exits. |
| `secblitz.exe gui [--broker <id>]` | elevated (required) | **Dashboard.** Owns `Engine`, maintenance, debloat. Single instance: a second elevated instance focuses the first window and exits. Refuses to run unelevated. |
| `secblitz.exe tray` | standard | **Tray agent.** Started at logon (installer HKLM Run value). Reads `status.json`; icon state + notifications; menu: Open Secblitz / Check now / Quit. Open & Check now start the launcher (UAC each time). Single instance per session. |
| `service …`, `update …` | SYSTEM / LocalService | Unchanged hidden/internal commands. |

Removed: the interactive TUI (`menu.rs`, terminal rendering in `ui.rs`,
`guided.rs` screen driver) and the human CLI (`guide`, `audit`, `apply`,
`revert`, `history`, `password`, `tools`, `diagnostics`, `operations`,
`quality-updates`), plus the `indicatif`, `console`, `dialoguer` crates.
A hidden `gui --self-test <page>` exists for automated screenshots.

### 3.1 Broker channel

Launcher creates `\\.\pipe\secblitz-broker-<128-bit random hex>` with
`FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS`, one instance,
DACL: current user SID + Administrators only. It passes the id via the
existing whitelisted-argument elevation path. After accept, the launcher
requires `GetNamedPipeClientProcessId == GetProcessId(elevated child handle)`;
otherwise disconnect. Messages: request = 2 bytes `[kind, arg]`, response =
1 byte status. Kinds (closed enum): OpenWindowsUpdate, OpenWindowsSecurity,
OpenEncryption, OpenSignIn, InstallBitwarden, BlockSuggestedApps,
ReinstallStoreApp(arg = index into the compiled debloat catalog). Unknown
kinds/indices are rejected. No strings cross the boundary. The GUI's own
consent dialog is the consent; the launcher does not re-prompt (it has no
window) but only performs the fixed action.

### 3.2 Updater interaction

`updater::busy()` currently treats any `secblitz.exe` from the install path
as busy. The tray must not block updates forever:
- The tray creates nothing global. The SYSTEM update worker, before
  installing, creates `Global\SecblitzUpdateQuiesce` (manual-reset event,
  DACL: SYSTEM full, Users SYNCHRONIZE) and sets it; the tray polls
  `OpenEventW` every 5 s and exits cleanly when it's signalled.
- `busy()` ignores processes whose command line is exactly the tray mode
  (read via `NtQueryInformationProcess(ProcessCommandLineInformation)`),
  then waits ≤15 s for those PIDs to exit before installing; if any remains,
  `DeferredBusy`.
- After install (success or failure) the worker relaunches the tray in each
  active session that had one, via `WTSQueryUserToken` + `CreateProcessAsUserW`
  (desktop `winsta0\default`), unelevated. Otherwise it returns at next logon.

## 4. Data

- **Score**: protected/total over control results (`advice::for_outcome`
  group Protected vs not; findings excluded). Shown as a ring "15 of 18".
- **checks.jsonl** in the engine state dir (admin-only ACL inherited): one
  line per completed check `{t, protected, total, kind: "check"|"fix"|"undo"|"debloat", n}`;
  capped at 500 lines (oldest dropped).
- **status.json** written by the monitor service next to `latest.json` in a
  new `Program Files/Secblitz/Status/` directory whose DACL grants Users read;
  schema `{schema:1, t, protected, total, attention:[ids], state:"ok"|"attention"|"unknown"}`,
  ≤4 KiB. The full monitor report keeps its existing protected DACL. The GUI
  also writes `status.json` after each check it performs (elevated) so the
  tray is fresh even without the monitor.
- **debloat journal** `debloat.jsonl` in the engine state dir: per batch
  `{t, removed:[{family, name, version, catalog_index}], skipped, failed}`.

## 5. Screens (plain language)

Window 1100×720, min 880×600, neutral palette (zinc greys, white surfaces), **light default** + neutral dark
mode; colour only for meaning (green protected / amber attention / red
failed / grey unknown); primary buttons near-black (white in dark mode). Left sidebar: Home, Fixes, Clean up apps, Tools, History,
Settings. One primary button per screen. Every change goes through a review
sheet whose default focus is **Cancel**. Keyboard: Tab/Enter/Esc.

- **Home**: score ring + verdict ("You're protected" / "3 things need your
  attention" / "We couldn't finish checking"), "Last checked …", primary
  button "Fix 3 problems" (or "Check again" when nothing to fix). Cards:
  Needs attention (top 4 with one-line reason) and Protected (count, show
  all). Readiness strip only when relevant ("Your disk is almost full", "A
  restart is waiting"). First run: scanning view with pulsing shield and live
  checklist.
- **Fixes**: list of all checks grouped "Needs attention / Protected / Can't
  check"; attention rows have a checkbox (pre-ticked = recommended), name,
  one-line "Protects you from …", restart badge; expander for "Why" + technical
  details. Sticky footer "Fix N selected".
- **Review sheet**: "We're about to fix these N things", list, restart note,
  "You can undo this later", buttons Cancel (default) / Fix now.
- **Working view**: progress with live per-item ticks; cannot be dismissed.
- **Result**: "You're now protected from: …" (only post-check-confirmed),
  "After you restart …", "Couldn't fix" with plain reason; Done.
- **Clean up apps** (debloat): §6.
- **Tools**: cards — Scan for viruses (Defender quick scan), Update virus
  protection, Repair Windows (check / repair system files; long-running,
  consent sheet), Windows updates (find & install security updates;
  consent sheet), PC health tips (diagnostic profile: Everyday / Gaming /
  Work & development / Extra security), Password generator, Password manager
  (Bitwarden), Windows settings shortcuts.
- **History**: score trend line + timeline of checks, fixes, undos, app
  clean-ups. "Undo last fixes" (newest batch, consent sheet). Debloat rows:
  "Restore" per app (broker winget reinstall; fallback opens Store page).
- **Settings**: Language, Theme, "Check my PC in the background" (monitor
  install/start/uninstall), "Show Secblitz in the taskbar corner" (tray Run
  value), automatic app updates status, About (version, licences, technical
  details).

## 6. Debloat

Catalog compiled into the binary: exact package family name prefixes (no
regex sweep), display name, group, Store product id (verified on VM, else
None → "can't restore automatically"). Groups and defaults:
Recommended (on), Sponsored apps (on), AI & Microsoft promotions (off), Apps
you may use (off), Gaming/Xbox (off, warning). Protected list (never shown,
refused if requested): Store, DesktopAppInstaller, SecHealthUI, VCLibs,
UI.Xaml, NET.*, WindowsTerminal, codec extensions, XboxIdentityProvider,
Edge, Calculator, Notepad, ScreenSketch, OneDrive, ShellExperienceHost,
anything NonRemovable or framework.

Mechanics (Windows PowerShell 5.1, elevated interactive admin, not SYSTEM):
inventory `Get-AppxPackage -AllUsers` + `Get-AppxProvisionedPackage -Online`
filtered to catalog; removal per app: `Remove-AppxPackage -AllUsers` then
`Remove-AppxProvisionedPackage -Online`; 0x80073CFA / NonRemovable → skipped
"Windows protects this app". Optional "Keep them from coming back" →
broker BlockSuggestedApps (HKCU ContentDeliveryManager values) + elevated
HKLM `CloudContent\DisableWindowsConsumerFeatures=1`. Journal per batch.
Attribution comment for Win11Debloat / WinUtil (MIT) lists.

## 7. Testing

Host `cargo test` (flow logic, score, status schema, broker codec, debloat
catalog invariants: no protected family selectable, defaults only from
Recommended+Sponsored, unique indices). Windows cross-build + Clippy
`-D warnings`. VM (Secblitz-W11-UI-Test only): snapshot first; screenshots
of every page via `--self-test`; real check → fix 2 → post-check → undo;
debloat Recommended+Sponsored → restore one; tray icon + notification after
a forced regression; installer install/upgrade with tray running.
