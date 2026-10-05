# Update protocol security review: 0.4.3 hardening and historical 0.4.2 audit

## Independent follow-up verdict: PASS for source and Linux validation

Reviewed the implemented 0.4.3 changes independently after the initial review.
**No protocol blocker confirmed. Previously observed release rollback and the
unlink-first replacement weakness are resolved in the reviewed source.** Native
execution and publication approval remain with the coordinating/native agent.
This follow-up changed only this report, not source, tests, dependencies, version,
installers or the signed payload wire format.

Specific checks:

* `parse_floor` enforces the 4,096-byte bound before parsing, required typed
  fields, duplicate/unknown-field rejection, canonical version, exact target,
  lowercase installer SHA-256 and ordered, bounded timestamps. The existing
  tests exercise every field's duplication/removal, truncation at every byte,
  exact limit and limit+1. Short-circuit ordering protects timestamp subtraction.
* `advance_floor` first enforces the installed-build minimum, then the recorded
  highest version. Same-version hash/target substitution and either timestamp
  decreasing fail. Identical metadata is idempotent; unchanged-content renewal
  with nondecreasing publication and expiry succeeds even after the old record
  expires. The timestamps are monotonic within a release version, not a global
  timestamp ordering imposed across different versions.
* The stored hash identifies the signed release installer. It is not an
  installed-executable hash, Authenticode signature or filesystem timestamp.
  The separate worker/current-image digest comparison binds the worker's
  compiled version to the currently installed image.
* `read_floor` treats only missing state as initialization. It uses no-follow
  existence inspection, then trusted file inspection and a limit+1 read.
  Corrupt, oversized, inaccessible or unsafe existing state propagates an error;
  it does not become `None`. A missing floor still retains the installed-version
  baseline without inventing the installed release's installer hash.
* Both native acceptance paths call `remember_release` after signed-manifest
  verification while holding `update.lock`. Check persists before equality or
  payload download. Worker reloads/enforces the current floor before equality,
  payload verification and launch. Consequently staging an older release does
  not bypass a floor advanced between check and worker. Write failure stops
  the path, and failed download cannot erase an already recorded higher release.
* Atomic replacement creates a protected, exclusive same-directory temporary
  file, writes and flushes it, validates any old destination, then calls
  `MoveFileExW` with replacement/write-through flags. There is no preliminary
  deletion or cross-volume fallback. Existing payload pins deny replacement;
  failure cleanup targets only this invocation's temporary file. Native tests
  for write failure, interrupted temps, pinning and corrupt floor were inspected,
  not executed by this reviewer. Hardware power-loss guarantees are not inferred.
* The loopback fixture explicitly calls `socket.set_nonblocking(false)` after
  accept and before timeout-bounded reads/writes. This correctly removes the
  inherited Winsock nonblocking mode without changing production transport.
  HTTPS-only, no redirects, no proxy inheritance and signed body checks remain.

Fresh validation used `source target/build-tools/cross-env.sh`, which selects
the persistent `target/windows-release` and `target/compiler-tmp` directories:

```sh
cargo test --locked --offline --lib updater::
cargo clippy --locked --offline --all-targets -- -D warnings
cargo clippy --locked --offline --all-targets --target x86_64-pc-windows-gnu -- -D warnings
```

Results: **24 passed, 0 failed, 1 opt-in live test ignored**, 8.45 seconds.
Both host and Windows GNU all-targets Clippy passed. Windows cross-target
analysis is not a native runtime test. The lockfile resolves indicatif 0.18.6.
The coordinator's current clean advisory audit and public-route restrictions
were not independently re-run as part of this protocol-only follow-up.

The public release remains 0.4.2 according to the coordinator; 0.4.3 native
validation/publication is pending. No new live fetch was made. A 0.4.3 candidate
rejecting a 0.4.2 feed is the expected installed-version guard, not a bypass or
a reason to weaken it. Corrected-code protection does not retroactively add
floor enforcement to the published 0.4.2 worker.

**Residual boundaries:** the floor records successfully persisted observations,
not releases never observed or history predating its creation. Hosting control
can still withhold delivery or replay eligible metadata not superseded locally.
Local privileged state deletion, trusted time, full TUF root/key rotation,
thresholds, release-signing operations and first-download provenance remain
outside this fix. The highest-seen protection resolves the specific prior
rollback weakness; it does not make the whole supply chain tamper-proof.

