# Independent review 1/4 - guided consumers

## Final redeploy - Settings broker

- Fixed the elevated-guided Settings failure: `BrokerRequest` is a closed enum with Bitwarden=23, Windows Update=24, Windows Security=25, encryption=26, sign-in=27. No URI, executable, or arguments are carried in the handoff. The four Settings selections never dispatch from the elevated session.
- The original non-elevated process accepts these codes only from its guided child, checks interactive/non-elevated context again and obtains fresh, action-specific consent before invoking the fixed `Action`. Exit codes and `--desktop-broker` are routing hints, not authorization.
- Decline/empty/EOF exits the parent handoff. After an accepted action (including a recoverable failure), an explicit Return confirmation reopens the guide; No/EOF exits without another UAC prompt. This also fixes the previous Bitwarden unconditional re-elevation loop.
- Starting already elevated, without an original parent, explains how to open Secblitz normally. It neither dispatches Settings nor claims they opened.
- Kept the current `A safer PC. Without the guesswork.` CLI description. Menu labels now say `Choose what to fix` and `Check my PC again`; technical evidence stays option 6. Selection now consumes translator-updated `Lang::control`, which uses the friendly advice labels.
- Removed unused `Ui::render_table`; report rendering still uses its tested internal `write_table`. No dead-code allowances added. Explicit `service start` is covered by a canonical-arguments regression test.
- Added tests for exact codes/actions, unknown-code rejection, child consent, direct-admin explanation, parent re-consent decline/EOF, accepted handoff/return, failure evidence, and loop termination. JSON rejection tests still precede interactive/platform work.

### Additional translation keys for this redeploy

```text
A safer PC. Without the guesswork.
[1] Choose what to fix
[3] Check my PC again
Open Windows Update settings now?
Open Windows Security settings now?
Open device encryption settings now?
Open sign-in settings now?
Desktop requests require a non-elevated window
That action did not finish. You can view the details before trying again.
Return to the PC check? Windows will ask for administrator permission again.
To open Settings from here, close Secblitz and open it normally, without Run as administrator.
Return to your original Secblitz window to open Settings? That window will ask you again before opening anything.
```

The previous Bitwarden-specific failure sentence is replaced by the shared `That action did not finish...` key. Prior-wave keys below still need coverage where used. Translator files remain untouched.

### Redeploy validation

- Full host `cargo test`: 79/79 library tests and 35/36 binary tests passed. The only failure was `i18n::tests::fixed_rust_diagnostic_prose_has_catalog_coverage`, listing new guided/broker phrases and outstanding cross-owner diagnostics. All functional tests passed.
- Windows `cargo check --target x86_64-pc-windows-gnu --all-targets` passed. Initial Clippy found only now-unused `Lang::control`; the selection consumer was switched back to this translator-fixed friendly-label API.
- Final **host and Windows-target `cargo clippy --all-targets -- -D warnings` both passed**, with the Windows invocation adding `--target x86_64-pc-windows-gnu`.
- Final `cargo test --bin secblitz`: **35/37 passed**, including the added redirected-input guard test and every functional guided/CLI/UI/advice test. Two translation tests failed during concurrent catalog edits: missing prose coverage and `catalog_is_complete_and_unambiguous` reporting **duplicate source `A safer PC. Without the guesswork.`**. Translator/main integrator should remove that duplicate and finish the listed keys; no i18n edits were made here.
- No guest operations were performed.

Ownership: `src/main.rs`, `src/guided.rs`, `src/ui.rs`, `src/advice.rs`, this document. No translation, engine, platform, action, service, or installer files edited.

## Changes

- A successful guided check shows compact counts once. The main menu no longer repeats every report row. Option 2 explicitly opens the friendly protection report and next steps; option 6 explicitly opens technical evidence. Selection uses friendly `advice::control_label` translation keys, not technical control names.
- Only current `attention` outcomes in the actual engine catalog and recognized repair advice are selectable. Numeric selections remain bounded, deduplicated and sorted; empty/none/EOF and declined confirmation never apply anything. `all` means all currently displayed candidates, not the entire compiled catalog.
- Apply/undo failures retain their full error chains, explain possible partial work without claiming the remainder ran, and keep the session open for undo/rescan. Initial and subsequent scan failures also leave recovery available. Native error evidence is sanitized for terminal controls and remains opt-in.
- After apply, undo, or an attempted external action, the old protection snapshot is invalidated. New fixes require an explicit successful Check again. No implicit scan pretends an asynchronous Defender/settings action has finished. Canceling an action leaves the snapshot intact.
- Last completed operation evidence remains available separately from the current scan. Last failure evidence stays available even after a successful recovery scan, explicitly labeled as the last failure.
- Extra action failures preserve the actual error rather than replace it with a generic message; worker panics preserve their textual payload. The user is told Defender work may still be running and that extra actions are outside Undo. Unknown action result statuses are errors, not inferred success.
- Reports with transaction IDs no longer imply every requested fix completed. Terminal output uses the actual console width with a spare final column, wraps standard tables/cards, and gates styling by the correct output stream's terminal/color support. Redirected output cannot gain ANSI escapes merely through forced-color environment settings.
- Parent Bitwarden install failure details can be viewed immediately without rerunning the installation. Success is explicitly acknowledged.

