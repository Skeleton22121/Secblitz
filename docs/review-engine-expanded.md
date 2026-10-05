# Independent expanded engine review - agent 3 of 4

Date: 2026-10-02. Owned changes: `src/engine.rs` and this report only.
Reviewed the implementation directly against `control-handoff.md` and
`permissions-design.md`, including the public permissions helpers and native
transition/readback boundary. No permissions-module or guest edits were made.

## Findings and fixes

### Completion relied entirely on backend acknowledgment

Apply appended `Applied` and restore appended `Restored` immediately after
`Backend::write` returned success. The native permissions backend already checks
its own readback, but the engine did not independently verify the exact state
before making the completion durable. A successful acknowledgment followed by a
different safe descriptor could therefore be recorded as completed.

Both paths now observe and validate the state after the write, before recording
completion. Apply compares against the target derived from the original prepared
snapshot; restore compares against the recorded original. Neither computes a new
target from readback. A mismatch or observation failure retains `Pending` or
`Restoring` recovery intent. The check applies to static controls as well.
Readback equality is a state check, not a new authorization exception: mutation
still requires both pre-write eligibility checks and the native write gate.

The new regression injects a different, valid, repair-safe ACL on final readback
for both service IDs, verifies failure and durable recovery state, reopens the
engine, proves conflict/no additional write, and then verifies exact recovery.

### Complex-ACL fixtures changed the wrong hex position

Two existing fixtures used a seven-character offset for the eight-character
`dacl-v1:` prefix. Their edits did not reliably create the documented DENY ACE.
They now calculate the prefix length and modify the actual first ACE type.
The malformed-original test explicitly verifies structural acceptance followed
by repair-target rejection, so incidental parsing failure cannot satisfy it.

## Contract review

- **Catalog:** the compiled allowlist covers twelve legacy controls, four new
  binary registry controls, and exactly BITS/wuauserv permissions controls.
  The permissions catalog accepts only `service-dacl-repair-v1`; observations,
  journal originals and writes must use validated descriptor snapshots.
- **Deterministic recovery:** sealed repeat apply, restart recovery and revert
  derive expected service state from the recorded original. Incomplete apply
  requires explicit revert. No path refreshes an original backup from current
  state. Revert writes only from the exact expected target; an exact original
  closes recovery without mutation even when eligibility is false.
- **Fingerprint:** owner, primary group, supported descriptor flags and complete
  DACL bytes participate in equality. Added independently encoded fixtures prove
  trusted-owner, group and protection changes are valid repair-safe snapshots
  yet still conflict on repeat apply and rollback. The existing safe-right drift
  regression covers safe-to-safe ACL changes.
- **Native boundary:** native writes independently compare owner/group/flags,
  validate the forward or inverse repair relationship, repeat policy and identity
  checks, reread before mutation and verify exact readback afterward. The engine
  passes only validated values and fixed IDs; it never executes descriptor text.
- **Untrusted originals:** loading validates every transaction before any replay.
  Each permission original must parse and have a successful nontrivial repair
  target. Added oversized descriptor, command-like text and SDDL rejection cases
  alongside existing sentinel, malformed, unsupported and already-safe originals.
- **Registry direction:** installer and WDigest repair `1 → 0`; both LSA controls
  repair `0 → 1`. Absence and safe values are preserved. Exact absence remains a
  supported recorded original for restore. Existing tests exercise all four
  directions, idempotence, management rejection and original retention.
- **Restore exceptions:** `eligible: false` permits mutation only for the exact
  UAC preservation reason on the two UAC controls or the exact machine-preference
  preservation reason on the four binary controls, and only at that control's
  target. These reasons mean the platform gate succeeded but apply preserves the
  safe setting; they do not mean a failed authority gate is bypassed. New tests
  reject cross-control reasons, whitespace variants, management errors, absence
  and all permission-control reason exceptions.
- **Compatibility:** schema remains 1. The existing literal twelve-control
  schema-1 fixture restores through the extended eighteen-control catalog.
- **Bounds:** WAL metadata is capped at 1 MiB before reading; a limited read also
  detects growth. Each line is capped at 128 KiB before JSON deserialization.
  Descriptor parsing imposes the separate 16 KiB decoded bound. Append checks
  line and total size before writing. New tests prove oversized valid-JSON lines
  and oversized WALs fail before observations or writes and remain untouched.
  The existing large real-descriptor fixture proves roundtrip beyond 4 KiB.

## Verification

Using the requested host cross-tool environment:

```sh
source /tmp/opencode/secblitz-cross-env.sh
rustfmt --edition 2021 src/engine.rs
cargo test --lib
cargo check --target x86_64-pc-windows-gnu --all-targets
```

- Library tests: **67 passed, 0 failed** in the shared working tree at verification.
  This count includes other reviewers' concurrent module tests.
- Windows GNU all-target compile check: **passed**.
- Four new engine regression tests; existing ACL fixture and malicious-input
  coverage strengthened.

These are portable tests and cross-compilation, not native Windows runtime
mutation tests. Native check/write races remain non-atomic; an additional exact
readback narrows what can be reported as success but is not compare-and-swap.
