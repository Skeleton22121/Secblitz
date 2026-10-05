# Review 4 of 4: keyboard integration

Date: 2026-10-02. Result: no application-main integration bug found. Only this review file was added. `src/main.rs` was reviewed without changes; existing tests were sufficient, so no implementation-mirroring tests were added.

## Integration findings

- `main.rs` declares `mod menu`; both `guided::run` and `guided::handle_broker` construct `TerminalMenu`. Production guided input uses `ChoiceInput`, not the old numeric line parser. There is no `StdinLock`, `stdin().lock()` or `read_line` in main, guided or menu source. Output locks elsewhere are not input locks.
- `execute` rejects unsupported JSON commands before terminal checks or interactive work. Only audit/apply/revert/history accept JSON. Guide checks stdin, stdout and stderr before elevation or engine access; the broker checks again before creating its menu. Each terminal-menu interaction also checks that contract. Redirected execution does not silently enter an interactive fallback.
- The final-pause predicate excludes both explicit `guide` and every `service` subcommand. No-argument guide also bypasses it. JSON bypasses it independently. `service run` dispatches before application elevation; install/start/uninstall/status remain explicit, closed choices. The existing UI pause guard requires all three streams to be terminals for the other console-owned commands.
- Elevation arguments are reconstructed from parsed fixed choices, preserving language and applicable flags. Guide receives only the internal `--desktop-broker` hint. Native `ShellExecuteExW` retains the process handle; `WaitForSingleObject` completes before exit-code retrieval and broker dispatch. The handle is released through RAII. No parent menu, pause, input worker or input-mode guard runs while the child is active.
- A shared console would share an input queue, but the reviewed parent/child call graph still has only one active reader: child first, parent broker after child exit. Separate-window behavior is not needed to establish that ordering. Queued keys can survive a handoff; confirmations default to No and require an affirmative highlighted choice. This source review does not establish the exact Windows Terminal/UAC console topology at runtime.
- `desktop_loop` recognizes only guide exit codes 23 through 27. The unelevated broker requests fresh action consent, then separately asks whether to return before another elevation. Other commands' exit codes never invoke broker input.

## Keyboard behavior and Windows API boundary

The prior diagnosis in `windows-v031-results.md` reproduced the old v0.3.0 numeric interface: Down/Space did not move focus, and whitespace plus Enter exited normally. That supports an interaction-model mismatch, not a proven deadlock in the user's earlier session. Current source replaces that boundary with native keyboard menus.

Normal menus use dialoguer 0.11 `Select`/`MultiSelect` with `interact_on_opt(&Term::stderr())`. Prose on stdout is flushed before interaction. Checkboxes start unchecked; selected indices are bounded, deduplicated and mapped into the current trusted candidate list. Confirmations use the menu owner's single-Enter loop, default No, and ignore Space/text as approval. Ordinary Select also accepts Space through dialoguer; this does not apply to mutation confirmations.

Reviewed the locked console 0.15.11 Windows implementation: `Term::read_key` uses native `ReadConsoleInputW` on `STD_INPUT_HANDLE`, while the chosen stderr terminal supplies rendering. This is a supported Windows console API path suitable for console applications hosted by Windows Terminal as well as conhost; it does not use a Rust buffered stdin lock. All three valid terminal streams remain required. Host compilation is not native keyboard acceptance.

No new production keyboard/elevation trace logging was found. The 14 `PROBE`/`READY` marker occurrences in `menu.rs` are inside ignored, test-only harnesses. Ordinary application errors use the existing friendly-error/details policy. One exception already documented by the menu owner remains: a caught dialoguer live-resize panic can still print the default panic-hook diagnostic before returning an error. This is outside main ownership and means a blanket claim of no possible panic traces would be inaccurate. QuickEdit/focus and native Ctrl+C behavior likewise are not established by this review.

## Installer, version and copy checks

- Desktop and Start Menu shortcuts still launch the executable with no arguments, which selects guide. Finish-page launch still passes exactly `guide`, with `runasoriginaluser`, `skipifsilent`, and the existing success gate. No installer argument changes are needed for the keyboard module.
- Existing installer source checker passed: **6 locales, 4 complete custom messages per locale, 39 deliberately rejected regressions**.
- Cargo package version is **0.3.1**. The freshly compiled host CLI returned `secblitz 0.3.1`. Current dependency declarations include console 0.15 and dialoguer 0.11 with dialoguer default features disabled; locked host and Windows checks accept the dependency/API combination. This does not identify or promote an existing packaged executable.
- Existing copy scanners passed: whole-key guided/menu coverage, localized keyboard hints across **6 languages**, obsolete numeric-prompt rejection, fixed diagnostic coverage, complete/unambiguous catalog, and exact coverage of **105 report handoff keys**.
- Literal U+2014 scan: **0 matches** across `src`, `installer`, `website`, `README.md`, `Cargo.toml`, `Cargo.lock`, `docs/guided-copy-keys.md`, and `docs/report-copy-keys.md`. This is an explicit scoped count, not a claim about every historical document. No global copy was edited.
- Input/trace scan of `src/main.rs`, `src/guided.rs`, and `src/menu.rs`: **0 buffered line-reader/held-stdin-lock matches**, **0 `trace!` matches**, and the **14 test-only probe markers** noted above.

## Verification performed

Read `/tmp/opencode/secblitz-cross-env.sh` once and sourced it for Rust commands. Checks used its existing user-local toolchain/cache and target directory.

| Check | Result |
| --- | --- |
| `cargo test --locked --all-targets` | 79 library + 46 binary tests passed; 0 failures; 2 ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings` | Passed |
| `cargo check --locked --target x86_64-pc-windows-gnu --all-targets` | Passed |
| `python3 installer/check-locales.py` | Passed with counts above |
| `cargo test --locked --bin secblitz i18n::tests::catalog_is_complete_and_unambiguous` | Passed targeted follow-up after observing the catalog owner's strengthened copy assertion |
| `cargo run --locked --bin secblitz -- --version` | `secblitz 0.3.1` |

`native_keyboard_smoke` needs an attended terminal or PTY keyboard driver; `native_resize_smoke` needs a coordinated resize driver. Their ignored status is intentional, not a test failure. Neither was enabled for unattended CI or run by this reviewer. The menu owner's earlier Linux PTY observations are recorded separately in `keyboard-review.md`.

No guest access, native Windows execution, installer execution, release build or artifact promotion was performed. Native Windows Terminal/conhost key handling, focus/QuickEdit, and original-standard-user UAC/broker round-trip acceptance remain with the coordinating runtime reviewer.
