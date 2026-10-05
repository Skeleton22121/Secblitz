# Independent 0.5.0 review - engine/contracts (1 of 4)

Date: 2026-10-03. Baseline contract: [automatic-features-plan.md](automatic-features-plan.md).

## Scope and disposition

Reviewed `src/model.rs`, `src/engine.rs`, and `src/permissions.rs`, with read-only inspection of Windows evidence and CLI integration. Changed only `model.rs`, `engine.rs`, and this review. No confirmed defect required a permissions-wrapper edit. No VM execution, website/updater changes, or live public updater audit.

**Two confirmed engine defects fixed**, each reproduced by a failing regression before the production fix. Library tests and host library Clippy pass. Full integration is not green: translation coverage and Windows readiness compilation need their respective owners.

## Confirmed defects and fixes

### 1. Firewall apply could acknowledge unverified effective readback

The engine checked only `readback.value == expected` before appending Applied and Sealed. Typed observation validation checks field/control compatibility, but deliberately does not prove effective protection. Consequently raw Block/true with missing, contradictory, nonlocal, or ineligible evidence could be accepted after a successful backend write. Native verification reduces exposure but does not close a subsequent observation race or enforce this invariant at the Backend trait boundary.

Fix: after exact raw readback verification, firewall apply also requires the existing `firewall_protected` predicate. Failure retains the original Prepare and leaves recovery pending; it does not acknowledge Applied or seal the transaction. Both apply entry points share this check. Successful outcomes still carry actual readback evidence.

Regression: `firewall_readback_requires_effective_protection_before_sealing` exercises both apply APIs with missing effective evidence, mismatched inbound/enabled evidence, missing authority, Managed/Unknown authority, and Local but ineligible readback. It verifies the write occurred once, the exact original remains pending, and reopen/undo restores it.

Undo intentionally does not require a protected result: restoring false, Allow, or NotConfigured is an exact-state restoration, not a new protection claim.

### 2. Blocked readiness misreported already-owned no-ops

Selected-apply preflight correctly recognized owned raw targets as unchanged, but the readiness-blocked branch discarded those outcomes and reported every selection as skipped. This falsely implied a readiness barrier prevented an already-completed selection.

Fix: retain preflight unchanged outcomes for owned selections; return readiness-skipped outcomes for the remaining selections. The branch returns before durable flush/new WAL/Prepare/backend writes. It does not remove the readiness evidence or authorize a new write.

Regression: `blocked_readiness_preserves_owned_noops_and_existing_wal_bytes` exercises owned-only and mixed selections with an existing sealed transaction, zero journal bytes and read-only evidence. It checks exact unchanged journal bytes, one existing WAL, no additional writes, and unchanged/skipped statuses as appropriate. The existing-active legacy apply path is also checked for byte preservation.

## Invariants checked

| Area | Review result |
| --- | --- |
| Fixed catalog and raw originals | The six exact firewall IDs retain true/Block targets. Effective evidence never enters schema-1 Prepare records or restore payloads. Catalog targets remain compiled and validated. |
| Safe inherited defaults | Eligible Local NotConfigured plus effective Block is compliant on audit and unchanged on both new-apply APIs. All three profile tests assert zero writes/new WALs and preserved raw NotConfigured. |
| Evidence cannot grant eligibility | Managed/Unknown plus eligible=true is rejected by model validation. Missing metadata is representable for old producers but cannot prove protection or authorize a firewall repair. Wrong-ID/kind evidence is rejected. |
| Contradictory explicit settings | Eligible Local raw Allow/effective Block and enabled true/effective false require review/error before new intent. Native Unknown/ineligible mismatch observations remain skipped and cannot write. |
| Final pre-write gate | Re-observation must remain eligible, match the exact raw original, and remain a genuine gap. Evidence loss or an inherited default becoming protected after Prepare retains pending recovery without a backend write. |
| Owned comparisons | Existing owners compare exact original-derived raw targets. Raw Block drifting to NotConfigured conflicts even if effective Block is verified. These ownership statuses are not fresh protection assessments. |
| Post-write check | Exact raw target and effective Local eligible protection are now both required for firewall apply. Gate/Windows source was inspected; no gate was removed. |
| Exact undo | Restore eligibility and exact raw checks remain authoritative. The legacy twelve-control schema-1 fixture now explicitly tests restoring a NotConfigured original while effective inbound remains Block. |
| Parser/history | Missing/duplicate/unknown/executable before-image fields remain rejected. Explicit null for an absent registry value remains required; no permissive fallback or schema change was introduced. Whole history validation still precedes replay. |
| Readiness | All-Unknown is informational and permits otherwise eligible apply. Only confirmed system/journal read-only or zero journal bytes blocks new repairs. Power, reboot, and other space observations are informational. |
| Existing-active early returns | Pending recovery and owned conflicts may omit readiness because they perform no new intent/write. The legacy sealed-active branch likewise makes no new writes. Moving readiness ahead of these branches is unnecessary for the no-new-repairs requirement. |
| Storage boundary | Engine opening/locking/loading can still fail on actual storage access restrictions before a readiness report exists. Readiness does not bypass those checks or promise that a clear volume flag proves write access. |
| Undo/readiness | Undo never probes or gates on readiness. Real journal storage, exact-state, and management/eligibility checks still apply. Original ownership and before-images are retained. |
| Permission wrappers | Both adapters forward readiness directly; the nested adapter test proves exactly one delegate readiness call and no machine/catalog/observation/findings/write calls. No health data is added indirectly through the wrappers. |

