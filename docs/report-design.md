# Consumer report contract

The heading is **Your PC, checked.** (bold). The brand tagline is **Less worry. More protection.** Counts refer to checks, not unique risks or a security score.

## Layout

- Above 60 terminal columns: bordered, wrapping table with **Protection**, **Status**, **Why it matters / Next step** columns. Tested at 80 and 100 columns.
- At 60 columns or fewer: stacked bordered cards with the same three labels. Tested at 40 columns.
- All borders use **rounded corners** (`╭╮╰╯`) with `─│┬┴┼├┤` for internal lines.
- The third column / card field shows the impact line (e.g. "Risk: Malware running as soon as it lands on your PC") **dimmed** in the wide table, followed by the next-step text. When impact is empty only the next-step text is shown. Newlines within a cell use the multi-segment `wrap()` path which handles embedded `\n` as hard line breaks.
- The **status cell** in the wide table shows a coloured chip: `✓ Good to go` (green), `! Can fix` (yellow), `↻ Restart needed` (yellow), `? Couldn't check` (dim), `• For your information` (dim). Chip colour is derived from the leading glyph character of the plain-text chip string, so `cells()` still receives pre-styled strings and `console::measure_text_width()` measures them correctly.
- Output uses console display-cell widths, including wide Unicode text. Redirected output defaults to 80 columns; console coloring follows the console crate's terminal support. When colors are disabled (non-terminal or `NO_COLOR`) no ANSI escape codes appear in output.
- Groups: **Recommended fixes**, **Protected**, **Needs your choice**, **More information**. Empty groups are omitted. Brief totals include the first three groups.
- Group headings carry a glyph and item count: `! Recommended fixes (N)`, `✓ Protected (N)`, `? Needs your choice (N)`, `• More information (N)`.
- Totals line uses the same icon prefix per group: `! Recommended fixes: N · ✓ Protected: N · ? Needs your choice: N`.
- Recommended controls alone receive stable, one-based numbers, in report order. Findings have no selection number. Guide code must select using the same filtered order (`ui::advice::for_control(...).step == NextStep::Repair`), not the original unfiltered result index.

## TUI report text (`guided_report_text`)

The TUI view renders a document body where each line is styled by its first non-space token:

- `▸ ` — group heading (cyan bold in the menu renderer)
- `! ` — recommended fix item
- `✓ ` — protected item
- `↻ ` — restart-required item
- `? ` or `• ` — choice or information item
- `  · ` — impact or note line (indented)
- `  ` — guidance/next-step line (indented)
- Blank line — separator between items

Items are grouped by `advice::Group` in the order Recommended → Protected → Needs your choice → More information, each group preceded by a `▸ {name} ({count})` heading. Recommended items are numbered (`! 1. Name — Status`). All others use their label without a number (`✓ Name — Status`). Impact lines are rendered as `  · {prefix} {phrase}`. The em-dash separator between label and status uses U+2014 (—).

`guided_report_text` is also the per-item result renderer in `attempt_and_verify` — it displays the apply-report items with updated statuses.

## Guided flow: recap (`approve_plan`)

The recap heading line is prefixed with `▸ `: `▸ Selected fixes: N · May need a restart: N`. Each fix is prefixed with `↻` when it requires a restart (its id is in `restart_ids`) or `!` otherwise. Ambiguous labels append the id in parentheses. Each fix is followed by:

- `  · Needs a restart to finish` (restart-required items only)
- `  · {impact line}` (when impact is non-empty)

Info lines (Restart / Undo / Next) are prefixed with `· ` (middle dot + space).

## Guided flow: fix results (`attempt_and_verify`)

The payoff block renders **before** the per-item `guided_report_text` content. It appears only when verification succeeded and this was an apply (not undo):

- `▸ You're now protected from:` followed by `✓ {phrase}` lines
- `▸ After you restart, you'll be protected from:` followed by `↻ {phrase}` lines

Failure lines (operation failed, post-check failed) use a `✗ ` prefix.

## Truthful status and advice

`src/advice.rs` is exposed as `ui::advice` without changing main or library module wiring. `for_control` and `for_finding` return translation source keys and a structured, presentation-only `NextStep`. These values are not backend action IDs or mutation authority; the guide should explicitly match supported backend actions. No arbitrary URLs or commands are emitted.

