# 0.5.0 experience integration review

## Scope

Owned files: `src/main.rs`, `src/guided.rs`, `src/ui.rs`, `src/advice.rs`, `src/i18n.rs`, and this review. Model, engine, platform, readiness collectors, updater, installer and website implementation were not edited. No VM operation, live update check, publication or deployment credential was involved.

## Confirmed fixes

1. **Confirmed storage blockers affect CLI review status.** `needs_review` now calls the shared typed `Readiness::blocks_repairs()` method. A known read-only Windows or saved-changes volume, or known zero available space on the saved-changes volume, causes review exit code 2 even if all security outcomes are otherwise compliant. Unknown probes, low battery, unplugged power, update restart notices and zero space on the Windows volume alone do not add a new CLI failure rule. They remain informational. Regression coverage includes values above 4 GiB and independently unknown probes.
2. **Contradictory effective evidence cannot turn an applied firewall result green.** An `applied` result accompanied by effective Disabled/Allow evidence now remains Needs your choice / Check again. Verified Local Enabled/Block can retain Fixed after an apply, while a previously compliant result can say Protected by Windows. Missing/wrong-variant/unknown-authority evidence is not a protection claim. The newly distinct `Relevant policy is configured: assessment only` reason is recognized as managed rather than inferred from arbitrary text.
3. **Post-operation verification precedes result output.** Removed the pre-audit status print from `attempt_and_verify`. Every attempted apply or undo fetches and captures exactly one audit before any result or failure rendering can fail. Audit progress remains the engine's existing progress path. Operation failure, latest verification failure and the last completed operation report remain separate.
4. **Device readiness has a plain informational heading.** The normal report says Device check. The readiness progress phase uses the same label. Readiness evidence and Firewall evidence remain technical-details labels; their serialized typed values and raw JSON field names are not translated.

## Approval and recovery invariants reviewed

- Fix recommended and Choose what to fix share the same fresh-report candidate builder: available exact control ID, `attention`, recognized repair advice, deduplicated selection, and no pending outcome/recovery finding.
- The recommended path previews only that snapshot's candidates. The final Enter-only choice defaults to Back. Changing the selection starts unchecked and replaces the prior selection; it never silently extends approval.
- The assessment generation is checked before dispatch. The engine independently performs live eligibility and state checks under its lock. UI generation is not authority to bypass those checks.
- Both successful and failed apply/undo attempts trigger one fresh audit. No-op/partial reports also verify once. Back, Escape, empty selection or declined approval neither writes nor starts an extra audit.
- Newly discovered candidates require another explicit approval. Successful verification does not erase an operation failure or replace the last operation's pending/error outcomes with success.
- A failed verification leaves no actionable snapshot and retains Undo. Result-rendering failure cannot prevent the already-captured verification. No retry, restart, disconnect, extra action or network action is added to a repair approval.

## Localization and presentation

All new automatic-flow, readiness, typed firewall validation, backend policy and effective-state diagnostics have complete English, Spanish, French, German, Portuguese and Italian entries. The existing five-column catalog plus Italian catalog is retained with strict uniqueness and parity checks.

- Exact copy from `docs/review-auto-flow.md` is checked, including `{gb}` and `{percent}` preservation.
- Model and readiness sources are included in fixed-prose coverage. Backend coverage now checks `ThrowGate` messages as well as ordinary throws and finding/reason keys.
- Complete template keys are checked before the coverage lexer strips Rust interpolation fields. This verifies actual lookup keys such as `{gb} GB free`, not unrelated fragments.
- Six-language UI tests verify wrapping at 40/80/100 columns, decimal GB above 4 GiB, battery notices, no native evidence in the normal report, and readiness/information rows excluded from protection totals.
- Unknown, absent and false readiness values retain distinct meanings. No battery is not applicable; unknown power is not interpreted as unplugged; no update restart pending is not a patch-compliance claim.
- Informational findings stay under More information. Attention and unknown findings remain reviewable, not relabeled as protected merely because a check is advisory.
- The tagline remains A safer PC. Without headaches. Arrow/Space/Enter menus remain unchanged. No new U+2014 punctuation is introduced.

## Verification

Used the persistent toolchain environment:

```sh
source target/build-tools/cross-env.sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings
rustfmt --check --edition 2021 --config skip_children=true src/main.rs src/guided.rs src/ui.rs src/advice.rs src/i18n.rs
```

Full host suite after the fixes: **128 library tests passed, 0 failed, 1 ignored; 66 binary tests passed, 0 failed, 2 ignored**. This includes guided and UI/advice regressions, all **13 localization checks**, the storage exit-code test, and the concurrent engine/readiness tests. Doc-tests passed with no cases. Host and Windows-target Clippy passed with warnings denied. Final formatting checks passed for all five owned Rust files with module recursion disabled. The cross-target result is compile checking, not guest execution.

Ignored cases are the opt-in live public updater check and two attended keyboard/resize probes. They were not executed by this review. Existing updater commands, quiet worker behavior, machine JSON and publication configuration are unchanged.
