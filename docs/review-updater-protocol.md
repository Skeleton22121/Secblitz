# Independent updater protocol review - reviewer 1 of 4

Reviewed 2026-10-03 for the 0.4.0 hourly SYSTEM update path.

## Scope and conclusion

Owned files: `src/updater.rs`, `src/updater/tests.rs`, and this report.
Windows integration was inspected read-only; its implementation, installed-build
binding, protected staging and persistence belong to the Windows reviewer.

No exploitable signature-verification or protocol-validation bypass was found
in the reviewed core. No production-core change was justified. Expanded the
actual core's tests from 3 to 14; these are not a reimplementation of the verifier.
This conclusion is not an end-to-end approval of SYSTEM installer execution.

## Findings and security boundaries

### Replay: installed-version floor, not highest-seen-release persistence

`verify` is deliberately stateless. A previously signed manifest remains usable
through its expiration plus 600 seconds. `newer` rejects versions below the
supplied current version and returns false for equality. It compares semantic
versions numerically, not lexically.

There is no persisted highest-seen release in this core. A feed-controlling
attacker with previously signed material can serve an older, still-valid release
that is nevertheless newer than the installed build, including after a newer
release was observed but not installed. They cannot use that mechanism alone to
install below the correctly established installed-version floor. Suppressing
updates remains possible for an attacker able to block delivery.

The Windows worker's byte comparison against the trusted installed image is an
essential prerequisite: its compiled version must not represent a stale worker
while the machine actually has a newer build. Protection against privileged
manual restoration of old binaries/state, or persistence of the highest-ever
observed release, is not provided by `newer`. Any stronger high-water policy
needs protected Windows persistence and an explicit recovery policy; it cannot
be implemented correctly solely in this stateless verifier.

### Expiration is fail-closed, including an equal-version feed

Both inspected Windows paths call `verify` before `newer`. An expired feed for
the currently installed version therefore returns an error, not `UpToDate`.
This is covered by a dedicated regression test. Serving the current release
indefinitely requires renewing its signed manifest. A signed timestamp does not
provide a trusted clock: an administrator-controlled clock can affect freshness.

The update protocol supplies no intentional downgrade operation, transactional
installer rollback, or hardening-settings restoration. An expired feed does not
authorize any of those actions. Restoring settings is a separate engine/CLI
operation, not an updater recovery behavior.

### Authentication and strict parsing

- The envelope is capped at 16 KiB before JSON parsing or base64 allocation;
  decoded payloads are capped at 8 KiB. Decoding the signature after the payload
  is bounded by that same small envelope cap and is not an unbounded allocation.
- `verify_strict` authenticates the exact decoded payload bytes before parsing
  their contents. Reserializing equivalent JSON does not preserve authentication.
- Unknown and duplicate envelope/payload fields are rejected, including escaped
  duplicate member names. Required fields cannot be omitted.
- Schema and target are exact; stable canonical versions exclude prereleases and
  build metadata. The filename must exactly match the version-specific installer
  basename. Hashes require 64 lowercase hex characters.
- Size is an integer in `1..=64 MiB`. Timestamps are unsigned integers, validity
  is positive and at most 90 days, and both clock-skew boundaries are inclusive.
  The ordering check protects timestamp subtraction; saturating additions avoid
  overflow. Installer verification requires both exact size and SHA-256.

The pinned Ed25519 key authenticates release authorship and installer bytes.
It does **not** provide Windows Authenticode publisher identification, Microsoft
approval, or SmartScreen reputation. A publisher warning can still appear.

### Network and memory integration

The checked-in origin is `https://beacons.lol`. The inspected Windows client
explicitly uses `Policy::none()`; reqwest's default redirect behavior must not
be relied on. The current dependency enables blocking/rustls and disables
default features; no automatic compression decoder is enabled by that declaration.

Both network and staged-file manifest reads use `take(MANIFEST_LIMIT + 1)`.
`verify` then measures the actual bytes, independently of Content-Length. The
extra byte distinguishes an exact-limit body from an oversized one. If response
decompression is enabled in a future dependency configuration, the cap must
remain on the bytes delivered to parsing, not just a compressed length header;
decoder resource behavior would also need review.

Installer reads similarly stop at signed size plus one. The test uses an endless
reader to verify that an oversized stream is not consumed indefinitely. All
production callers inspected obtain their `Manifest` through `verify` before
calling `installer`, which is necessary for its size bound.

### Error/status handoff

Validation and I/O failures propagate as `Err`. The presence of the serializable
`UpdateOutcome::Failed` variant does not turn those errors into successful API
results. Windows attempts to persist a generic failure status; early failures
can occur before persistence is possible. CLI integration must handle `Err` and
must not infer success from a previous status file. CLI ownership is separate.

## Added attack and boundary coverage

- Independent corruption of each of the 64 signature bytes with the authentic
  payload unchanged; malformed signature lengths; small-order identity-key
  forgery rejected by strict verification.
- Equivalent JSON with changed raw bytes requires a new signature.
- Invalid/noncanonical base64, wrong envelope types, missing/unknown/duplicate
  fields, signed invalid UTF-8 and malformed JSON.
- Every manifest field duplicated or missing, including duplicate target names
  expressed using JSON escapes.
- Wrong schema/target, traversal/absolute/alternate-stream/NUL/mismatched
  filenames, invalid hash length/alphabet/case, and noninteger numeric fields.
- Exact 8 KiB payload and 16 KiB raw-envelope limits, with valid signed JSON and
  whitespace padding; one byte beyond each limit is rejected.
- Exact publication/expiration skew and 90-day boundaries, future publication,
  invalid intervals, and overflow-edge timestamp arithmetic.
- Canonical version rejection, numeric downgrade/equality behavior, expired
  equal-version feeds, bounded installer reads and propagated read errors.

Existing tests retain wrong-key, zero/oversized installer size, truncated/extra
installer bytes, same-length hash corruption and invalid origin coverage. Test
signatures use deterministic synthetic keys, never the release private key.

## Validation

After sourcing `/tmp/opencode/secblitz-cross-env.sh`:

```sh
cargo test --locked --offline --lib updater::
cargo check --locked --offline --tests --target x86_64-pc-windows-gnu
```

Results: **14 updater tests passed** against the repository's actual `src/lib.rs`;
Windows GNU check passed, including test compilation. The initial locked run
was blocked by the then-stale repository lockfile. After that lockfile was
updated externally, the repository commands above both passed. This reviewer
did not edit the lockfile. An isolated source-linked harness also passed during
that interval. `src/updater/tests.rs` was formatted with rustfmt.

No guest or native Windows tests were run. No private keys or cloud credentials
were read, and no live feed was fetched. Feed freshness conclusions above are
code/test results, not a claim about the currently deployed feed's contents.
