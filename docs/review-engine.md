# Engine review - waves 1 and 2, and exact-state control extension

Scope: `src/engine.rs`; platform/model files were read for their contracts, not
modified. This is a source and fault-state review on Linux, with Windows-target
compilation. It is not a Windows VM or power-loss certification.

## Confirmed failures and fixes

### 1. Append trusted a stale validated prefix (defense in depth)

Previously, append checked type/link count and the maximum file size, but not
whether the file length still matched the prefix that was loaded or created.
Truncating a prepared transaction to its complete header and then appending
`Applied` succeeded, despite losing the before-image. Appending a partial external
tail likewise allowed another record to be appended to malformed data. Later
loading rejected the result, but the engine had already reported append success.

Fix: each transaction tracks its validated/durably appended length. A mismatch
rejects append and poisons the engine instance before any additional bytes are
written. Length advances only after successful sync. The engine does not truncate
or repair a suspect journal. Regression coverage changes the actual on-disk file
both at a complete-record boundary and by appending a partial JSON tail, checks
that append fails, and checks that the damaged bytes remain unchanged.

This is not presented as an unprivileged Windows exploit: the production platform
directory ACL is intended to exclude such modification. Same-length in-place
tampering and races after the check remain outside this defense.

### 2. Path/handle identity was not checked (defense in depth)

Type and link-count checks do not prove a held file is still the named journal.
On Unix, renaming the original and placing a same-length regular file at its path
left append writing the old inode. The named transaction then lacked the new
record. A lock opened before a replacement could similarly lock the wrong file.

Fix: compare path/handle identity after acquiring the lock and before append.
Unix checks device/inode; Windows checks volume serial/file index using native
handle queries. Both also retain link/type checks. A Unix regression replaces the
path with a same-length file and confirms neither file gets appended to.

Windows already denies delete sharing and the platform pins directory ancestors;
its new identity branch was cross-compiled, not runtime-tested. Unix identity
checks do not establish a complete hostile-directory security boundary or close
every pathname race. Production backend operation is Windows-only.

### 3. Restore retries unnecessarily consumed the bounded WAL (availability)

Each retry appended `RestorePending`, even when the loaded state was already
`Restoring`. A persistent backend failure before mutation therefore grew the WAL
on every retry, eventually exhausting the 1 MiB limit and preventing recovery.
Final-probe conflicts after entering `Restoring` could also repeat this growth.

Fix: reuse the existing durable restore intent. Revert already syncs loaded
journals before processing them. Append a new intent only when first entering
restoration; retain support for old journals containing duplicate intents.
Regression coverage repeatedly reopens and retries a backend failure before
mutation, checks exact byte equality with the first durable intent, confirms apply
remains blocked, then permits the restore and verifies completion.

## Other independently checked behavior

- **Privileged restore allowlist:** engine targets are compiled IDs, backend
  targets must match them, and journal IDs must be supported by that backend.
  Before-images are limited to booleans, three inbound enums, or strict UAC
  presence/DWORD objects. Typed deserialization rejects duplicate UAC keys,
  missing value fields, extra fields, and executable/untyped values. No journal
  value supplies a path or command.
- **Machine binding:** every loaded header must match the backend machine ID,
  schema, canonical filename, and sequence. All journals are validated before
  replay, including older transactions. Existing tests reject a different machine
  and an invalid older journal before mutation.
- **Write-ahead order:** prepare and restore intent are synced before their
  corresponding preference writes. Backend errors leave an unknown-outcome
  intent. A failed append/sync poisons the instance. Existing tests cover failures
  before and after backend mutation, and a real read-only result handle failure.
- **Torn tails:** added coverage tries every nonempty proper byte prefix of an
  `Applied` record. Open and revert reject every such tail without changing it or
  writing preferences. A completely absent result record is intentionally a valid
  pending transaction and is recoverable.
- **Repeated apply / conflicts:** apply retains original before-images and does
  not start another transaction while one is active. Tests cover partial apply,
  repeated successful apply, and a newest unresolved rollback blocking both
  older rollback and a new apply.
