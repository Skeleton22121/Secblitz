# Keyboard menu independent review

## Scope and evidence

Initial design review, 2026-10-02, followed by implementation review and fixes below. Initial ownership was this file only; follow-up ownership explicitly includes `src/menu.rs`. No other source files or guests were modified by this reviewer. Initial findings below describe the pre-menu snapshot, not the current implementation.

## Implementation review and fixes

Reviewed actual `src/menu.rs` and the converted input boundary in `src/guided.rs`.

- **Fixed double confirmation:** the previous `Select` followed by `approve()` demanded two Enter presses after choosing Yes. `TerminalMenu::select(..., enter_only = true)` now uses a small native `Term::read_key` loop. Arrows move, one Enter returns the highlighted choice, Space and arbitrary text do nothing, and Esc/q cancel. `guided::Input::confirm` still supplies default index 1 (No), and approves only `Some(0)`. The `ChoiceInput` signature is unchanged.
- **Contained live-resize panic:** retained dialoguer for normal Select/MultiSelect, with a narrow `catch_unwind` boundary around dependency interaction. The upstream <3-row clamp panic becomes an error; no selected indices or consent escape the failed interaction. Existing caller-owned cursor cleanup runs normally after recovery. This requires the current unwind panic strategy; abort builds cannot recover. The process-global panic hook is deliberately unchanged, so the upstream diagnostic can still appear before the error is returned. This is panic containment, not a claim that upstream no longer panics or that the screen fully redraws afterward.
- **Native confirmation layout:** checks dimensions both before drawing and after input, clips each label to physical display width, shows at most 10 choices per page, and clears only its own visible rows on return/error. It does not use dialoguer paging and returns an I/O error for a too-small resize. Hints are also clipped to one physical line. Both menu types reject initial dimensions below three rows or six columns.
- **Reviewed guided invariants:** production uses `TerminalMenu`, not a stdin lock/numeric parser; `choose_inner` flushes prose before interaction and bounds-checks returned indices; `multiple` bounds-checks, sorts and deduplicates; repair candidates remain a trusted report snapshot; fresh consent remains separate from checkbox selection. These call sites were read-only.

### Verification completed

- `cargo test --locked --bin secblitz`: **46 passed, 0 failed, 2 opt-in tests ignored**. Includes guided consent/broker/stale-report tests and translation coverage. New reducer tests verify single-Enter approval, default No, ignored Space/text, cancellation and wrapping navigation. The dependency-boundary test exercises the upstream clamp failure and preserves ordinary I/O error kinds.
- Ran both ignored probes against the compiled Rust test executable in a Linux PTY with all three streams attached: **both passed**. Keyboard probe exercised page navigation to index 11, Esc, unchecked Enter, checkboxes 1 and 3, default No, Space on highlighted Yes remaining in the same prompt, then one Enter approving Yes.
- Live-resize PTY probe changed 24×80 to **2×80 while waiting for a key**, separately for dialoguer Select, dialoguer MultiSelect and native confirmation. All returned errors rather than terminating the test process. Cursor-show output was observed before every return; dimensions were restored between probes.
- No engine/action runs in either probe. Windows conhost/Windows Terminal, QuickEdit and UAC runtime checks remain for the main owner; Linux PTY results do not establish native Windows behavior.

### Remaining handoff notes

- Ordinary dialoguer Select still accepts Space as an upstream convenience; **confirmation Space never approves**. MultiSelect retains Space toggling and Enter submission.
- Ctrl+C remains OS-managed via `read_key`; do not advertise it as equivalent to Esc or promise Drop cleanup after OS termination.
- QuickEdit and parent/child lifecycle observations from the initial review remain relevant. No guest changes were made to investigate them.
- Follow-up source changes are limited to `src/menu.rs` and this review file.

`Cargo.toml` declares dialoguer 0.11 with default features disabled and console 0.15. Local console source inspected: `/tmp/opencode/secblitz-cargo/registry/src/index.crates.io-1949cf8c6b5b557f/console-0.15.11/src/`. Dialoguer was not yet present in that cache, so the exact upstream v0.11.0 source was read:

