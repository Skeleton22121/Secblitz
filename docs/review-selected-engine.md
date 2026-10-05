# Selected engine review - reviewer 2 of 4

Scope: `src/engine.rs`; guided/main, backend and permission code read for context only. No guest operations.

## Translation handoff

New fixed diagnostic source keys for the parallel i18n owner:

- `Incomplete transaction precedes another active transaction`
- `Duplicate active control owner; journal history is invalid`

The former `Duplicate active control owner; revert before applying` diagnostic is replaced. Pre-prepare observation failures reuse the existing `error` outcome and raw backend error detail; there is no new status or interpolated message.

## Confirmed bugs fixed

1. **History-wide validation was missing.** Independently well-formed WALs could contain overlapping active owners or an incomplete older batch beneath a newer active batch. Only selected apply rejected duplicate ownership, after other operation paths had already accepted that history. `load()` now rejects both conditions before audit observations, callbacks, apply, or rollback. Disjoint sealed batches remain valid, with at most the newest active batch pending/reverting. The old synthetic overlap-recovery test now verifies fail-closed behavior instead. This deliberately supersedes the overlapping-owner recovery exception documented in `selected-actions-design.md`: neither normal legacy apply nor selected apply can produce overlapping active ownership.

2. **Initial selected probe/target errors discarded the completed-operation report and stranded earlier writes unsealed.** Before a control's Prepare record exists, observation/validation/target derivation errors now become an `error` row and callback, allowing earlier successful entries to seal. Runtime-unsupported and malformed observations are covered, including reopen, a later disjoint batch, and exact reverse undo. Legacy apply retains its error-return semantics. Failures after Prepare still stop with an incomplete recoverable transaction; no partial apply resume is introduced.

## Invariants reviewed

- Empty selection, empty/unknown/duplicate IDs fail before locking, probes, callbacks, findings or WAL changes. Inputs are exact catalog IDs, never paths, targets or executable data.
- `available_controls()` borrows the validated immutable catalog without probes. Audit emits control outcomes then findings pending/complete; unknown assessment results are not success claims.
- Only selected control IDs reach observe/write in selected apply. Separate `Backend::findings()` remains a broad read-only assessment as specified by the API, not a selected-control probe or write acknowledgment.
- Guided mode calls selected apply repeatedly; legacy all-controls apply still blocks new batches while any unreverted transaction exists.
- All selected existing owners are checked before new controls are prepared. Drift or an owner probe error blocks the whole requested batch. Original before-images are reused without refreshing; disjoint batches stack, reopen and undo newest first.
- Service targets derive from exact recorded originals for owned comparisons, write readback and restore. Safe-looking current DACL drift cannot redefine the expected result. Owner, group and protection fingerprint checks remain in existing regressions.
- Full WAL parsing precedes replay, including completed history. Duplicate/unknown fields, invalid values, identities, filenames, oversized/truncated records, and sequence collisions fail closed. Limits remain 128 KiB per record, 1 MiB per WAL, 2,048 transactions; before-image deserialization rejects duplicate object keys.
- Machine identity is nonempty, bounded to 256 bytes and excludes control characters; headers must match it. Journal JSON serialization treats strings as data. Native preference requests validate fixed IDs and typed value domains before script interpolation; service descriptors use the bounded native parser.
- The operation lock spans load, probes, callbacks, writes and assessment. Windows protected-directory enforcement, no-delete-sharing handles, reparse/hardlink rejection, exact handle identity and append-length checks remain intact. Storage failure poisons the engine instance. Durable intent always precedes a write.
- Final gate, write, readback and persistence failures retain the original before-image for explicit revert. They intentionally do not imply retrying apply is safe.

## Verification

- Final host `cargo test --lib`: **79 passed** (includes concurrent owners' tests).
- Host library Clippy with `-D warnings`: passed.
- Windows GNU library Clippy with `-D warnings`: passed.
- Windows GNU `cargo test --lib --target x86_64-pc-windows-gnu --no-run`: passed; Windows test executable cross-compiled, not executed in a guest.
- `rustfmt --check --edition 2021 src/engine.rs`: passed.
- Full host `cargo test`: library passed; binary had 22 passes and three i18n failures: `permission_evidence_and_identifiers_survive_all_languages`, `privilege_controls_and_autologon_evidence_are_localized`, and `fixed_rust_diagnostic_prose_has_catalog_coverage`. These cover renamed presentation labels and pending translation keys across concurrent changes, including the two keys above. An intermediate internal `expect` string also appeared in that run's coverage failure; the final code removes the panic entirely by handling the optional target structurally.
- Windows GNU cross-target Clippy with `--lib --tests -D warnings` was blocked by the out-of-scope `src/ui.rs` dead-code diagnostic for `render_table`. No warning suppression or caller edits were made.

## 0.4.2 release integration fix: updater entries in the journal root

The published 0.4.1 updater leaves its staging files in the shared ProgramData Secblitz root. Previously, `Engine::load()` rejected these as `Unexpected journal entry`, preventing the guide from opening after an otherwise successful update.

The loader now reserves only these exact, case-sensitive names:

- Files: `update.lock`, `update-status.json`, `update-manifest.json`, `update-installer.exe`, `update-worker.exe`.
- Directory: `Updates`.

Each reserved file passes non-following path metadata checks and read-only handle validation through `file_safe()`, including Windows hardlink/reparse checks. No append or write access is requested: the old installer/worker may still hold read-sharing-only handles. The inspection handle shares read/write/delete so it does not obstruct updater-owned operations. The protected root remains the replacement-race trust boundary.

`Updates` must be a real directory, never a file, symlink or reparse point. The engine does not scan its contents; production platform state-directory validation supplies recursive ACL/owner protection. Updater bytes are never parsed, restored or executed by the engine. The engine lock remains `engine.lock` in the base directory. Reserved entries are skipped before the transaction-count check, so they do not consume `MAX_TRANSACTIONS` slots.

Unknown names, suffix/prefix lookalikes and corrupt WALs still fail closed. No wildcard or extension-based updater exception was added. This fix adds no translation keys and changes no source file outside `src/engine.rs`.

Four regression tests cover:

1. All five legacy files and a populated `Updates` directory coexist with open, audit, selected apply, history and exact transaction revert. Updater bytes remain unchanged. On Windows the fixture keeps read-sharing-only file handles open throughout.
2. Wrong file/directory types, unknown names and `.jsonl` lookalikes are rejected; a corrupt real WAL remains rejected even with valid updater entries present.
3. Hardlinks at every reserved file name are rejected.
4. Unix file and directory symlinks at every reserved name are rejected.

Release verification:

- Host library suite: **97 passed**, including all existing journal/crash tests and the four new regressions.
- Windows GNU library Clippy with `-D warnings`: passed.
- Windows GNU library test executable: compiled and linked successfully with `--no-run`.
- Engine `rustfmt --check`: passed.
- Initial builds hit `/tmp` quota/space limits. Successful retries used the workspace `target` directory for compiler temporary files and Windows build output; no shared build artifacts were deleted.
- No guest operations were performed. Live published 0.4.1 to 0.4.2 updater/installer validation belongs to the main integration run.