## 0.4.3 security changes

Two confirmed design weaknesses from the 0.4.2 audit are addressed in
`src/updater.rs`, `src/updater/windows.rs`, and `src/updater/tests.rs`:
previously observed signed-release rollback and non-atomic updater file writes.
The Ed25519 strict verifier, public key, `https://secblitz.lol` origin, signed
manifest wire format, installer arguments, directory ACLs and engine lock
location are unchanged. Version/publication changes are owned elsewhere.

### Protected highest-seen release state

`<native ProgramData>\Secblitz\Updates\release-floor.json` is a protected local
record with a 4,096-byte limit (read at most 4,097), unknown/duplicate-field
rejection, and this schema:

```json
{
  "schema": 1,
  "version": "0.4.3",
  "sha256": "<64 lowercase hexadecimal characters of the signed installer hash>",
  "target": "windows-x86_64",
  "published_at": 1000,
  "expires_at": 2000
}
```

The example hash is a placeholder, not an accepted hash. Versions must be
canonical stable semver. The target and schema are fixed. Timestamps must be
unsigned integers with a positive validity interval no longer than 90 days.
Stored records are **not checked for present-day freshness**: expiring an old
manifest must not erase replay protection or prevent a later signed renewal.

After signature/schema/freshness verification, both check and worker paths:

1. Read and validate the floor through a trusted, non-reparse, single-link handle.
   Missing state initializes from the installed-version bound; malformed state
   fails closed and is never silently reset.
2. Reject a signed version below either the current installed/worker build or
   the persisted highest-seen version. A manually installed newer build raises
   the effective minimum without fabricating an installer hash for that build.
3. At the same persisted version, require the same installer hash and target.
   Require nondecreasing publication and expiry timestamps. A newer signed
   manifest renewing the same installer is accepted; an older renewal or a
   same-version binary substitution is rejected.
4. Atomically persist an advancing signed floor while holding `update.lock`,
   **before payload download or installer launch**, even for a fresh `UpToDate`
   response. If persistence fails, no download/install follows that decision.
   Repeated identical metadata requires no rewrite.

Thus seeing N+2 and then failing its payload does not make N+1 eligible after a
restart or clock rollback while the installed build is still N. The worker also
consults current persisted state, so staging is not an exemption. Corrupt floor
content blocks updating, but remains isolated under `Updates`; the journal
loader does not parse it. New local state does not alter old-client wire
compatibility or the five legacy base-directory filename exceptions.

### Atomic metadata, status and payload replacement

`replace()` now exclusively creates a UUID-named protected temporary file in the
same pinned updater directory. It writes and calls `sync_all()` before inspecting
any existing destination's owner/DACL, reparse status, type and hardlink count.
It then switches the name with native `MoveFileExW(REPLACE_EXISTING |
WRITE_THROUGH)`, without a cross-volume copy fallback or unlink-first window.
The replacement retains the temporary file's explicit SYSTEM/Admin protected
DACL. The old name is not touched on partial-write or pre-switch validation
failure; pinned payloads still prevent replacement through their sharing modes.

RAII closes and removes only the exact temporary path successfully created by
that invocation. A process crash may strand a partial protected temporary file;
it is never read as status, manifest, installer, or floor. Subsequent calls do
not delete another invocation's temporary file. No claim is made that this is
a transactional installer rollback or a guarantee against every storage-device
power-loss failure; the name switch removes the prior application-level
missing/truncated-destination window.

### Verification for this change

* **24 offline protocol tests passed**, including the existing strict signature,
  malformed metadata and bounded stream cases. The opt-in live release audit was
  excluded. The existing framing test uses loopback only.
* New/updated tests cover observe-N+2/fail-payload/restart/replay-N+1 rejection,
  clock rollback, same-version hash substitution, unchanged-content renewal after
  the old record expires, timestamp rollback, valid higher releases, installed
  builds newer than stored state, and strict/oversized/truncated local records.
* An engine integration test with a read-only test backend opens, audits and
  lists history while leaving corrupt `Updates/release-floor.json` intact. This
  verifies journal isolation, not an attended guided UI session.