## Reviewed boundaries

- No command defaults to `guide`; `execute` opens the actual `Engine` with the permission wrapper and guided apply calls `Engine::apply_selected` with selected IDs only. Revalidation and mutation eligibility stay engine-owned; the UI is not mutation authority.
- Elevation reconstructs fixed arguments from parsed choices, preserves language/details/no-animation, and adds only `guide --desktop-broker` for the guided child. No arbitrary user text crosses the argument boundary.
- Exit 23 and the hidden flag are routing hints, not authorization. The parent acts only on a guided child's exit 23, obtains fresh installation consent, and calls the same `tools::install_bitwarden` entry point as the explicit command. That entry point independently requires non-elevated desktop-user identity. Spoofing the hidden flag in an elevated process can request an exit; it cannot perform an elevated installation.
- JSON is still accepted only for audit/apply/revert/history and rejected before terminal checks/menus/elevation on guided invocations. Raw report serialization is unchanged.
- Engine-selected batch semantics and unknown/server-role eligibility are being reviewed by their owners. This consumer only offers engine-audited eligible `attention` controls, never findings, skipped, unknown, error, or absent catalog IDs.
- Settings dispatch and action execution remain action-owner responsibilities. No guest/runtime UI tests were run here.

## Translation handoff - new source keys

Translator/main integrator: collect existing keys from the owned files as well; this list contains the changes introduced by this review. All entries use `Lang::t` / `Input::say` / `Input::confirm`; do not translate raw native evidence or IDs.

```text
[2] Review protection and next steps
[6] Technical details (optional)
Check again before choosing fixes. The previous check is no longer current.
Check again to review current protection, or undo your recorded fixes.
Technical details of the last failure (may include system paths and native messages):
Technical details of the last completed operation (not a new protection check):
The operation did not finish. Some changes may already have been made; remaining work is not confirmed. You can check again or undo recorded fixes.
Choose Technical details to see the original failure.
Choose Check again when you are ready to verify current protection.
Extra actions are not part of Undo my last fixes.
Bitwarden installation did not finish. Review the failure before trying again.
Show technical details of this failure?
Saved changes are available for review or undo. This does not mean every requested fix completed.
```

New technical diagnostic source strings (interpolated values are evidence):

```text
Worker panicked without a text payload
Action worker failed: {cause}
Unexpected action result: {}: {}
```

The previous `[2] See details and actionable next steps` label and unconditional `Your changes are saved. Undo is available for recorded changes.` message are no longer used by this consumer.

## Verification

- First full `source /tmp/opencode/secblitz-cross-env.sh && cargo test`: 79/79 library tests passed; 22/25 binary tests passed. Failures were translator-owned `permission_evidence_and_identifiers_survive_all_languages`, `privilege_controls_and_autologon_evidence_are_localized`, and `fixed_rust_diagnostic_prose_has_catalog_coverage`. These reflect technical-label expectations and outstanding translation coverage across concurrent work; no i18n files were changed to suppress them.
- Added session mocks: none then rescan selects refreshed candidates; partial apply failure blocks stale apply, retains technical cause and permits undo/rescan; initial/later audit failure recovery; confirmed apply then canceled rollback; failed rollback recovery; extra-action cancellation/EOF never invokes the runner, and failure retains the complete cause.
- Added CLI regression coverage for hidden guide-only broker hint, separation from `--yes`, canonical elevation reconstruction and guided JSON rejection.
- Final full `cargo test` (with the requested environment sourced): **79/79 library tests and 29/31 binary tests passed**. All new session/CLI tests and all UI/advice tests passed. Two translator-owned failures remained: `permission_evidence_and_identifiers_survive_all_languages` (expects old technical BITS label) and `fixed_rust_diagnostic_prose_has_catalog_coverage` (pending source keys across concurrent work). The third initial failure was already fixed by concurrent translation work.
- `cargo check --target x86_64-pc-windows-gnu --all-targets`: **passed**. Only unused-method warnings (`Lang::control`, `Ui::render_table`); no compile errors. This is compile verification, not a Windows guest execution claim.
- Formatted only owned Rust files with `rustfmt --edition 2021 --config skip_children=true src/main.rs src/guided.rs src/ui.rs`; module recursion was disabled to avoid editing translator-owned files.
