# Selected actions: engine integration contract

## Public interface for main

```rust
pub fn available_controls(&self) -> &[Control];
pub fn audit(&mut self) -> Result<Report>;
pub fn audit_with_progress(
    &mut self,
    callback: impl FnMut(&str, &str),
) -> Result<Report>;
pub fn apply_selected(
    &mut self,
    ids: &[String],
    callback: impl FnMut(&str, &str),
) -> Result<Report>;
```

`available_controls` returns the validated backend catalog with compiled targets;
it does not probe current state. Main should use catalog IDs, display audit
results, then pass only the user's selected IDs to `apply_selected`. There is no
caller-supplied plan, before-image, target, or eligibility flag to trust.

An empty selection, empty/unknown ID, duplicate ID, or `ALL` token is an error
before lock acquisition, observations, callbacks, findings, or WAL mutation.
There is no wildcard syntax. To select every catalog control, pass every ID
explicitly. A compiled ID absent from this backend's catalog is also rejected.

Audit emits `(control_id, outcome_status)` after each observation (including
failed probes), followed by `("findings", "pending")` before assessment and
`("findings", "complete")` afterwards. `findings` is a phase identifier, not a
selectable ID. Completion means assessment collection finished, not that all
findings are healthy; collection errors remain `unknown` findings. `audit()`
delegates with a no-op callback. `attention` requires a differing target and
engine eligibility, preserving static absent/already-safe registry checks.

## Selection scope and reporting

Selected operations observe, write, report control outcomes, and invoke control
callbacks only for selected IDs, in catalog order. In particular, an unsupported
unselected observation cannot stop the selected operation. The separate
`Report.findings` assessment is still collected through `Backend::findings`;
it is not constrained to selected controls and is not mutation acknowledgment.
Main must not present selected success as an all-controls health assessment.

The public `Report` shape and control statuses are unchanged. Successful new
writes are `applied`; exact recorded targets or already-present fresh targets are
`unchanged`; ineligible controls are `skipped`. No `not_selected` rows are added.
`transaction` identifies the new batch when created, or the blocking recovery
transaction for `pending`. It can be `None` for an owned-only no-op or conflict
batch; it is not a list of all owners. Use `history()` for batch history.

## Successive batches and recovery

Legacy `apply(callback)` preserves its existing all-controls behavior: any
unreverted transaction blocks starting a new one. `apply_selected` explicitly
allows another batch while older batches are sealed and unreverted, provided
new writes affect previously unowned controls. It never refreshes or overwrites
an earlier before-image, and never creates a second active owner.

Before preparing any new entry, all selected owned controls are probed against
the exact expected after-image derived from their recorded original. Exact
matches return `unchanged`. Drift returns `conflict`; probe failures return
`error`. Either blocks the entire requested batch from creating a WAL or writing
new controls, whose outcomes are `skipped`. Service ACL comparisons include the
exact descriptor fingerprint, not just whether the current ACL looks safe.

New controls are observed after selection. The existing durable Prepare, fresh
eligibility/value gate, write, exact readback, Applied, and Sealed protocol is
shared with legacy apply. Preflight is not authorization to bypass the final
gate. External changes can still race any operation; failures after Prepare
retain a recoverable incomplete transaction. This is a recoverable batch, not
an atomic multi-control OS transaction.

Any incomplete or reverting active transaction blocks selected writes and emits
`pending` for selected controls without control probes. Loading supports multiple
unreverted transactions. Legacy overlapping-owner journals remain loadable for
recovery, but cannot be extended with selected writes.

`revert(callback)` restores only the newest unreverted batch, in reverse entry
order. Conflicts or interrupted rollback keep that batch active, preventing
earlier batches from being undone until the newest is fully restored. A second
revert call then restores the preceding batch. This also applies after reopening
the engine; ownership comes from validated WAL entries, not process memory.

## Verification

Engine tests cover upfront invalid-selection rejection, selected-only probes and
callbacks with an unsupported unselected control, disjoint successive batches,
reopen and reverse undo, original WAL retention, mixed-batch exact ACL drift,
pending/reverting blockers, final-gate races, and audit progress. Existing legacy
apply, journal corruption, exact ACL readback, and rollback tests remain in force.
Checks run locally with `/tmp/opencode/secblitz-cross-env.sh`; no guest operations
are required.

### Translation handoff to main/UI owner

The engine adds these fixed diagnostics, which need entries in the existing
translation catalog (outside this change's file ownership):

- `Select at least one control`
- `Duplicate selected control: {id}`
- `Unknown selected control: {id}`
- `Duplicate active control owner; revert before applying`
- `Selected batch blocked by an owned control conflict or probe failure`

Until those entries are added, the binary's
`i18n::tests::fixed_rust_diagnostic_prose_has_catalog_coverage` check reports them
as untranslated. Control statuses require no new translation keys.