* The Windows library test executable **cross-compiled and linked successfully**.
  Existing native pinned-payload replacement tests exercise the atomic helper.
  Added elevated tests inject a partial disk-write failure, retain an interrupted
  temp while reading valid old status, verify exact temp cleanup, reload a native
  persisted floor, and fail advancement when the floor is pinned or corrupt.
* Native tests have **not** been executed for this change. The native agent must
  run the updater suite serially, then actual 0.4.2 → 0.4.3 migration and a
  subsequent check establishing the new floor. The immutable 0.4.2 worker cannot
  enforce a floor it does not implement; protection starts in corrected code.

The previous `/tmp/opencode/secblitz-cross-env.sh` and toolchains had been removed.
Isolated Rust 1.93.0/rustfmt/Windows std and extracted MinGW tools were restored
under `target/tools/`; no system package installation or guest operation occurred.
Host protocol tests used system Rust 1.93.1, cached Cargo dependencies and
`--locked --offline`. Windows builds used the restored Rust 1.93.0 and local
MinGW binaries. Both used `target/windows-release` and
`target/windows-release/compiler-tmp`. No live feed requests, signing keys,
hosting credentials, publication, or installer execution were used in this fix.

### Remaining trust and operational boundaries

This is **not full TUF**: threshold signing, root/key rotation, trusted time,
independent freshness checkpoints, and first-install Authenticode provenance
remain separate operational/design gaps. Hosting control can still suppress
delivery indefinitely or replay an eligible release never superseded locally.
A privileged administrator can delete or modify local state; that is outside
the standard-user boundary. A signer-authorized excessively high version raises
the floor intentionally; recovery requires an authenticated forward release,
not accepting an older version or automatically resetting state. The floor and
signature do not prove that a signer-authorized installer is benign.

## Historical 0.4.2 audit

The following records the original 0.4.2 findings and test evidence. Its
stateless-replay and unlink/create descriptions are superseded by the changes
above; it is not evidence of current native 0.4.3 execution.

## Conclusion

**No exploitable protocol-validation or unsigned-installer acceptance bypass was
confirmed.** The live 0.4.2 feed and installer passed the actual Rust verifier
against the repository's pinned public key on **2026-10-03 at 05:14:24 UTC**.
Local bit flips failed authentication/integrity checks.

This is bounded audit evidence, **not a claim that the entire delivery chain is
tamper-proof**. Hosting compromise can suppress updates or replay still-valid
signed releases. The initial browser download remains Authenticode-unsigned and
does not benefit automatically from an already trusted installed updater.

The initial audit made no production change; its two confirmed design weaknesses
were subsequently prioritized for the security changes above.
`src/updater.rs` was then left unchanged. Expanded `src/updater/tests.rs` from 14
groups to **21 offline groups plus one explicitly invoked live audit**. The only
other authored file is this report.

## Scope and method

Reviewed the complete core, its tests, and `src/updater/windows.rs` read-only,
including both production calls to `installer`, worker/current-image binding,
signature revalidation and pinned installer execution. Read release context in
`scripts/sign-release.py`, `scripts/build-release.ps1`,
`scripts/prepare-pages.py`, `installer/setup.iss`, `build.rs`, the Windows CI
workflow, Cargo dependencies, public trust assets and the update contract.
Inspected the resolved ed25519-dalek 2.2.0 strict verifier/scalar checks.
`cargo tree -e features -i ed25519-dalek` confirms that the
`legacy_compatibility` feature is not enabled.

Tests call the real private core functions from the existing unit-test module;
they do not implement a competing verifier. Synthetic signing uses only the
existing deterministic `[42; 32]` fixture seed. No production private key,
hosting credential or credential configuration was read. Live activity was
exactly two ordinary HTTPS GETs to the fixed public feed/download routes, with
no redirects, retries, publication, remote mutation or installer execution.
Downloaded bytes and their mutated copies remained in memory.

Native namespace, ACL, scheduler and execution behavior was inspected as
context, not modified or dynamically certified here. No VM or native Windows
test was run. Cross-target Clippy compiled/checks the Windows code and tests;
it is not evidence of Windows runtime behavior. See the separate
[native live-upgrade evidence](windows-v042-results.md).

## Live public release verification

