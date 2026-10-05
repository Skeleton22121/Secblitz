# Independent release-infrastructure review 4 of 4

Reviewed 2026-10-03 against release **0.4.2**. Scope: signing, static deployment
contents, and read-only crossover inspection of the native updater and installer.
No production credentials or signing key were accessed. No Cloudflare changes,
publication, native-source changes, or dependency updates were performed.

## Result

**Confirmed public source exposure, fixed in the local deployment path; publication
is still required.** The previous direct `website/` upload included `README.md`
and `test_video.py`. Both production URLs returned HTTP 200 and actual source
bytes, not an SPA fallback, at approximately 05:12 UTC on 2026-10-03:

| URL on `https://secblitz.lol` | Bytes | SHA-256 of public response |
| --- | ---: | --- |
| `/README.md` | 10,353 | `4db57814276fdc1e67fbf69cfdeb7132f79b96deabb0676932c990343d5c00b4` |
| `/test_video.py` | 16,459 | `1ca99257fc35c6a301d90959dbaf4a25ee7e4089fba0d6ecbb6cf713fe736359` |

Responses exactly matched local source. HEAD and bounded-in-scope GET checks
were limited to those two known non-secret source paths on the owned site.
No sensitive filename probing was performed. An initial Python HTTP request was
denied with 403; curl subsequently returned the confirmed source responses.
This is unnecessary source/internal-documentation disclosure, not evidence of a
credential leak or a signature bypass. Source README/tests remain intact.

The inspected updater authenticates all release-critical fields and installer
bytes using its compiled Ed25519 pin. Hosting-only control does not supply the
signing authority required to replace an installed client's update with arbitrary
code. This does not authenticate the initial browser download, guarantee hosting
availability, or establish protection after signing-key/build-host compromise.

## Changes made

### Deterministic publication staging

New `scripts/stage-pages.py` generates `dist/pages` from an explicit allowlist:

- `index.html`, `styles.css`, `app.js`, `_headers`, `releases/stable.json`;
- the named SVG, WebP poster, MP4 and Schibsted Grotesk WOFF2 font;
- the current version's installer and portable executable;
- the explicitly enumerated, already published 0.3.0, 0.4.0 and 0.4.1 download URLs;
- `assets/fonts/OFL.txt` and/or `assets/fonts/LICENSE.txt` when supplied by the
  website owner.

There are no recursive asset/download globs. Extra source files, including hidden
files, README and tests, cannot expand the published set. At most 32 files are
accepted; each is bounded by Pages' 25 MiB limit and the feed by 16 KiB. Future
release owners must add the outgoing current version to the reviewed historical
list when retaining its immutable URLs. Historical artifacts are retained but
are not authenticated by the current stable feed.

Selected input files must be regular, single-link objects, with no symlink in any
existing ancestor. Reads validate the opened file identity and check for changes
during reading. The staging process rejects FIFOs/special files, canonical-path
violations, credential/private-key markers in ASCII or UTF-16-style binary
strings, invalid PE header signatures/architectures, invalid WOFF2 length/magic,
and wrong media signatures. These are format sanity checks, not malware scanning
or full PE/media validation.

Only copied bytes are validated: pinned-key feed verification, current installer
hash/size/version, HTML installer links/displayed hash, local HTML and CSS asset
existence, and both current executables' hashes against final build references in
`dist`. A temporary sibling snapshot is validated before replacing the destination.
A failed validation retains any prior valid snapshot. An existing destination
containing unknown files, hardlinks or symlinks is rejected without deleting user
work. Source files are never deleted or rewritten by staging.

`wrangler.jsonc` now selects **`./dist/pages`** instead of `./website`. Staging
does not emit `_redirects`; domain migration redirects remain the hosting owner's
zone-level configuration. No visible website, video, caption, controls or copy
were changed.

### Existing preparation command compatibility

`scripts/prepare-pages.py` still defaults to `website`, including its existing
origin-configuration and separately downloaded feed-verification modes. It now
also accepts `--site dist/pages`, rejects hardlinked/special output files,
validates canonical local asset paths, and scans UTF-16-style secret markers.
Preparation remains validation, not staging or publication.

### Signing filesystem hygiene

`scripts/sign-release.py` now rejects symlink ancestors, hardlinked and nonregular
inputs, and oversized inputs. The private key must be outside the repository;
on POSIX its opened object must be owned by the effective operator and have exact
mode `0600`. File identity and change-during-read checks apply before signing.
The signer still requires Ed25519 and equality with the expected public pin.