- Only recognized controls with outcome `attention` receive **Can fix**, a number, and “Secblitz can fix this”. Backend eligibility remains authoritative at apply time.
- `compliant` / `ok`: **Good to go**, scoped to this check.
- `applied`: **Fixed**; the exact restart-required detail instead yields **Restart needed**.
- `unchanged`: **Good to go** only for exact target-already-present reasons. An original setting restored by undo is not evidence of protection.
- `restored`: advise a new check, never label it protected. The exact restart-required detail yields **Restart needed**.
- `unknown` / `error`: **Couldn't check**, with control-specific guidance.
- `skipped`, `review`, `pending`, `conflict`, `info`, unknown status: **Needs your choice**, unless an exact recognized management reason supports **Managed elsewhere**.
- Preserved absent/nonzero or already-safe preferences may already protect the user, or use defaults. The model does not distinguish those cases; the UI does not declare them healthy or managed.
- Findings never offer automatic repair, even if their titles describe a repairable area. All 14 platform finding titles and five service-permission finding titles have specific advice. Journal recovery and failed assessment have separate guidance.

## Impact phrases

Each `Advice` carries a `pub impact: &'static str` translation source key — a noun phrase naming the concrete threat that check guards against. It reads naturally after any of three prefixes: “Risk: …”, “Protects you from: …”, or “Why it matters: …”. The prefix is determined by group:

- `Group::Recommended` (attention) → prefix “Risk:”
- `Group::Protected` → prefix “Protects you from:”
- All other groups (Choice, Information, Restart needed) → prefix “Why it matters:”
- Empty impact → no impact line rendered (returns empty prefix and no line).

The `impact_prefix()` method returns “” when `impact` is empty. The `impact_line(lang, &Advice)` helper in `src/ui.rs` translates prefix and phrase separately and returns `None` when impact is empty.

Rules for impact content:
- Control ids with recognized names each have a non-empty impact phrase.
- Aggregate/fallback ids (“findings”, unknown controls) have empty impact.
- Finding titles with direct user-facing consequences have phrases; informational, audit, and journal findings have empty impact.
- Impact phrases do not change based on status — the same phrase appears whether the check is Protected, Recommended, or Couldn't check.

## Payoff evidence rule

After a successful fix-and-verify cycle (`attempt_and_verify` in `src/guided.rs`), the payoff section is shown only when:
1. Verification succeeded (`state.snapshot` is `Some`).
2. The operation was an apply, not undo (`ids` is `Some`).
3. At least one attempted id has a non-empty impact phrase.

An attempted id qualifies only when the apply report shows `status == "applied"` for it (Secblitz changed it in this batch) **and** the fresh post-check classifies it as `Protected` via `for_outcome` (so contradictory firewall evidence, still-`attention` and unverifiable results never qualify). Qualifying ids are split:

- **"After you restart, you'll be protected from:"** when the apply detail is exactly `Preference applied; restart required`. The post-check reads the saved setting, so it is already compliant before the restart; that is never shown as protected now.
- **"You're now protected from:"** otherwise.

If nothing qualifies, the payoff section is omitted entirely. An id with empty impact is always excluded. Duplicate impact phrases (e.g. two update-service controls with the same phrase) are deduplicated to one entry per list.

Normal reports use curated friendly labels and advice only. They exclude raw evidence, registry enums, service descriptors, machine IDs, and transaction IDs. History also hides transaction IDs unless details are enabled. Saved-change messages explain undo availability; recognized undo outcomes instead recommend checking again.

## Integration

`Ui::new(lang, no_animation, json)` remains unchanged. Use `.with_details(true)` to append technical evidence to a normal report, or `report_details(&Report)` for a guide's explicit details choice. Evidence is terminal-sanitized; control IDs and transaction IDs are included there. `render_table(Vec<(String, String, String)>)` renders already-localized guide rows with the same adaptive layout.

JSON returns the original serialized report with existing pretty-printing and newline, bypassing advice and display sanitization. A details request in JSON mode likewise serializes the original report. Progress uses friendly control labels, with “Additional protection checks” for the findings phase.

All report copy goes through `Lang::t`; source keys are documented in `report-copy-keys.md`. Raw technical evidence continues through the existing detail renderer. Unknown titles never leak arbitrary evidence into the summary.