- **Empty transactions:** repeated all-compliant apply creates no WAL. A crash
  after only the header blocks apply until explicit revert, which completes that
  empty transaction without preference writes. Added a regression for both.
- **Links / ACL:** engine rejects symlinks/reparse points and multiple file links.
  Tests exercise Unix journal and lock symlinks and journal hard links. Production
  `Engine::open` requires the platform state directory; its native implementation
  checks owner/DACL and existing children and retains ancestor/root handles.
  The engine does not independently implement ACL validation.

## Residual constraints and findings

1. **Compare-before-write is not atomic.** The final engine probe catches drift
   before that probe (tested), but `Backend::write(id, value)` takes no expected
   current value. Windows starts a separate PowerShell invocation for writing;
   Defender/firewall setters do not compare the engine's expected value. UAC
   repeats a current-value guard, but that guard and its setter are still separate
   operations. Another administrator, policy agent, or system writer can change a
   preference between the last read and mutation. The engine lock only serializes
   cooperating engine operations. Fixing this fully cannot be claimed from an
   engine-only change; a backend contract and suitable OS primitive would be
   needed. Post-write readback detects failed writes, not overwritten intervening
   intent or ABA changes.
2. **Crash durability depends on storage.** Files use `sync_all`; Windows opens
   request write-through, and Unix directory entries are synced. Tests cover
   logical crash states, not hardware flush failure or Windows power cuts. A torn
   record fails closed and requires external investigation; there is no automatic
   salvage. Complete-record truncation before reopening is indistinguishable from
   a legitimate earlier crash prefix; journals have no external authenticated
   monotonic anchor. The new length check only protects a live transaction's known
   prefix.
3. **Trust boundary remains the protected directory.** A hostile administrator,
   a cloned machine identity, same-length authenticated-looking WAL edits, and
   simultaneous ACL changes are not prevented by schema/identity checks. Machine
   identity is captured when opening the engine, not hardware attestation.
4. **Finite retention:** 2,048 transaction files and 1 MiB per WAL remain hard
   limits. No automatic archival/compaction was added. Existing nearly full WALs
   can still lack space for recovery records. Empty/torn creation also deliberately
   blocks subsequent operations rather than deleting evidence.

## Verification

- `source /tmp/opencode/secblitz-cross-env.sh && cargo test`:
  final run: 28 library tests and 13 binary tests passed; zero doctests. Other
  agents' concurrent work increased the binary test count from the first run.
- `source /tmp/opencode/secblitz-cross-env.sh && cargo check --target x86_64-pc-windows-gnu --tests`:
  passed, including the Windows handle-identity branch.
- No Windows runtime, NTFS ACL attack, or physical crash/power-loss test was run.

## Wave 2 - main/backend interactions and journal usability

Re-read the current engine, `main.rs`, report renderer, `model::Backend`, native
platform launcher/backend, PowerShell gate/setters, and journal ACL implementation.
Only engine code/tests and this document were edited. The service does not use
Engine or the administrator journal. Other reviewers are concurrently changing
the main/platform/service code; the observations below distinguish source review,
fake-backend fault tests, and native compilation.

### Confirmed report defects (reproduced before fixing)

1. **Pending recovery could be reported as clean by audit.** A backend can mutate
   successfully and then fail acknowledgment/readback. Apply correctly returns an
   error and leaves `Prepare`, but an immediate audit sees the target preference
   and reports `compliant`. Its journal finding previously used `info` even for
   an unsealed transaction or an interrupted rollback. Main's `needs_review`
   deliberately accepts both statuses, so an otherwise clean assessment could
   exit 0 with an unresolved transaction. The regression
   `audit_marks_unknown_apply_and_interrupted_rollback_as_pending` failed on the
   missing `pending` finding before the fix.

   **Fix:** journal findings distinguish an incomplete apply/rollback (`pending`)
   from a sealed, successfully applied transaction retained for optional rollback
   (`info`). Audit retains current preference observations while independently
   exposing incomplete recovery. Repeated apply and incomplete revert also include
   this journal finding. Successful revert marks its in-memory transaction
   completed after the durable completion record and reports any remaining active
   transaction. No report schema or external caller signature changed.