Output uses a same-directory temporary file and atomic replacement rather than
truncating the destination. Existing symlink/hardlink output aliases are rejected;
replacement also avoids writing through an alias to another input. These are
defense-in-depth/operator-error fixes. A malicious workspace executed as the
signing operator already has that operator's authority; no new remote or
standard-user-to-administrator exploit is claimed from the old local alias cases.
Privileged administrators and a compromised signing account remain trusted/outside
the stated local attacker boundary. POSIX mode checks are not a Windows ACL audit.

## Signing and updater crossover findings

- The exact UTF-8 payload bytes are signed. The manifest carries schema, canonical
  stable version, fixed `windows-x86_64` target, exact version-derived installer
  basename, lowercase 64-character SHA-256, byte size, publication and expiration.
  Envelope/payload unknown fields and duplicate JSON fields are rejected by the
  reviewed gates. Native verification uses `verify_strict` with the compiled pin.
- Native transport uses the compiled HTTPS origin, disables proxies and redirects,
  bounds feed/installer reads and timeouts, rejects downgrade versions, and checks
  payload length/hash before persisting installer bytes. There is no feed-supplied
  executable path, command line, origin or replacement public key.
- Native staging/installation inspects ownership/DACLs, rejects reparse points and
  hardlinks, binds the worker image to the currently installed image, rechecks the
  signed manifest and installer, and retains a non-write/non-delete-share payload
  handle through installer exit. The installer uses a fixed Program Files location,
  elevated setup, embedded maintenance code and a cleared/allowlisted environment.
  These sources were inspected read-only; this review did not rerun Windows VM tests.
- A build-output change after signer hashing does not silently authorize different
  installer bytes: staging checks its snapshot against the signed hash and final
  build reference, and the installed updater independently verifies downloaded and
  staged bytes. Staging cannot protect against the trusted operator mutating output
  after validation, a malicious compiler, or malicious code deliberately signed by
  the release authority.
- Hosting compromise can withhold updates, serve expired/invalid responses, or
  replay an unexpired signed release where version rules permit it. It cannot
  invent a valid signed release. Native freshness has a ten-minute clock allowance;
  local publication validation is stricter. System clock integrity is assumed.

### Renewal and rotation

The checked feed was published **2026-10-03 03:59:29 UTC** and expires
**2027-01-01 03:59:29 UTC**. Refresh before expiration using the unchanged final
installer and offline signing key, even if the application version is unchanged.
Run the same gates and publish the refreshed feed on both legacy and primary
origins. Do not make renewal dependent on possession of a hosting API credential
by the signing environment.

Existing clients accept exactly their compiled key. Replacing the website's
public-key file or advertising a new key in JSON does not rotate installed pins.
A new-pin binary needs an old-key-authorized migration release. The current
single-envelope/single-feed design also needs an explicit plan for clients still
on the old pin when the feed switches keys; do not assume simultaneous old/new-pin
compatibility. Compromise of the old signing key requires a separate recovery
decision, not an assurance that normal signed rotation revokes attacker authority.

### Initial download is still unsigned

Both current `dist/secblitz.exe` and the installer have empty PE Authenticode
certificate table entries `(0, 0)`. Manifest signing is not Windows publisher
signing and does not remove SmartScreen/unknown-publisher warnings. A checksum on
the same compromised webpage is not an independent trust anchor. An attacker
controlling hosting can substitute an unsigned initial installer for a new user
even though the genuine already-installed updater would reject it. Preserve the
truthful unsigned-publisher disclosure; any stronger bootstrap-authentication
claim needs a separately trusted distribution/publisher-verification mechanism.

## Verification performed

Passed against actual 0.4.2 files:

```sh
python3 scripts/prepare-pages.py --require-feed --expected-version 0.4.2
python3 scripts/stage-pages.py --expected-version 0.4.2
python3 scripts/prepare-pages.py --site dist/pages --require-feed --expected-version 0.4.2
```

Direct-source validation counted 18 files; staging and staged-output validation
counted **16**. No font license currently exists at either allowed source path.

Both entries in `dist/SHA256SUMS` were independently recomputed and matched:

| Artifact | SHA-256 |
| --- | --- |
| `secblitz.exe` | `78c666e456b3f72fbe503e4aad6213b1baec4f7172e3e8cacd17f9bf893b4672` |
| `secblitz-0.4.2-windows-x64-setup.exe` | `28c93869508923b2ea865267025dc6c0d9e92e5d343e5a78f175d32e5b1b83c4` |

The installer is 3,813,017 bytes. The compiled app contains the expected public
pin and configured origin strings. This string check is corroborating evidence,
not a substitute for native runtime acceptance or a reproducible-build proof.

Credential/private-key marker scans passed for all selected static artifacts and
both final build executables, including UTF-16-style strings. No actual Cloudflare
token/global-key value was accessed, so an exact-secret-value absence claim is not
made. Compressed/encrypted/encoded data or an unlabelled arbitrary token can evade
a marker scan. Main can perform a private exact-value scan if needed, reporting
only pass/fail and never the credential.

