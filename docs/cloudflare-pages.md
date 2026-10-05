# Cloudflare Pages preparation

## Current state

The authorized primary domain is now **https://secblitz.lol**. The deployment
operator verified its active zone, added `secblitz.lol` and `www.secblitz.lol` to
the existing Pages project `secblitz` (`secblitz.pages.dev`), replaced the parked
apex A record with a proxied CNAME, and configured the www CNAME. Existing mail
MX/TXT records were left untouched. Confirm TLS/domain activation before calling
the migration live.
`wrangler.jsonc` selects `secblitz` and static output `website`.
`assets/update-origin.txt` is set to `https://secblitz.lol`; the pending release
version is `0.4.2`. The first live deployment was `9eb6cfc7`, serving `0.4.1` on
`beacons.lol`. The 0.4.2 migration is **pending**, not recorded as published. These
preparation scripts do not perform Cloudflare operations or publish anything.
The current site is validated in place; its HTML, CSS and JavaScript are preserved.

### Compatibility during the domain migration

Published 0.4.1 clients have `https://beacons.lol` compiled into their updater.
Keep that domain active on this Pages project and continue serving
`/releases/stable.json` and `/downloads/*` there directly over HTTPS, including
the new 0.4.2 feed/installer. Do **not** redirect those requests: installed clients
reject redirects. Once upgraded, 0.4.2 clients use `https://secblitz.lol`.

Only the old homepage redirects to the new primary domain. The www primary host
redirects all paths to the apex. The website owner's intended `_redirects` is:

```text
https://beacons.lol/ https://secblitz.lol/ 301
https://www.secblitz.lol/* https://secblitz.lol/:splat 301
```

Keep feed refreshes current on both domains. The signing key and feed schema do
not change with this migration; the signer is domain-independent.

## Local checks

Use Python 3.10+ and `cryptography`. If it is missing, install into a virtual
environment outside `website`, for example:

```sh
python3 -m venv /tmp/secblitz-release-venv
/tmp/secblitz-release-venv/bin/python -m pip install cryptography
python3 scripts/prepare-pages.py
```

Use that environment's Python for the scripts if needed. The validator checks
every output file (including installers and MP4s) against the 25 MiB per-file
Pages limit and a conservative 20,000-file limit. It rejects symlinks, hidden
configuration files, private-key file extensions, PEM private-key markers and
Cloudflare credential assignments (including Global API Key headers and environment
variable names). Bare hexadecimal hashes/public keys are not treated as secrets.
This is a targeted secret check, not proof
that arbitrary content contains no secrets. Keep all credentials outside the
static output. An existing feed is signature-, hash-, size- and expiry-checked.
Use `--require-feed --expected-version 0.4.2` as the final release gate. Without
an explicit expected version, `--require-feed` uses the package version in
`Cargo.toml`. The gate verifies that the feed version, every installer href in
the HTML, and the displayed `id="sha"` checksum identify the same actual installer.
Local HTML href/src/poster assets must exist within the output directory.
If a canonical link is present, it must be exactly the compiled origin plus `/`;
duplicate or stale-domain canonical links fail validation. HTML base overrides
are rejected so relative asset checks retain their meaning. If `_redirects` is
present, the gate checks the two explicit migration rules above (301 or 308),
including that no legacy updater paths are redirected. Local robots links, like
other local hrefs, must resolve to an existing file; robots rules do not authorize
updater redirects. These static checks do not prove live DNS/TLS or edge behavior.
This checks release metadata and bytes, not the executable's embedded version;
native release tests remain responsible for verifying the installed version.
No placeholder feed is generated. `video/` and its Remotion/build files are outside
`website` and are not deployed; only the exported site assets are included.

The website owner has added this rule to `website/_headers`:

```text
/releases/*
  Cache-Control: no-store
```

The site's CSP includes `media-src 'self'` for its local video. Both rules were
verified locally; confirm both headers on the production host after deployment.