| Observation | Result |
| --- | --- |
| Source origin asset | `https://secblitz.lol` |
| Feed request | `https://secblitz.lol/releases/stable.json`, direct HTTP 200 |
| Public key, raw Ed25519 hex | `1788edf5b6137aa8844154f18a0ee2d7b2080ee9ed7e3fa30434b1d1a421e7c8` |
| Envelope | 450 bytes; SHA-256 `1369eecdbb9c3f7055f9843a0b12e6f8fc882df93d79bbe70119233219907a8b` |
| Signed schema / target / version | `1` / `windows-x86_64` / `0.4.2` |
| Installer request | `https://secblitz.lol/downloads/secblitz-0.4.2-windows-x64-setup.exe`, direct HTTP 200 |
| Signed and downloaded size | **3,813,017 bytes** |
| Signed and downloaded SHA-256 | **`28c93869508923b2ea865267025dc6c0d9e92e5d343e5a78f175d32e5b1b83c4`** |
| Published | `1790999969` - 2026-10-03 03:59:29 UTC |
| Expires | `1798775969` - 2027-01-01 03:59:29 UTC; exactly 90 days |
| Check time | `1791004464` - 2026-10-03 05:14:24 UTC |
| Core version decision | Newer than `0.4.1`; equal to `0.4.2`; equality is not a new install |
| Installer PE inspection | DOS/PE headers present, PE32 optional header `0x10b`, certificate directory zero/absent |
| Local tampering | One payload byte changed without resigning: rejected. One installer byte changed: rejected. |

The setup wrapper being PE32 does not contradict an x64 application target;
Inno's setup source explicitly uses an x86 setup process in 64-bit install mode.
The absent embedded certificate directory supports the current unsigned-setup
status; this was not a Windows Authenticode/catalog-policy evaluation.
No claim of reproducible source-to-binary equivalence or independent inspection
of the installed executable's embedded configuration is made by these GETs.
The public-key asset is the trust anchor for this check, not a key supplied by
the downloaded manifest. Historical documentation referring to `beacons.lol`
describes older installed clients, not the current source origin.

## Attack tests and results

All listed rejection/acceptance assertions passed in the final suite.

| Attack surface | Evidence |
| --- | --- |
| JSON / UTF-8 | Retained malformed envelope and authentically signed invalid UTF-8/JSON tests; duplicate and missing fields across the whole manifest, escaped duplicate names, unknown fields and wrong JSON types fail. Added bounded signed nested arrays and truncation at every byte of a valid fixture envelope. |
| Base64 / signature | Retained invalid alphabet, missing padding, nonzero trailing pad bits, wrong types, missing fields and wrong signature lengths. Wrong public key and corruption of all 64 signature byte positions fail. Raw JSON reformatting needs its own signature. |
| Ed25519 malleability | Added a valid signature with `S + L`, `S = L`, and all-ones scalar. Added order-1/2/4 and noncanonical field encodings (`p`, `p+1`, alternate identity sign) as candidate public keys and signature R points. All attempted forgeries fail. These vectors are targeted checks, not exhaustive cryptographic proof. |
| Numeric bounds | Added textual `u64::MAX + 1`, a 96-bit integer, exponent overflow, negative zero, leading-zero numbers and NaN in each integer field. Added `u32::MAX + 1` schema and `u64::MAX` installer size. They fail. Maximum representable semver components parse without overflow. |
| Time | Existing exact publication/expiry ±600-second boundaries, positive interval, 90-day maximum and `u64::MAX` arithmetic cases pass. Expired metadata fails even if the version equals the current build. |
| Path / version | Added UNC, device path, slash authority, single/double percent-encoded traversal, query, fragment, ADS, slash/backslash, Unicode separator/bidi and semver component overflow, both as filenames and as versions with matching constructed filenames. All fail. |
| Seeded authentication/parser campaign | 1,024 alternating raw-payload/signature single-bit mutations; 1,024 bounded invalid envelope byte strings; 128 of those byte strings separately fixture-signed to reach payload parsing. No acceptance or panic. Seed `0x534543424c49545a`. |
| Seeded installer campaign | 1,024 varying 1–127-byte read fragment sizes over a 1 KiB signed-hash fixture. Each valid stream succeeds; each one-bit mutation, random truncation or extra-byte variant fails. Seed `0x424f554e44454421`. |
| HTTP body framing | Eight bounded loopback exchanges: valid chunked bytes succeed; excess body, truncated chunk/body, missing final chunk, false 1 MiB length and absurd `u64::MAX` Content-Length fail in HTTP parsing or body validation. A valid signed envelope padded beyond 16 KiB and sent chunked is stopped at limit+1 and rejected. No live-host malformed traffic. |
| Streaming bounds / errors | Retained endless-reader test proves only signed-size+1 bytes are delivered to the installer verifier. I/O errors propagate. Exact payload/envelope limits and one-byte excess are covered. |
| Signed content trust | Fixture-signed arbitrary text and truncated MZ bytes with matching size/hash are accepted by the integrity helper. This confirms that it authenticates bytes; it does not certify PE format or installer behavior. Nothing is executed. |
| Replay | Added explicit observe-10 / still-installed-8 / replay-9 sequence: accepted as an upgrade from 8. Installed-10 rejects 9. Clock rollback can revive expired metadata but cannot lower the installed-version floor. |