**18 adversarial test cases passed** using disposable fixture files and ephemeral
test-only signing keys, covering deterministic output, unlisted-file exclusion,
preserved source/user work, rollback on failure, optional font license inclusion,
malformed/traversal paths, unstaged HTML/CSS assets, leaf/ancestor/output symlinks,
hardlinks/FIFOs, ASCII/UTF-16 secret markers, PE mismatch, changed portable build,
signed-installer tampering, feed-signature tampering, HTML hash mismatch, invalid
WOFF2 length, signer key permissions and output aliases. Harness:
`/tmp/opencode/test-release-review4.py` (temporary review evidence, not deployed).
An initial test attempt duplicating full production binaries hit disk quota;
compact synthetic PE/feed fixtures resolved it. Actual release staging had already
passed independently.

### Dependency audit limitation

`cargo audit --version` reported the subcommand unavailable. No tools were installed
and no dependency versions were changed. Selected lockfile versions inspected:
`ed25519-dalek 2.2.0`, `curve25519-dalek 4.1.3`, `reqwest 0.12.28`,
`rustls 0.23.45`, `rustls-webpki 0.103.15`, `ring 0.17.14`, `hyper 1.11.1`,
`quinn 0.11.12`, `tokio 1.53.1`, `webpki-roots 1.0.9`. This is an inventory,
not a current advisory clearance; no unverified advisory IDs are asserted.
Run a current RustSec audit in the existing CI/tooling environment and triage exact
advisory IDs, affected versions and enabled target/features.

## Main/owner handoff and remaining external work

1. **Update the deployment instructions** in the separately owned
   `website/README.md` and `docs/cloudflare-pages.md`. They still describe a direct
   `website` upload, which explicitly bypasses the new Wrangler default. Dashboard/
   Git build output must also become `dist/pages`; its build command must run
   staging with Python/cryptography available. Keep signing separate/offline.
2. **Supply the actual upstream font license** as `website/assets/fonts/OFL.txt`
   (or the specifically allowed `LICENSE.txt`) and rerun staging. Do not replace
   it with a fabricated license. The font bytes currently pass WOFF2 checks.
3. **Run gates, then publish only the successful snapshot**, with the existing
   authorized deployment environment and actual production branch. Suggested
   operator sequence, not executed by this review:

   ```sh
   python3 scripts/prepare-pages.py --require-feed --expected-version 0.4.2 &&
   python3 scripts/stage-pages.py --expected-version 0.4.2 &&
   python3 scripts/prepare-pages.py --site dist/pages --require-feed --expected-version 0.4.2 &&
   npx wrangler pages deploy dist/pages --project-name secblitz
   ```

   Supply the confirmed production branch when needed. Never deploy a previous
   snapshot after a failed stage command. No Cloudflare key is needed for any
   local validation/signature verification in this review.
4. **Check production after publication**: the two known source URLs must no longer
   return those source bytes. A normal Pages SPA fallback can return index HTML
   with status 200; distinguish that from disclosure. A custom 404 page is not
   required to prevent source exposure. If strict 404 semantics are desired, the
   website owner can add a reviewed 404 asset and extend the allowlist. Previous
   immutable Pages preview/deployment URLs may retain old source snapshots;
   retiring those is a hosting-owner operation and was not checked here.
5. Confirm feed/download direct responses on `secblitz.lol` and `beacons.lol`,
   feed `Cache-Control: no-store`, security headers, hashes/signatures and actual
   0.4.1-to-0.4.2 migration behavior. The review did not republish or repeat main's
   prior live-feed/native acceptance. Keep legacy update paths free of redirects.
6. **Separate credentials and authority.** Pages deployment needs an appropriately
   scoped Account / Cloudflare Pages / Edit credential. Domain setup additionally
   needs DNS read/edit and Single Redirect edit permissions scoped to the two
   owned zones, with only the additional read permissions the actual API workflow
   requires. Routine deploy jobs should not inherit domain-administration rights.
   Confirm exact permission names against the API endpoints in use. A currently
   used Global API Key is an operational rotation/least-privilege issue, not proof
   of a product exploit. Its replacement/rotation belongs to the user/operator.
   Keep tokens ephemeral and out of frontend/build artifacts; expose deploy
   credentials only to the deployment job and never the offline signing job.
7. Retain protected production CI environments/branches, reviewed dependencies and
   pinned deployment-tool/action versions. The trusted build/signing environment,
   native administrator boundary, key backups, renewal schedule, initial-download
   authenticity and hosting availability remain explicit operational assumptions.

No claim of universal compromise prevention or absence of every possible embedded
secret follows from this review.