## Confirm ownership and endpoint identity

The deployment operator has already confirmed ownership and authentication using
ephemeral `CLOUDFLARE_API_KEY` and `CLOUDFLARE_EMAIL` environment variables. Do not
persist them in source, `.env` files, browser assets, reports or command transcripts.
Future authentication can instead use an appropriately scoped API token or
`npx wrangler login`. Inspect identity/project information privately when needed;
do not recreate the existing project or change mail records. Wait for the custom
domain's TLS/DNS readiness before declaring production available.

After confirming ownership, supply that explicit origin:

```sh
python3 scripts/prepare-pages.py --origin https://secblitz.lol
```

This validates URL structure and saves the compile-time asset; it does **not**
verify remote ownership. Never infer ownership from a syntactically valid URL.
Rebuild the application and final installer after configuring the origin.
An empty origin makes the updater return `NotConfigured` without network traffic.

Source builds may instead pass `SECBLITZ_UPDATE_ORIGIN` **at build time**, for example:

```sh
SECBLITZ_UPDATE_ORIGIN=https://secblitz.lol cargo build --locked --release
```

Pass this environment variable to the actual Windows release build process too;
setting it only while signing or deploying cannot change an existing binary.
Retain the release builder's static-CRT `RUSTFLAGS`; do not use `RUSTFLAGS` as a
substitute for endpoint configuration. Core uses `/releases/stable.json` on this
compiled origin.

## Sign the final release

The production Ed25519 private key is stored at
`/home/slay/.config/secblitz/release-signing-key.pem`, with mode `0600`.
Never copy it to the repository, static site, installer, browser or tool output.
Back it up securely outside those locations. The public 32-byte key is pinned in
`assets/update-public-key.hex` and must be compiled into the application.

Finish building and, if used, Authenticode-signing the installer **before**
generating its manifest. Use the actual final package version; the example below
does not authorize relabeling the existing older installer:

```sh
python3 scripts/sign-release.py \
  --installer dist/secblitz-0.4.2-windows-x64-setup.exe \
  --version 0.4.2 \
  --key /home/slay/.config/secblitz/release-signing-key.pem \
  --output website/releases/stable.json \
  --lifetime-days 90
```

Place that exact installer at `website/downloads/` with the same filename using
the release packaging process. Do not modify it after signing. Then run:

```sh
python3 scripts/prepare-pages.py --require-feed --expected-version 0.4.2
```

The signer refuses a key that differs from the pinned public key, a mismatched
filename, a noncanonical stable version, or a lifetime outside 1–90 days.
The optional `--public-key` argument supports isolated tests with temporary keys;
production must use the real compiled public key. Signing checks bytes and
filename, not the executable's embedded version: the release builder must verify
that identity. No existing old installer was signed during preparation.

The feed is `/releases/stable.json`, the installer is
`/downloads/secblitz-X.Y.Z-windows-x64-setup.exe`, and both use the same HTTPS
origin. See [the core contract](update-contract.md) for the signed envelope.
JSON payload bytes are UTF-8, sorted-key, compact JSON; Ed25519 signs those exact
bytes. The envelope contains standard padded base64 payload and signature.
Re-sign still-served releases before their maximum 90-day expiration, even when
there is no newer application version. Schedule a feed refresh before day 90:
use the unchanged final installer and production signing key, rerun validation,
and publish the refreshed feed. Expired feeds are rejected by installed clients.

Manifest signing is **not Windows Authenticode signing** and does not remove
SmartScreen warnings. Public-key rotation requires distributing a binary with
the new pin through a release trusted by the old pin first; replacing the asset
on the website cannot change installed clients. Existing binaries accept only
their compiled pin, not arbitrary old/new keys or keys advertised by the feed.

## Authorized deployment handoff

Publication is authorized and handled by the deployment operator. After the
native tests pass, the final installer is copied into `website/downloads`, the
website owner updates its version/checksum, and the feed is signed, run:

```sh
python3 scripts/prepare-pages.py --require-feed --expected-version 0.4.2
npx wrangler pages deploy website --project-name secblitz
```

Confirm the project's production branch and supply `--branch` with that actual
branch if needed rather than accidentally deploying a preview (do not assume a
branch name). Pages dashboard/Git integration uses build output `website`; its build
command can be `python3 scripts/prepare-pages.py --require-feed` with
`cryptography` installed in the build environment. Sign offline before uploading
the feed; the Pages build needs only the public key.

For a future GitHub Actions deployment, store `CLOUDFLARE_API_TOKEN` and
`CLOUDFLARE_ACCOUNT_ID` in GitHub Actions secrets, pass them only to the deployment
step, pin the Wrangler version in that workflow, and restrict deployment to an
approved production branch/environment. Do not commit tokens, print them, or
write them to website/environment files. No deployment workflow is added here.

## Production verification (deployment operator only)

Run these after the domain becomes active and deployment completes. They do not
use Cloudflare credentials. Download into a fresh temporary directory, never into
the production staging site. No `--location` is used: the updater forbids redirects,
so require **HTTP 200**, not a redirect or an HTML fallback.

```sh
curl --proto '=https' --tlsv1.2 --fail --silent --show-error --head https://secblitz.lol/
curl --proto '=https' --tlsv1.2 --fail --silent --show-error --head https://secblitz.lol/releases/stable.json
curl --proto '=https' --tlsv1.2 --fail --silent --show-error --head https://secblitz.lol/downloads/secblitz-0.4.2-windows-x64-setup.exe
```

Verify the homepage CSP includes `media-src 'self'`, the feed is JSON with
`Cache-Control: no-store`, and the installer is an attachment/octet-stream.
Then verify downloaded bytes against the production pin locally:

```sh
VERIFY_DIR=$(mktemp -d /tmp/secblitz-live-check.XXXXXX)
curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
  https://secblitz.lol/releases/stable.json --output "$VERIFY_DIR/stable.json"
curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
  https://secblitz.lol/downloads/secblitz-0.4.2-windows-x64-setup.exe \
  --output "$VERIFY_DIR/secblitz-0.4.2-windows-x64-setup.exe"
sha256sum "$VERIFY_DIR/secblitz-0.4.2-windows-x64-setup.exe"
python3 scripts/prepare-pages.py --expected-version 0.4.2 \
  --verify-feed "$VERIFY_DIR/stable.json" --installer-directory "$VERIFY_DIR"
```

Compare the printed SHA-256 with the website and staged release. The local feed
verifier checks Ed25519, exact version/filename, freshness, SHA-256 and size without
network access or private-key access. Verify updater behavior and the installed
`Secblitz.exe --version` in a Windows VM, and check browser video/download behavior.
Verify compatibility separately: the following updater URLs must return **200**
directly, while the old homepage and www hostname must redirect with the expected
Location. `curl --fail` alone does not reject 3xx; inspect the status and headers.

```sh
curl --proto '=https' --tlsv1.2 --fail --silent --show-error --head https://beacons.lol/releases/stable.json
curl --proto '=https' --tlsv1.2 --fail --silent --show-error --head https://beacons.lol/downloads/secblitz-0.4.2-windows-x64-setup.exe
curl --proto '=https' --tlsv1.2 --fail --silent --show-error --head https://beacons.lol/
curl --proto '=https' --tlsv1.2 --fail --silent --show-error --head https://www.secblitz.lol/releases/stable.json
```

Repeat the temporary-directory download and pinned-key verification above using
`beacons.lol` for both fetches; expect the same 0.4.2 manifest and installer bytes.
Test an actual installed 0.4.1 client's upgrade through that compatibility origin.
The preparation agent has not performed these production network checks or
published the 0.4.2 migration.

Keep the previous working deployment available for hosting rollback; clients
still enforce signed versions, freshness and pinned-key verification.