The first loopback harness run expected an HTTP response for an absurd
Content-Length; reqwest/Hyper instead rejected its headers with `Parse(TooLarge)`.
The test was corrected to count rejection at either the HTTP or integrity layer.
This was a test expectation error, not a product bypass. Final runs are clean.

## Confirmed behavior and design limits

### Hosting compromise versus signing authority

**Cloudflare account/site compromise without the release signing key:** an
attacker can replace site HTML, checksums, feed responses and downloads, or deny
delivery. On an already trusted installation, they cannot authorize different
installer bytes merely by replacing the feed's hash, size or version: those
fields are in the signed raw payload, and the downloaded bytes must match both
size and SHA-256. Replacing a body with HTML/another PE of the same length also
fails its hash. Hosting an unsigned malicious binary therefore does not make
the signed updater execute it.

In 0.4.2, the same attacker could replay **previously signed, still-valid metadata** and its
original installer. If its version is greater than the installed build, that
older release remains an eligible upgrade even after a newer release was seen
but not installed. If equal, it reports `UpToDate`; if lower, it is rejected.
Thus hosting control plus suitable existing signed material permits withholding
a newer fix or offering an older eligible release, not arbitrary new code or
a downgrade below the correctly established installed build. Expiration plus
600 seconds eventually makes that particular replay fail under an honest clock;
delivery suppression can continue indefinitely. That version had no persisted
highest-observed metadata/version floor, threshold signing or online root/key
rotation protocol. This is a documented design limit, not newly discovered
signature forgery; a full TUF redesign is outside this change.

**Signing-key compromise, malicious release author or compromised build/signing
pipeline:** a matching signature on attacker-chosen bytes is sufficient update
authority once other schema/version checks pass. The signed `target` string
does not inspect the executable's architecture. Neither PE parsing nor a later
`--version` check could establish that a legitimately signed installer is benign;
the installer has already run when post-install version confirmation occurs.
There is no independently signed hash for the resulting installed image.

**First download:** the browser/setup bootstrap is still unsigned in Windows
publisher terms. If an attacker controls the website/download origin, replacing
the initial installer and the hash displayed on the same site can deceive a
new user who accepts the elevation warning. The initial malicious executable
could remove the verifier or substitute its own key. Existing pinned-update
verification does not solve that bootstrap boundary. Independent verification
with an already trusted public key would be a separate user action; a checksum
obtained from the same compromised page is not independent provenance.

### Clock and local privilege boundary

Freshness uses the local system clock, not trusted network time. A clock moved
back into an old signed validity window can revive that metadata. It still
must be no earlier than publication minus skew and still must satisfy the
installed-version comparison. Changing machine time normally requires suitable
local privilege; an administrator capable of replacing trusted installed
binaries/state is already outside this updater's unprivileged-adversary
boundary. Do not classify clock rollback alone as a critical remote bypass.
Bad clocks can also cause fail-closed update outages.

The native worker checks its image hash against the current trusted installed
image before comparing the manifest with its compiled package version. This
prevents merely retaining an old worker from supplying a lower current-version
floor. Both the check path and worker verify metadata before testing equality.
Staged metadata is reverified for freshness, and installer size/hash are
rechecked through a file handle retained without write/delete sharing through
process exit. These are source observations, not new native race-test results.