- [Select](https://github.com/console-rs/dialoguer/blob/v0.11.0/src/prompts/select.rs)
- [MultiSelect](https://github.com/console-rs/dialoguer/blob/v0.11.0/src/prompts/multi_select.rs)
- [Paging](https://github.com/console-rs/dialoguer/blob/v0.11.0/src/paging.rs)
- [Renderer](https://github.com/console-rs/dialoguer/blob/v0.11.0/src/theme/render.rs)

## Findings for implementation owner

1. **Replace the entire production line-input boundary.** `guided::run` and `guided::handle_broker` hold a `StdinLock`; `Input::line` blocks on `read_line`, and `Input::confirm` still requires `"1"`. Convert main menu, fixes, next steps, extra tools, action confirmations, and broker reconsent/return together. Do not retain a lifetime-long stdin lock while adding another reader. A Rust lock by itself does not prove a native Windows deadlock: console's Windows key reader uses `ReadConsoleInputW`, bypassing Rust's buffer. Mixing buffered line reads with native event reads nevertheless gives two input mechanisms and can strand buffered input. Scripted test input should be a separate test adapter, not a production numeric fallback.

2. **Cancellation is Esc/q, not Ctrl+C.** Both dialoguer optional interactions return `Ok(None)` for Esc or q. `Select` accepts Enter **or Space**; `MultiSelect` toggles with Space, submits with Enter, and also toggles all with `a`. Both call `Term::read_key`, not `read_key_raw`. Console 0.15.11 calls `read_single_key(false)`, leaving Windows processed Ctrl+C behavior in effect. Do not document Ctrl+C as a clean Back/None result or assume destructors run on process termination. Esc should exit the root, return from submenus, and decline confirmations. Default every mutation confirmation to No; all fix checkboxes start unchecked. Check selection with `Some(yes_index)`, never `is_some()`.

3. **Add caller-owned cursor cleanup for ordinary errors.** Dialoguer hides the cursor before rendering and explicitly shows it on successful selection/cancellation, but renderer/read/clear/flush `?` exits have no cursor Drop guard. Wrap the interaction in a guard that best-effort calls `show_cursor()` and `flush()` on the same `Term`. Install it before interaction; propagate the original error. This handles ordinary errors/unwind, not OS termination or abort. Native console cursor functions can silently ignore Win32 failures, so this remains best-effort.

4. **QuickEdit is a separate source of apparent hangs.** Console's Windows key-read path does not disable QuickEdit. Selecting output in legacy conhost can suspend processing; changing `read_line` alone cannot establish that this failure is fixed. If adding a native guard, save the complete input mode, set `ENABLE_EXTENDED_FLAGS`, clear only `ENABLE_QUICK_EDIT_MODE`, and restore the saved mode on every ordinary exit. Cover the whole guided scan/menu session and broker interaction, since output can pause during a scan too. Do not hold such a mode guard in a waiting parent across a child session. Use checked native calls and preserve other mode bits. Confirm behavior in Windows Terminal and legacy conhost; mouse selection should not be mistaken for a keyboard-read failure.

5. **Keep all three TTY checks and visible errors.** `guided::require_terminal` already checks stdin/stdout/stderr before elevation (`main::execute`) and broker input (`handle_broker`). Keep this: dialoguer checks its rendering term, not the application's full three-stream contract. Use `interact_on_opt(&term)` with an explicit stderr term if menus follow progress/branding; stdout remains the existing report stream. Flush stdout before a stderr interaction. Propagate rendering/input errors, rather than treating them as cancellation or retrying forever. `main::show_error` currently hides unfamiliar errors unless `--details`; add a fixed localized interactive-input failure hint if necessary.

6. **Bound page size and keep menu rows physically single-line.** Dialoguer `Paging::new` and `Paging::update` use `clamp(3, terminal_rows)`, which panics below three rows. A preflight minimum of 10 rows avoids that initial failure, but does not cover resizing to fewer than three rows during interaction. Treat resize as a known dependency limitation unless actually mitigated. `.max_length(n)` accepts item count and internally adds two; reserve prompt/page space and choose a positive bounded item count. Renderer wrap accounting uses byte lengths and a hard-coded prefix width; paged clearing does not account for implicit wrapping. Fit/sanitize translated labels and prompts to display-cell width, reserve prefix/page suffix and the final physical column, and avoid embedded newlines. A fixed page cap alone does not fix wrapping. Default paging emits English `[Page x/y]` outside the theme; include this in localization review.

7. **Preserve report output; do not double-clear.** Dialoguer defaults to clearing its own prompt/menu. Prefer `.report(false)` after `.with_prompt(...)` if the caller prints its own selection summary. Do not clear the entire screen or call a second unconditional `clear_last_lines`, which can erase the audit/consent above it. `Ui::Progress::drop` already calls `finish_and_clear`; keep progress lifetimes ended before menu rendering. Newline-terminate and flush prose before opening a menu. Native Enter is consumed as a key event rather than requiring a trailing line read; do not add a blocking "drain newline" read. Queued repeated Enter can reach the next prompt, so default-No confirmations are important even with event input.

8. **Current parent/child path has no competing input reader.** `main::elevate_and_wait` waits on the process handle before obtaining its exit code. `desktop_loop` invokes `handle_broker` only after that call returns. Guided/default launches also bypass `ui::pause` in `main`. Preserve these boundaries: no parent menu/pause/input worker while waiting. Console inheritance or separate-window behavior should be verified on Windows, but neither topology creates simultaneous readers in the reviewed call graph. Do not diagnose this as an existing dual-reader bug without runtime evidence.

9. **Keep privilege dispatch closed and ID-based.** `guided::session` builds candidates from current attention results, available controls, and repair advice. Map returned checkbox indices only into that exact candidate snapshot; retain invalidation after changes and separate affirmative consent before apply/undo/actions. Test adapters should reject out-of-range indices rather than panic. Preserve fixed broker exit codes 23–27, fresh parent reconsent, and `main::elevated_args` reconstruction. Labels, typed text, translated strings, and arbitrary arguments must never become privileged commands.

## Implementation follow-up checklist

Review actual `src/menu.rs` and all production call sites once available:

- Root/submenu Esc, unchecked Enter, confirmation Enter/No, affirmative Yes, checkbox toggle twice, multi-page selections, and input/render errors.
- Maintain guided tests for candidate filtering, stale-report invalidation, partial failures, rollback, broker reconsent, and explicit return before re-elevation. Replace numeric scripts with semantic choices; old parser tests do not exercise keyboard menus.
- Verify redirected stdin, stdout, and stderr each reject guide before audit/UAC and reject broker work before input. Existing `redirected_guide_and_broker_fail_before_interactive_work` covers the normal captured-runner topology, not each stream independently.
- Test native Windows arrows/Space/Enter/Esc in both conhost and Windows Terminal, with animation enabled/disabled, minimum supported dimensions, long translated labels, page changes, resize, and mouse selection. Observe cursor visibility and console mode after selection, cancellation, and an ordinary error. Test Ctrl+C separately without claiming RAII termination cleanup.
- Observe a full child-to-parent broker round trip: parent waits, child exits, parent asks again, decline causes no new UAC, explicit return alone reopens. Check no extra `read_line`/pause consumes a later key.

Status: implementation review and owned fixes complete; binary tests and Linux PTY keyboard/resize probes pass. Native Windows verification remains pending with the main owner.