2. **An unrelated findings failure discarded operation results.** Every report
   path used `backend.findings()?`, including after a durable seal or completed
   rollback. A timeout, stderr/protocol error, or parse failure in this separate
   assessment request therefore lost the completed operation's report and
   transaction identifier. Main exited 1 without rendering the report; JSON callers
   received no operation report even though writes had completed. The regression
   `findings_failure_preserves_reports_and_journal_outcomes` first failed on the
   propagated assessment transport error before the fix.

   **Fix:** assessment-request failure becomes an `unknown` finding with the error
   chain. Existing operation outcomes and transaction identity are retained; main
   already treats `unknown` as review-needed/exit 2. Tests cover audit, successful
   apply, repeated apply, and successful revert with persistent findings failure,
   including reopen and exact write counts. This does **not** downgrade machine
   identity, journal validation/storage, preference observation during mutation,
   or write failures: those still return errors and stop mutation processing.

### Management-gate false positives: engine result and platform handoff

The reported local-device false positive is a **backend usability blocker for
affected controls**, not journal corruption. At initial source inspection, `Gate` rejected
any immediate child under `Microsoft\Enrollments`, OMADM Accounts, or cloud join;
it also rejects any provider values, any RSOP GPO, missing/failed RSOP queries,
and local policy artifacts. These checks do not distinguish active enrollment
from every default/stale artifact. Existing Windows evidence in
`windows-test-results.md` records 33 enrollment subkeys and rejection of the
reversible firewall/UAC fixtures. That evidence identifies the triggering rule;
it does not independently establish whether each key represents active management.
The user reports the local-device result as a false positive. During this wave,
the platform owner replaced the enrollment-template heuristic with
`IsDeviceRegisteredWithManagement` (strict successful-result and boolean checks),
retaining the other management gates. Final read-only inspection also confirmed
global progress suppression in the native PowerShell bootstrap. Those corrections
are now present in source; their native runtime revalidation belongs to the
platform reviewer. No engine gate bypass was added.

Fault tests establish the engine's behavior at the contract boundary:

- All controls rejected before prepare: `skipped`, no transaction, no write.
  Clearing the rejection permits a later apply on the same engine; eligibility
  is not cached.
- Gate rejection at the fresh probe after prepare: pending transaction, no write.
  Revert closes it as `unchanged` while still blocked if the before-image is
  already present; no privileged restoration is attempted.
- Gate rejection after successful apply: revert retains the before-image,
  returns `skipped` plus a `pending` journal finding, and does not write. New apply
  remains blocked across reopen. Clearing the rejection allows explicit revert
  to finish normally with the original before-image.
- A sealed transaction with some previously skipped controls still requires
  revert before a new apply can change those controls. This is the existing
  single-active-transaction rule, not a cached management decision.
- Machine-ID transport failure occurs during open, releases its lock, and leaves
  no transaction. A later successful identity probe can open/apply normally. This
  is covered with a simulated transport error, not a new execution of the CLIXML
  runtime reproduction reported by the other reviewer. Findings fallback does
  not weaken machine binding or mask machine-ID failure.

### Windows-native test coverage added

- `windows_handles_deny_delete_and_release_locks_on_drop`: native competing file
  handles cannot acquire the held exclusive lock; lock deletion and WAL
  rename/deletion fail while pinned; dropping handles permits reacquisition and
  rename, and pending rollback completes without a preference write.
- `windows_native_link_count_rejects_lock_and_wal_hardlinks`: native link-count
  validation rejects hard-linked lock/WAL files before preference writes and
  recovery works after the extra link is removed. Requires a hard-link-capable
  local test volume (normally NTFS).
