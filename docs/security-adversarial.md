# 0.4.3 security regression acceptance

**Security deployment gate: LIVE E2E PASS.** The genuine published 0.4.2
installation upgraded through its SYSTEM task to published 0.4.3 in 15.821 s.
The exact new image, UpToDate, protected release floor, valid current/busy status
JSON, native 18-result/19-finding audit, real-guide DeferredBusy and normal Esc
exit were verified. The floor remained byte-identical across the busy check;
all controls and original WAL hashes were restored unchanged. Full evidence and
scope are in windows-v043-results.md.

This supplements [the native adversarial review](security-review-native.md).
It records targeted acceptance of the new release-floor and atomic persistence
changes, not a repeat of the entire standard-user boundary review.

## Verified

- All nine elevated updater tests passed as SYSTEM, session 0, on the isolated
  UI clone.
- Failed/partial status writes preserve the prior JSON; unrelated stranded
  temporary files are neither adopted as status nor deleted.
- The protected release floor persists an observed higher version across a
  failed payload, rejects rollback, preserves its old value on write failure,
  and fails closed on malformed state instead of resetting it.
- Host protocol tests covering floor persistence, clock rollback, immutable
  release metadata and corrupt-floor compatibility with the engine passed.
- The generated lockfile now uses indicatif 0.18.6 and no longer contains
  `number_prefix`.
- The basic installer lifecycle passed with original control/WAL baselines
  restored. That phase used no production feed. The subsequent live gate used
  only the public signed feed; no signing secret was involved in either phase.

## Native and live release gates resolved

The protocol owner fixed the native loopback HTTP helper by making the accepted
socket blocking before its timed reads. The focused case and full native suites
now pass: **116 library tests and 52 CLI tests**, with the live/attended probes
still deliberately ignored. Release bytes are unchanged, so the prior nine
SYSTEM cases and full installer acceptance remain applicable. Exact tested
0.4.3 artifacts were promoted, with 0.4.2 preserved in the archive.

See 0.4.3 Windows results for hashes, restored-state
proof and evidence paths. The later genuine 0.4.2 to 0.4.3 live test passed.
Live floor checks were read-only; rollback and interruption fault injection
were not repeated against the published feed.

The persistent floor raises the minimum authenticated release already observed
by an installation. These tests do not claim protection from a signing-key
compromise, erasure of protected state by an administrator, or every initial
replay before any higher release has been observed. The atomic-write tests use
controlled failure injection rather than destructive disk/power testing.