### Actual bounds and transport contract

| Item | Implemented bound / behavior |
| --- | --- |
| Envelope | 16,384 bytes before JSON parsing; native reads at most 16,385 into the parsing buffer |
| Decoded signed payload | 8,192 bytes; base64 decoding allocation is also bounded by the small outer envelope |
| Installer | `1..=67,108,864` bytes (64 MiB); read signed size+1 and require exact signed size/hash before staging |
| Publishing scripts | Stricter 25 MiB Pages installer/file limit |
| Worker image | 64 MiB cap; native worker-vs-installed digest uses a 65,536-byte read buffer |
| Status | 4,096-byte limit; read 4,097 to detect excess; **not a 4 MiB staging limit** |
| Numeric fields | Schema is `u32`; size/timestamps are `u64`; semver components are `u64`. No 96-bit protocol integers. |
| Cryptography | 32-byte public key, 64-byte (512-bit encoded) Ed25519 signature, SHA-256 digest of 32 bytes / 64 hex characters. No 512 MiB integrity allowance. |
| Freshness | Publication at most 600 s ahead; expiration checked with 600 s grace; positive validity at most 7,776,000 s |
| Network | HTTPS-only, redirects forbidden, inherited proxies disabled; connect timeout 15 s; 120 s shared manifest/installer download budget, remaining time applied to second request |

`installer` assumes a manifest returned by `verify`; both inspected production
callers satisfy that prerequisite. Consequently `m.size + 1` cannot overflow:
the authenticated and validated size is at most 64 MiB. Constructing a private
unchecked `Manifest` with `u64::MAX` in a test would violate that precondition,
not demonstrate a remotely reachable bug.

Timestamp subtraction follows `expires_at > published_at` under short-circuit
evaluation, and skew additions saturate. Overflow-edge valid timestamps near
`u64::MAX` can pass only with a similarly extreme supplied clock, not today's
clock. They do not authorize oversized installer reads.

The limits measure bytes presented to parsing/integrity checks, independent of
Content-Length and chunk boundaries. They are not exact socket-byte or total
process-RSS limits: HTTP buffering, Vec capacity and worker-image buffering add
overhead. The declared reqwest configuration disables default features and
does not enable gzip/brotli/deflate/zstd decoding. Installer execution has no
120-second deadline; the native worker deliberately retains locks while waiting
for the trusted installer. The shared network deadline is not an end-to-end
installation deadline.

In 0.4.2, staging/status replacement was unlink-create-write-sync rather than atomic
replacement. Interruption can leave missing/partial files and cause failure;
the subsequent signature/size/hash checks do not treat that as permission to
execute unverified bytes. Durability and native recovery remain separate work.

## Validation commands

Rust toolchain: `rustc 1.93.0 (254b59607 2026-01-19)`. All Cargo runs used the
requested persistent target and compiler-temp locations:

```sh
source /tmp/opencode/secblitz-cross-env.sh
export CARGO_TARGET_DIR=/home/slay/projects/cybersec/windows-hardening-tool/target/windows-release
export TMPDIR=/home/slay/projects/cybersec/windows-hardening-tool/target/compiler-tmp

rustfmt --edition 2021 --check src/updater/tests.rs
cargo test --locked --offline --lib updater::
cargo test --locked --offline --lib updater::tests::live_public_042_feed_and_installer_match_pinned_trust -- --exact --ignored --nocapture
cargo clippy --locked --offline --all-targets -- -D warnings
cargo clippy --locked --offline --all-targets --target x86_64-pc-windows-gnu -- -D warnings
```

- Final offline subset: **21 passed, 0 failed, 1 deliberately ignored live test**;
  8.29 seconds. Includes the bounded loopback framing test.
- Explicit live test: **1 passed**, 1.16 seconds; real network is used despite
  Cargo's `--offline` dependency-resolution flag. This test is opt-in, tied to
  expected release 0.4.2 and intentionally fails once the published target or
  freshness no longer matches this audit.
- Host and Windows GNU all-targets Clippy: **passed with `-D warnings`**.
- Only the owned test file was formatted; no production source, signing script,
  Windows integration, public trust asset, release artifact or dependency file
  was intentionally edited.