- Shared engine tests compile for Windows too, including every partial `Applied`
  record and newly added every-partial `RestorePending`, `Restored`, and `Reverted`
  records. Partial records reject reopen without writes or truncation; valid
  zero-result-byte crash prefixes recover without repeating completed writes.
- Tests use temporary directories and a fake preference backend: no Windows
  security preferences are changed, and production state-directory ACL enforcement
  is intentionally not exercised by the engine unit-test fixtures.

### Wave 2 verification and blocker assessment

The two new report regression tests failed before the production fixes, with the
expected missing-pending and propagated-findings-error failures. Full-suite
verification initially encountered an unrelated concurrent API mismatch:
`main.rs` passed `StatusDetails` to `Ui::service_status(&str)`. No edits outside
the owned files were made to resolve it; it no longer blocked the final run.

Final commands, all using `source /tmp/opencode/secblitz-cross-env.sh`:

- `cargo test`: **36 library + 8 binary tests passed**, zero doctests. Counts
  include other agents' concurrent changes (shared tool tests now run only once).
- `cargo test --target x86_64-pc-windows-gnu --no-run`: **passed**, both library
  and binary Windows test executables compiled and linked.
- `cargo build --target x86_64-pc-windows-gnu`: **passed**, Windows application
  compiled and linked. This was a debug build, not a release/package validation.

No additional journal-state-machine or gate-transition blocker was demonstrated
by this review after the report fixes. Runtime validation of the platform's
management correction remains a handoff, and the wave-1 non-atomic preference comparison,
fail-closed torn-WAL recovery, storage durability, and trust-boundary constraints
still apply. Native Windows tests added here are compile/link evidence unless
separately executed by the Windows runtime reviewer; no Windows runtime execution
or physical power-loss test was performed in this wave by this agent.

## Fixed registry and exact-state service-DACL integration

Owned changes: `src/engine.rs` and this addendum only. Read
`docs/control-handoff.md`, platform registry contracts, and the permissions
module's pure descriptor API. No guest or service mutation was performed.

Integration handoff at final source inspection: `main.rs` still constructs Engine
with the plain `platform::backend()`, whose native constructor returns the static
Windows backend. The application/platform owner must wrap that backend with
`permissions::with_permissions(...)` to expose the two service controls to the
CLI. Engine support and adapter tests do not alone establish that live CLI wiring.

### Four static registry controls

The engine now independently allowlists:

| ID | Explicit unsafe original | Fixed target |
| --- | --- | --- |
| `installer.always_install_elevated` | present DWORD 1 | present DWORD 0 |
| `lsa.restrict_anonymous_sam` | present DWORD 0 | present DWORD 1 |
| `lsa.limit_blank_password_use` | present DWORD 0 | present DWORD 1 |
| `wdigest.use_logon_credential` | present DWORD 1 | present DWORD 0 |

Only boolean `present` plus binary integer `value` is accepted, with explicit
null when absent. Apply preserves absence/already-safe values and additionally
enforces the explicit unsafe-original rule even if a backend mislabels absence
as eligible. The same check preserves nonzero/absent UAC settings. Mutation still
requires the backend's current eligibility and unchanged final observation.

Restoration recognizes the exact new platform reason
`Preserving absent or already-safe machine preference` only for these four IDs
and only while the observed value equals their fixed target. It retains the old
UAC-specific reason/ID exception separately. Neither exception permits a
management/capability error to authorize restoration. Absence remains a valid
restore before-image as required by the registry contract, although automatic
apply does not create absent-value repairs. Reboot reporting remains driven by
the compiled backend control metadata; no restart is performed by the engine.

### Two exact-state service controls

Only `permissions.service.bits` and `permissions.service.wuauserv` are accepted.
Their catalog target must equal `"service-dacl-repair-v1"`; actual observed,
journaled and written values must pass `permissions::validate_value`. The marker
is never a descriptor or a native write payload.