## Owner handoffs

### Main/CLI: blocked readiness is absent from exit-status review

`main::needs_review` currently examines only outcome/finding status strings. A report whose controls are compliant and findings informational can therefore exit 0 despite known blocked readiness. This is a reporting gap, not a core repair authorization bypass: core apply still blocks new changes.

Added the public shared `Readiness::blocks_repairs(&self) -> bool` and switched engine apply to it. Recommend the main owner include:

```rust
report.readiness.as_ref().is_some_and(|r| r.blocks_repairs())
```

in `needs_review`, alongside the existing status checks. Test compliant-only reports with each of system read-only, journal read-only, and zero journal bytes. Unknown and informational power/reboot reports should not become failures merely because readiness exists. Keep readiness outside repair/protection counts. No main-file edits were made in this review.

### Translation owner

The full binary suite currently fails these three coverage tests:

- `i18n::tests::backend_fixed_errors_titles_and_advice_are_translated` (first missing key: `Firewall stored profile cannot be established`).
- `i18n::tests::guided_fixed_copy_uses_whole_keys_and_localized_keyboard_hints` (first missing key: `Fix recommended`).
- `i18n::tests::fixed_rust_diagnostic_prose_has_catalog_coverage` (automatic-feature guided/advice/UI/engine copy).

This fix also introduces the diagnostic template `Apply {} effective protection is unverified; pending transaction {} retained`; include it in the translation handoff. Existing missing authority/effective/mismatch/readiness keys are still reported by the same test.

### Windows readiness owner

Windows all-target compilation currently fails at `src/readiness/windows.rs:393`: `(class == EXPECTED)` compares `windows_sys::core::GUID`, which does not implement `PartialEq` in windows-sys 0.59. Compare the GUID fields explicitly or use an appropriate supported comparison. This is outside engine ownership. The previously reported missing Report/Outcome constructor fields did not recur in host compilation; Windows success cannot be claimed while this new blocker remains.

## Validation evidence

All commands sourced `target/build-tools/cross-env.sh` (persistent Rust 1.93.0 toolchain).

- Both new regression tests **failed before their fixes** with the expected assertions (false successful readback, skipped instead of unchanged).
- Targeted engine suite after the fixes: **56 passed**. Expanded evidence cases and legacy-default coverage were included in the subsequent full run.
- `cargo test --all-targets`: library **126 passed, 0 failed, 1 ignored**; binary **59 passed, 3 failed, 2 ignored**. The three failures are the translation coverage tests above. Ignored tests are the explicit live public updater audit and attended terminal/PTY tests.
- `cargo clippy --lib -- -D warnings`: **passed**.
- `rustfmt --edition 2021 --check src/model.rs src/engine.rs src/permissions.rs`: **passed**.
- `cargo check --target x86_64-pc-windows-gnu --all-targets`: **blocked by the out-of-scope GUID comparison above**.

No Windows runtime or VM verification is claimed. These results describe the shared working-tree state at review time; other owners are integrating concurrently.
