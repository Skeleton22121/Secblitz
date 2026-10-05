# Automatic repair flow review

## Owned changes

Only `src/guided.rs`, `src/ui.rs`, `src/advice.rs`, and this handoff file are edited. Model, engine, main, menu, translations, updater, broker entry points, and native security boundaries stay with their owners. No VM operations.

## Approval and verification

- Root menu order: Fix recommended, Choose what to fix, Review protection and next steps, Check my PC again, Undo my last fixes, Extra tools, Technical details, Exit.
- Both repair paths share one candidate builder: latest assessment, `attention`, actual engine catalog membership, recognized repair advice, deduplicated IDs. Any pending recovery finding or pending control outcome blocks a new plan.
- Fix recommended previews exactly those IDs immediately. Choose what to fix starts with the existing unchecked checklist. The shared final preview offers Apply these fixes, Change selection, Back. Back is the default, and execution requires Enter on Apply; no second Yes prompt. Change selection explicitly explains that boxes start unchecked, and replaces rather than extends the previous selection. Empty, Escape, EOF, and Back do not mutate or rescan.
- The approved plan is bound to its assessment generation. It contains only exact IDs from that snapshot, not extra actions or new candidates discovered later. The engine remains responsible for live preflight/eligibility and re-observation under lock.
- Before an apply or undo attempt, the snapshot is invalidated. Every attempted operation, including a returned no-op, partial report, or error, immediately triggers exactly one audit. The audit is fetched and captured before any fallible result rendering. No retry, automatic undo, network action, restart, or disconnect is introduced.
- Operation errors, latest verification errors, and the last completed operation report are separate. Successful verification does not erase a partial-write failure. Failed verification leaves no actionable snapshot; Undo remains available. Technical details show operation and verification failures separately. External tools still invalidate the snapshot when appropriate and retain their independent consent/broker paths.

## Presentation

- `advice::for_outcome(&Outcome)` consumes typed firewall evidence. Verified Local effective Enabled/Block plus an already-compliant status displays Protected by Windows. It does not claim the underlying raw preference was NotConfigured, since Outcome does not expose raw values.
- Managed authority displays Managed elsewhere; missing, unknown, mismatched, or contradictory effective evidence cannot become a protected claim. Typed evidence does not independently authorize mutation. Legacy advice stays available for non-firewall controls and existing tests.
- Readiness is a compact, wrapping informational line before result groups and totals. It displays caller-available decimal GB, read-only/zero-journal-space notices, AC and battery information, and Windows Update restart status. Probe failures remain unknown; no battery is not applicable; battery at or below 20 percent is a notice. A false update-restart signal does not mean fully patched. No green protection score or new repair candidate comes from readiness.
- `info` findings are rendered under More information and excluded from Protected/Needs your choice totals. Pending and unknown findings remain reviewable. Native readiness/firewall evidence is available only in technical details and JSON.
- Owned Outcome/Report fixtures use defaults for added fields. Existing menu infrastructure supports three-item Enter-only confirmation without modification.

## Translation handoff

All new human-facing copy uses English source keys through `Lang::t`, `say`, or `choose_inner`. No em dash is introduced. New keys for the translator:

```text
Fix recommended
The new check could not finish. Check again before choosing more fixes. Undo is still available.
Only these fixes will be applied. Some changes may need a restart. Extra tools and software installs are not included.
Apply these fixes?
Apply these fixes
Change selection
Choose the fixes to keep. The boxes start unchecked.
Rechecking your protection
Technical details of the latest check failure:
Protected by Windows
Windows is already providing this firewall protection. No change is needed.
The active firewall setting could not be verified. Check again before making changes.
For your information
More information
Before making changes
Windows drive
Saved changes drive
Free space unknown
{gb} GB free
Disk is read-only. Fixes will wait.
No space for saved changes. Fixes will wait.
Power information unknown
Plugged in
Not plugged in
Power source unknown
Battery: not applicable
Battery: {percent}%
Low battery. Connect power before making changes.
Battery level unknown
Battery information unknown
Windows Update needs a restart. Save your work and restart when ready.
No update restart pending
Update restart status unknown
Readiness evidence
Firewall evidence
```

Keep `{gb}` and `{percent}` placeholders intact. Existing keys such as Back, Review your selected fixes, and Nothing selected are reused. Broker, updater and extra-action strings are unchanged.

## Tests and integration status

- Queued fake assessments reject unexpected extra audits and supply post-operation state explicitly. Semantic root-choice events replace brittle numeric root indexes.
- Coverage includes recommended one-approval batches, individual omissions, change-selection cancellation, newly appearing candidates needing separate approval, pending recovery, no candidates, no-op/partial reports, operation plus verification errors, retained Undo, and verification even when result output fails.
- UI/advice cases cover typed Local/Managed/Unknown evidence, wrong evidence variants, information-only findings, disk counters above 4 GiB and `u64::MAX`, read-only/zero space, absent/unknown/low battery, unknown/true/false update restart, and narrow wrapping without readiness inflating counts.
- The old `/tmp/opencode/secblitz-cross-env.sh` is absent in this environment. Actual checks use `source target/build-tools/cross-env.sh` (persistent Rust 1.93.0 and Windows GNU tools).
- Initial host production `cargo check --bin secblitz` passed. Initial focused test compilation was blocked by main-owner fixtures at `src/main.rs` missing `Report.readiness` and `Outcome.effective/authority`. Initial Windows compilation was blocked by the `Observation` initializer in `src/permissions/windows.rs` missing `effective/authority`. Owners can migrate these with defaults; they are not edited here.
- Final host production `cargo clippy --bin secblitz -- -D warnings`: **passed**, after simplifying the snapshot-generation guard.
- Requested focused commands were both attempted against the actual sources: `cargo test --bin secblitz guided::tests` and `cargo test --bin secblitz ui::`. Both remain **blocked before test execution** by the two `src/main.rs` constructors at lines 1121 and 1143. No owned-file compile errors were reported in those attempts. The tests must be rerun after the main owner adds the defaults.
- Final Windows `cargo check --target x86_64-pc-windows-gnu --bin secblitz` remains **blocked** by `src/permissions/windows.rs:242`. This initializer needs `..Observation::default()` or explicit `effective: None, authority: None`. No other-owner file was changed to bypass the integration blocker.