`target_for(id, before)` delegates only these two IDs to the pure deterministic
`permissions::repair_target`; other IDs retain their compiled static targets.
Apply computes an exact repair target from the observed descriptor, syncs its
exact before-image in `Prepare`, rechecks unchanged state/eligibility, and writes
the computed descriptor. Repeated apply for a recorded entry and revert both
recompute the expected after-image from the **recorded original**, not the current
descriptor. Thus an intervening safe-to-safe ACL change remains a conflict;
`repair_target(current) == current` cannot authorize stale rollback. Comparison
includes the entire canonical snapshot, including owner/group and retained flags.

Loading validates descriptor envelopes and rejects originals for which repair
is impossible or produces no change. Supported originals cannot become arbitrary
service names, paths, SDDL commands or caller-selected targets. Valid complex or
otherwise ineligible observations are skipped before repair planning. This fixes
an integration edge discovered while implementing the controls: calling the pure
repair helper before honoring ineligibility would abort on a complex descriptor
and leave earlier successful registry repairs unsealed. A regression checks that
the earlier repairs are instead sealed and the service control remains skipped.

### Format, limits and compatibility

- `SCHEMA` remains **1**. Descriptor snapshots use the existing JSON string
  before-image variant; no record type or field was added.
- Record limit increased from 4 KiB to **128 KiB** to accommodate bounded exact
  descriptor strings. The current permissions parser separately bounds binary
  descriptors to 16 KiB. The **1 MiB WAL** and **2,048 transaction** limits remain.
- A literal legacy schema-1 WAL containing all original twelve IDs is replayed
  successfully with the eighteen-control catalog. This is forward reading of old
  journals, not a claim that older binaries understand the six new IDs.
- Future descriptor-repair changes must preserve the v1 transformation for v1
  journals: recovery derives the expected after-image from that algorithm, not a
  separately stored after-image. No wall-clock timestamps or implicit defaults
  are used to reconstruct it.

### Added verification

The new tests cover all four binary targets, absent/already-safe preservation,
strict malformed/beyond-binary rejection before replay, gate-blocked restoration,
exact original/absent rollback, and literal legacy twelve-control WAL replay.
Descriptor fixtures independently construct canonical security descriptor bytes;
expected repaired bytes are not obtained only by reusing the helper under test.

ACL coverage includes both fixed service IDs, deterministic repair/no-op targets,
catalog-marker enforcement, rejection of foreign IDs and malformed/impossible/
already-safe WAL originals, before- and after-mutation apply failures, unknown
restore outcomes across reopen, safe intervening drift conflicts, and a snapshot
well beyond the former 4 KiB record limit surviving apply/reopen/revert. Existing
torn-record, storage-error, locking and pending-review tests remain in the suite.

Verification checkpoint using `/tmp/opencode/secblitz-cross-env.sh`:

- `rustfmt --edition 2021 --check src/engine.rs`: passed.
- Latest `cargo test`: **59 library tests passed** (including 33 engine tests);
  **13 CLI tests passed, 2 localization coverage tests failed** on concurrently
  added platform permission-gate diagnostics. The remaining untranslated strings
  were outside engine ownership; no engine diagnostic was listed in the final
  failure. Localization integration remains with its owner.
- Windows GNU `cargo test --no-run` and application `cargo build`: compiled and
  linked successfully during this integration. No Windows test executable or
  actual security-preference mutation was run by this agent.
- `cargo clippy --lib --tests -- -D warnings`: blocked by six
  `manual_is_multiple_of` diagnostics in `permissions/state.rs` and
  `permissions/descriptor.rs`; no engine diagnostic was reported. Those files
  remain with the permissions owner.

Residual limits: the engine/native last-read gates are not an OS atomic CAS;
external writers can race them. Native descriptor normalization/writeback and
management/service identity gates need the permissions owner's Windows validation.
Torn WALs still fail closed rather than being repaired, and restoring an original
dangerous grant intentionally lowers that service's protection. These are bounded
fixed-object repairs, not a general repair of all local privilege-escalation paths.
