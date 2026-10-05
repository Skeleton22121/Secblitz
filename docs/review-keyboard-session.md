# Independent keyboard review - session (2 of 4)

## Verdict and scope

Reviewed on 2026-10-02. **No confirmed guided-session correctness or authorization bug found.** Source and tests were left unchanged; this report is the only file authored by this review.

Owned review surface: `src/guided.rs`, including its generic `ChoiceInput` tests. Read-only context: `src/menu.rs`, `src/main.rs`, `src/engine.rs`, and `src/advice.rs`. Native terminal behavior belongs to the terminal-menu reviewer. Validation used the Linux host only; no guest was used.

## Session and consent audit

| Area | Result and evidence |
| --- | --- |
| Current-report index mapping | `session` builds candidates from the current report's control results, requiring `attention`, membership in `engine.available()`, and `NextStep::Repair` (`guided.rs:180–196`). Displayed labels and selected IDs share that same candidate vector. Findings and unrelated report row numbers are not selection inputs. |
| Bounds and duplicates | `Input::choose_inner` rejects out-of-range single selections. `Input::multiple` validates every index before indexing candidates, then sorts and deduplicates (`guided.rs:75–110`). No text-to-command or text-to-ID parsing is involved. |
| Empty checklist | `Some(vec![])` prints “Nothing selected. No changes made.” and continues to the root menu (`guided.rs:194–195`). It does not exit the session. |
| Escape and EOF | Nested checklist cancellation continues to the root; confirmation cancellation is false; next-steps and extra-tools cancellation returns `ToolOutcome::None`. Root `None` exits with zero. Escape therefore backs out of nested menus rather than exiting the guide. Terminal EOF/interruption is sticky in `TerminalMenu`, so the next root read also returns `None` and ends the session. No cancelled choice authorizes apply, undo, or an extra action. |
| Safe consent default | Shared `Input::confirm` supplies `[Yes, No]`, default index 1, and `enter_only = true`; only `Some(0)` consents (`guided.rs:95–97`). Apply, undo, tool actions, broker dispatch, and return-to-check all use this helper. |
| Unchecked defaults | Read-only inspection confirms `TerminalMenu::multi_select` supplies all-false defaults (`menu.rs:228`). A generic mock returning an empty vector is not evidence that native Space/Enter handling or initial checkbox state works. |
| Report invalidation | Confirmed apply and undo set `report = None` before calling the engine, including failure paths (`guided.rs:199–208, 229–238`). Completed-operation reports are retained only for technical details, never as the next selectable audit. Failed rescans replace the old report with `None`. Changed extra actions and extra-action errors also invalidate it. |
| Recovery and technical errors | Failed apply/undo/check paths retain the original error and offer recheck/undo. Technical details display the stored error through `ui::error_details`; successful operation details are explicitly identified as not a new protection check. Extra-action errors propagate into this route; broker failures retain their separate opt-in/details-flag presentation. |
| Fixed broker protocol | Codes 23–27 map only to Bitwarden, Windows Update, Windows Security, encryption, and sign-in. Unknown codes are rejected. The parent loop interprets these only for `guide`. The hidden `--desktop-broker` hint is guide-only and supplies no consent. Broker execution requires an interactive, non-elevated desktop and a fresh false-default confirmation. Declining the return prompt yields false and does not request another elevation. |

### Recovery presentation limitation, with engine gates intact

A successful rescan after a failed apply can again produce `attention` candidates even while an incomplete transaction requires undo. The audit reports journal recovery as a separate finding (`engine.rs:732–770`); the guided checklist filters control results and does not suppress them based on that finding. Thus a fix can still look actionable while the engine returns `pending` instead of applying it (`engine.rs:841–890`). This is a presentation limitation, not stale-report reuse or an authorization bypass.

Unsupported/ineligible observations are `skipped` or `error`, not selectable `attention` results (`engine.rs:741–761`). Eligibility can also change after an audit: `apply_selected` accepts only catalog IDs, reprobes, checks eligibility, and checks it again after durable Prepare before writing (`engine.rs:803–820, 953–1012`). The checklist is an offer to request a fix, not a guarantee that it will remain eligible. These strict engine gates were reviewed as context and remain unchanged.

## Test evidence and coverage limits

- `selections_are_bounded_unique_and_never_commands` covers index bounds, deduplication, empty selection, cancellation, and exhausted-script `None` semantics. Exhaustion does not manufacture a default selection forever.
- `selection_after_rescan_uses_new_candidates_and_none_never_mutates` follows an empty checklist with a recheck and an actual apply to the newly available control. Its scan count and exact applied ID demonstrate return-to-menu and fresh mapping, rather than merely asserting that nothing happened.
- `eof_empty_escaped_and_declined_input_never_modify` records apply and undo calls across root, checklist, apply-confirmation, and undo-confirmation cancellation/default paths. These are session-boundary assertions, not native key-event tests.
- `partial_apply_failure_preserves_cause_and_allows_undo_and_rescan` verifies no second apply from an invalidated report, recovery actions, preserved failure context, and removal of terminal escape characters from shown details. `failed_initial_and_later_checks_never_leave_actionable_stale_results` covers failed refresh invalidation and subsequent recovery.
- Positive apply/undo paths assert actual fake-engine calls. Declined broker and extra-action tests use closures that panic if invoked; positive broker tests assert invocation and the exact request. These checks are stronger than a no-op callback plus a false-result assertion.
- The mock asserts default index 1 when `enter_only` is requested. It does not independently require every confirmation to request `enter_only`; the shared helper was inspected for that requirement.
- The fake engine does not implement real eligibility, journal, or mutation behavior. Its passes do not validate engine gates or Windows actions; those gates were inspected read-only.
- Remaining targeted coverage gaps: successful apply immediately followed by another choose-fixes attempt; nested Escape followed by a demonstrably executed root action; cancellation via a dedicated sticky-EOF session fake; and input I/O error propagation via an error-producing `ChoiceInput`. Existing tests and control-flow inspection support the corresponding behavior, but no dedicated regression tests were added absent a confirmed bug.
- Two native terminal smoke tests are ignored in the ordinary host run. This review makes no claim that real arrow keys, Space, resizing, cursor restoration, or Windows terminal behavior were exercised.

## Host validation

| Command | Observed result |
| --- | --- |
| `cargo test --bin secblitz guided::tests` | Passed: 12 tests; 36 filtered out. |
| `cargo test --bin secblitz` | Passed: 46 tests; 0 failed; 2 native terminal tests ignored; 48 total. Includes current i18n, CLI/broker, menu-unit, and presentation tests. |
| `cargo clippy --all-targets -- -D warnings` | Blocked: Cargo reports `no such command: clippy`. |
| `rustup toolchain list` | Unavailable: `rustup` is not installed/on PATH. |

Toolchain observed: `/usr/bin/cargo` 1.93.1 and `/usr/bin/rustc` 1.93.1. Clippy has **not** passed and remains an outstanding check in an environment with that component. Test counts describe this workspace snapshot, including the current i18n work; later concurrent changes need their own validation.
