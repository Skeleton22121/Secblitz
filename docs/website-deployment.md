# Secblitz website

## Published 0.7.0 release

Deployed from `dist/pages` (35 allowlisted files) to production on 2026-10-05. Deployment: https://b5736d75.secblitz.pages.dev. After upload, https://secblitz.lol and the legacy https://beacons.lol feed returned the exact signed local bytes with `Cache-Control: no-store`, and `prepare-pages.py --verify-feed` accepted both feeds plus the downloaded installer against the pinned key. The live installer, portable binary and hero video matched the hashes below; the homepage links and displayed checksum identify the 0.7.0 installer. Downloads are served with `max-age=14400` (a zone browser-cache setting overriding `_headers`); the versioned filenames make this harmless.

- Installer: `dist/secblitz-0.7.0-windows-x64-setup.exe`, 8,152,862 bytes, SHA-256 `2d01dba23210c797c9f99bc56c9b4c72a0d6325905288786564570e9bc21770b`.
- Executable: `dist/secblitz.exe` / `secblitz-0.7.0-windows-x64.exe`, 19,878,400 bytes, SHA-256 `fd9b4bff5900c64adbb23a6135d7414dde6254d630cbb7ee5e0dbe30e8625ee8` (cross-built GNU, `target/windows-release`).
- Change: the iced desktop GUI replaces the terminal UI; Clean up apps keeps an offline copy of every removed app.
- Installer compiled with ISCC in the UI clone. Native checks: silent upgrade over 0.7.0 with the monitor service running exited 0; `update health --json` reported version 0.7.0, task ready, monitor running. After publication, the installed 0.7.0 client's `update check --json` returned `up_to_date` against the live feed and Settings showed "Up to date". No live automatic upgrade from 0.6.1 is claimed.
- Before publication, a 0.7.0 client reported "Couldn't check": `newer()` rejects a feed older than the installed version ("Update downgrade rejected"). That is the anti-rollback guard working; only pre-release builds can hit it.
- Website: the hero plays a 20-second seamless loop of real 0.7.0 footage (`assets/intro-a18b68fac12f.mp4`, 3,140,150 bytes, 1920x1080, H.264, no audio; source and cut list in `video/sources-v070.json`) over a themed 16:9 still (`assets/preview-541ff80cb74f.webp` light, `assets/home-dark-1215a72074e4.webp` dark). Reduced motion, Save-Data and media errors keep the still.
- Website follow-up (deployment https://576115ff.secblitz.pages.dev, 36 files): a VP9 WebM (`assets/intro-a8f1e9ace5d9.webm`, 2,032,641 bytes, SSIM 0.992 against the render) is offered before the MP4; `app.js` and `theme.js` are loaded with `?v=070` because the zone serves scripts with `max-age=14400`, so a visitor could keep a pre-release script for hours. A Play/Pause button over the video lets reduced-motion and Save-Data visitors start the tour and lets everyone stop it. Live checks in Chromium and Firefox: autoplay, pause and resume, reduced motion starting on the still, and MP4 fallback when the WebM fails. Cloudflare Web Analytics injects a beacon that the site's CSP blocks; turn it off in the zone.
- Hero back to screenshots (deployment https://21ddeaaf.secblitz.pages.dev, 34 files): at the owner's request the video, its Play/Pause button and its script were removed; the hero shows `assets/app-home-light.webp` and `assets/app-home-dark.webp` (1100x720) by theme again. The video files moved to `LEGACY_ASSETS`. Live check: Chromium and Firefox, light and dark, show the matching screenshot and no page errors.
- 0.6.1 bytes, feed and checksums are archived in `dist/archive/0.6.1`; its download URLs remain published. Windows publisher signing is still absent. `scripts/test-video.py` and `scripts/test-web-security.py` still pin older releases and were not rerun; the browser checks for this release covered light, dark, reduced motion and 390 px width.

## Published 0.6.1 release

Deployed from `dist/pages` (26 allowlisted files) to production on 2026-10-04. Deployment: https://654011b6.secblitz.pages.dev. After upload, the https://secblitz.lol and legacy https://beacons.lol feeds returned the exact signed local bytes with `Cache-Control: no-store`; the live 0.6.1 installer, 0.6.1 portable binary and retained 0.6.0 installer matched the hashes below; the homepage links and displayed checksum identify the 0.6.1 installer. No live client automatic-upgrade test is claimed.

- Installer: `dist/secblitz-0.6.1-windows-x64-setup.exe`, 4,330,089 bytes, SHA-256 `b7680b902cd968d21178fa464fd5f97e01fb01bbbe17f958b696959b88929f20`.
- Executable: `dist/secblitz.exe` / `secblitz-0.6.1-windows-x64.exe`, SHA-256 `7388547cc7d1efe0520ef20afab938ecf6abee2db9c8a83b2bd2c9a3819d7c27` (cross-built GNU, `target/windows-redesign`).
- Change: terminal UI and report redesign only; updater, engine and installer sources are unchanged from 0.6.0. Host suite: 384 passed.
- Installer compiled with ISCC in the offline UI clone. Native smoke (`target/windows-release-v061/`): silent install exit 0, installed `--version` 0.6.1, `SecblitzUpdate` task Ready as SYSTEM with `update check`, uninstall exit 0 with no exe/task/shortcut left; clone preference, ACL and journal hashes restored. `update health --json` exited 1 on this clone because it carries a remembered opt-out and no `ProgramData\Secblitz\Updates` directory; the published 0.6.0 installer behaves identically under the same conditions, so this is pre-existing and not a 0.6.1 regression. The full 0.6.0 acceptance suite was not rerun.
- 0.6.0 bytes, feed and checksums are archived in `dist/archive/0.6.0`; its download URLs remain published. Windows publisher signing is still absent.

## Published 0.6.0 release

The requested 0.6.0 build was deployed from `dist/pages` (24 allowlisted files) to production on 2026-10-03. Deployment: https://2e8cf067.secblitz.pages.dev. The primary https://secblitz.lol homepage links, installer and portable hashes, Ed25519 feed signature/version, feed `Cache-Control: no-store`, and direct legacy https://beacons.lol feed were verified after upload. No live client automatic-upgrade test is claimed by these HTTP checks.

- Installer: `dist/secblitz-0.6.0-windows-x64-setup.exe`, 4,310,119 bytes, SHA-256 `0df567b3747be1dcf493a65b9c8bd3385d4f650aed5e55130441640e7d11654e`.
- Executable: `dist/secblitz.exe`, SHA-256 `30d68a0380caf139cad52df093d7afc8a6c8e26630d51c574c5a186f1f88f855`.
- This includes the native-observed health fix accepting Task Scheduler's canonical `SYSTEM` account name as well as its exact SID. Installed health returned schema 1, version 0.6.0, task ready, monitor absent.
- Focused installation/upgrade/uninstall checks passed; original journal bytes/ACLs and retained app directory were preserved. Evidence: `target/windows-release-v060/acceptance-final.json`.
- Pinned-key release metadata is signed; staging and `prepare-pages.py --site dist/pages --require-feed --expected-version 0.6.0` passed. Old 0.5.0 bytes/feed/checksums are archived in `dist/archive/0.5.0`; historical download URLs are retained.
- Windows publisher signing is still absent. The homepage recording remains older footage.

Authenticate on the operator machine with `npx --yes wrangler@4.147.0 login`, or supply a scoped Pages deployment token through the process environment. Do not put credentials in repository files or chat. Then publish:

```sh
CLOUDFLARE_ACCOUNT_ID=1d82b8d6e163ce4a892083cc5de745a8 npx --yes wrangler@4.147.0 pages deploy dist/pages --project-name secblitz --branch main
```

Verify the public signed feed, exact installer/portable hashes, no-store feed headers and direct legacy-origin compatibility after deployment. The previous release documentation follows for historical context.

Static site for Cloudflare Pages. The browser has no dependencies or third-party requests.
Publication uses a Python allowlist staging step with `cryptography` available.

- **Build command:** `python3 scripts/stage-pages.py --expected-version 0.5.0`
- **Output directory:** `dist/pages` (never upload `website` directly)
- **Primary website origin:** `https://secblitz.lol/`
- **Preview locally:** `python3 -m http.server 57435 --directory website`

## Files

| File | Purpose |
| --- | --- |
| `index.html` | The page |
| `404.html` | Branded not-found page |
| `styles.css` | All styles |
| `app.js` | Automatic video playback policy and checksum copy |
| `_headers` | CSP, security headers, caching |
| `assets/fonts/` | Self-hosted Schibsted Grotesk and upstream OFL.txt |
| `downloads/` | Versioned installer and portable release binaries |

## Shipping a new installer

1. Copy `dist/secblitz-X.Y.Z-windows-x64-setup.exe` into `downloads/` and copy `dist/secblitz.exe` as `downloads/secblitz-X.Y.Z-windows-x64.exe`.
2. In `index.html`, update both download `href`s, the version in the collapsed Download details, the size and the `SHA-256` in `#sha` (`sha256sum downloads/*.exe`). Keep the button label version-free.

Cloudflare Pages limits files to 25 MiB.

## Primary-domain migration

The website title, canonical URL and Open Graph URLs now use `secblitz.lol`.
The 0.5.0 download links remain relative. The footer uses the Secblitz brand.
The current 0.5.0 updater is compiled for `https://secblitz.lol`. The release
artifacts and signed feed are staged for main to publish on the shared Pages
project. Older clients retain their compiled origin until upgraded.

Keep `beacons.lol/releases/*` and `beacons.lol/downloads/*` serving directly,
without redirects. Existing clients need the old compiled origin to receive
the signed migration release. Retain all published versioned binaries.

### Active redirects configured by main

The official [Pages redirects documentation](https://developers.cloudflare.com/pages/configuration/redirects/)
marks domain-level redirects as unsupported. No `_redirects` file was added:
an unqualified `/` rule would also affect the new primary host, while an old-host
wildcard could break update/download requests. Main has activated zone-level Single Redirects,
as documented in [Cloudflare's redirect settings](https://developers.cloudflare.com/rules/url-forwarding/single-redirects/settings/).

| Zone | Match expression | Target | Status |
| --- | --- | --- | --- |
| `beacons.lol` | `(http.host eq "beacons.lol" and http.request.uri.path eq "/")` | Static `https://secblitz.lol/` | 301 |
| `secblitz.lol` | `(http.host eq "www.secblitz.lol")` | Dynamic `concat("https://secblitz.lol", http.request.uri.path)` | 301 |

Both rules preserve query strings; the www rule also preserves paths. Active
configuration is confirmed by main's handoff, not a new live probe in this task.
Keep old-origin feed/download URLs serving directly without `Location` and retain
`Cache-Control: no-store` on the feed. The same signed feed and stable signing key
support both domains through the shared Pages project.

### Current release: 0.5.0

The exact artifacts from the native-accepted 0.5.0 release handoff are staged:

| File in `downloads/` | Bytes | SHA-256 |
| --- | ---: | --- |
| `secblitz-0.5.0-windows-x64-setup.exe` | 3,850,954 | `c17a543fccbb0ec1c37c487aeb8da2e7bfd8a832e04996f6ead32e6dd2b77f3b` |
| `secblitz-0.5.0-windows-x64.exe` | 4,851,712 | `036d8b69367cb7422ca5d6e821e73749f1c36ce35df437ac47b7d83ba829a6d9` |

Both primary download links target the installer and display its checksum.
The portable binary is staged at `/downloads/secblitz-0.5.0-windows-x64.exe`.
The optional portable HTML link is withheld because the current release gate
requires every EXE link to be the canonical installer path. The gate owner must
support portable links before adding that secondary link. Existing 0.3.0 and
published 0.4.0, 0.4.1, 0.4.2 and 0.4.3 binaries are retained unchanged. Users on 0.4.0 need
to download the new installer once to fix automatic updates. This migration note
is retained here rather than in the public FAQ.

Automatic updates are implemented and selected by default in the installer.
The native-tested hourly SYSTEM scheduled task checks its compiled release origin
and verifies Ed25519-signed metadata. It does not force an app close or reboot.
The release owner's newly signed 0.5.0 `releases/stable.json` is used without
modification. Main reports four reviews passed, host acceptance of 128 library
and 66 CLI tests, and native acceptance of 138 library and 66 CLI tests. The final
accepted build includes the stale copy-only Undo instruction fix. These are main's
results, not native tests rerun by this website task. Final artifacts are copied
byte-for-byte without packing, rebuilding or native transformations.
The installer remains unsigned, with the existing collapsed Download details
disclosure unchanged. This update does not claim live 0.5.0 publication or upgrade
success before main's deployment checks.

The 0.5.0 website copy describes read-only scanning, recommended or individual
choices, approved fixes followed by a fresh check, recognition of existing Windows
protection, and read-only storage/power/restart readiness. It does not claim disk
repair, a new antivirus engine or that informational findings are issues. The
recording remains the original 0.3.1 footage with unchanged bytes and metadata.

Release gate, run from the project root:

```sh
python3 scripts/prepare-pages.py --require-feed --expected-version 0.5.0
```

## Recording integration: real assets verified locally

The player directly below the hero in `#how` uses `assets/intro-6bb434a9c067.mp4`
and `assets/preview-33b342ab21fb.webp`. It stays hidden until video metadata loads and hides
again on a media error.

The supplied Remotion render uses actual Secblitz 0.3.1 keyboard UI footage.
The pipeline handoff identifies Take03 and a verified single-firewall fix and
Undo fixture. The marketing render removes the technical footer. The page has
no figcaption or caption description reference; its accessible video label is
"See Secblitz in action". Original application text remains in the footage.

- MP4: 613,240 bytes, 33 seconds, 1280 x 720, H.264.
- MP4 SHA-256: `6bb434a9c067377bf2a23225c70a52fde824a5ee932dcfcc1aef330d4ebd6d94`.
- Poster: 56,674 bytes.

The video, both poster references and Open Graph image use content-hashed
filenames with unchanged bytes. The stylesheet retains `?v=marketing`.
The release gate resolves local URL paths independently of query strings.

Playback is muted, looping and inline with metadata preloading. Automatic playback
requires visibility and is disabled by reduced-motion or Save-Data preferences.
There are no playback buttons, native controls, control bar or playback prompts.
Both `muted` and `defaultMuted` are enabled. Picture-in-picture and remote playback
are disabled, with `nodownload nofullscreen noremoteplayback` control restrictions.
The autoplay property is enabled in JavaScript only after preferences, metadata
and visibility are checked, preventing early autoplay during HTML parsing.
Offscreen and hidden-document playback pauses and automatically resumes when
visible again. Reduced motion, Save-Data and autoplay denial show a static poster,
including when preferences change after playback has started. Denial is quiet and
does not cause repeated playback attempts.
The four steps are explanatory text until verified chapter timestamps arrive.

Run from the project root:

```sh
uv run --no-project --with playwright python scripts/test-video.py --real --site dist/pages
```

Download assertions are pinned to the immutable 0.5.0 installer and portable
binary sizes and hashes. The full browser suite passed against the marketing
render. Media hash prefixes are pinned to their filenames. Future delivery updates
must update the references and staging allowlist when the actual bytes change.

Without `--real`, only missing-media and mocked playback-policy tests run. With it, the
suite also reports the actual asset hash and checks metadata, decoded frame pixels at 5 and
20 seconds, time advancement, absence of controls, looping, automatic
playback, reduced motion, offscreen pause/resume and mobile
inline playback. Layout checks cover 320, 390 and 768 pixels, plus desktop.
Copy checks open all FAQ and download disclosures, reject em dashes and retired
technical language, verify the exact tagline and accessible video label, and
confirm that release details start collapsed. Byte-range handling and mocked
media routes support the cache-busting query string.

The real-media test server implements byte ranges for seeking and applies the
site's CSP. Actual playback passes with `media-src 'self'` and no console errors.
Save-Data and document visibility are injected preferences in the real tests;
media methods and IntersectionObserver are native. This browser harness keeps
background tabs visible, so a native tab-switch smoke test remains a deployment
check. Quiet autoplay denial is covered separately by the mocked policy suite.

Browser screenshots are saved outside the site under `/tmp/opencode/`:

- `secblitz-real-frame-5s.png`: decoded Choose screen.
- `secblitz-real-frame-20s.png`: decoded public-network firewall Fixed result.
- `secblitz-real-poster-desktop.png`: reduced-motion still poster.
- `secblitz-real-desktop.png`: desktop recording without controls.
- `secblitz-marketing-desktop.png`: full-page desktop marketing copy.
- `secblitz-real-mobile-{320,390,768}.png`: full-page responsive captures.

The decoded frames are visibly different and their pixel hashes differ. Both
frame captures and the desktop/mobile layout captures were visually reviewed.

### Deployment and release handoff

1. Stage and validate, then deploy only `dist/pages` to the configured Cloudflare Pages project for
   `https://secblitz.lol/`. Canonical and Open Graph URLs target this origin.
2. Check the deployed poster, video byte-range responses and native tab-switch
   pause/resume. Verify mobile playback on a physical phone in addition to the
   Chromium mobile emulation already checked locally.
3. Verify deployed CSP with `media-src 'self'` and confirm `/releases/*` returns
   `Cache-Control: no-store`. Local CSP verification is not a deployment check.
4. Confirm the deployed 0.5.0 installer and portable binary match the hashes above.
5. Complete the live signed-feed update E2E test after publication. Do not claim
   live upgrade success based only on the native acceptance or local release gate.

The original direct-source gate passed with 18 files, including documentation and
tests which were subsequently confirmed public. Publication now uses an explicit
allowlist. Documentation lives here and browser tests live in `scripts/test-video.py`.
Both new binaries passed local HTTP download size/hash checks and byte-for-byte
comparison with `dist`.
The light layout is preserved with benefit-first hero, outcome cards, simple
Scan/Choose/Fix/Undo steps and a shorter FAQ. Version, unsigned-publisher notice
and checksum live in the native collapsed Download details disclosure. The
existing checksum copy hook is retained. Production deployment checks and the
live signed-update E2E test remain separate from local validation.
The video remains the real 0.3.1 recording with no page caption or burned-in
technical footer. Silent automatic playback has no visible controls.

### Source-exposure remediation handoff

The source README and test have been moved outside `website`. The repository-root
README remains untouched. Only content-hashed media names are staged; the two old
source media copies are retained locally but never included in the deployment.
`404.html` supplies the site's not-found page. Local Python serving verifies status
404 for absent paths and separately verifies the branded page. Production behavior
for the four retired paths is now HTTP 403 under main's narrow WAF rule, as recorded
below; do not treat that as a failed local 404 test or claim production returned 404.

```sh
python3 scripts/prepare-pages.py --require-feed --expected-version 0.5.0 &&
python3 scripts/stage-pages.py --expected-version 0.5.0 &&
python3 scripts/prepare-pages.py --site dist/pages --require-feed --expected-version 0.5.0
```

Main can then publish `dist/pages` using the authorized production environment.
Verify `/README.md`, `/test_video.py`, `/assets/secblitz-demo.mp4` and
`/assets/poster.webp` remain blocked without source content. No publication or
Cloudflare operations were performed by this website task. The staged current
installer/feed is 0.5.0.

#### Production remediation confirmed by main at the 0.4.3 handoff

Main reports that source responses persisted in Cloudflare's inner cache after
purging, so main added a narrow WAF block for only the four retired paths above on
both apex domains, `secblitz.lol` and `beacons.lol`. Main's subsequent checks found
HTTP 403 with no source bytes on those paths, while current renamed media and the
feed returned HTTP 200. The source-exposure mitigation is therefore confirmed
publicly by main's checks, not merely staged locally.

Main also deleted four retired Pages deployments carrying publicly reachable
source URLs. The only active deployment reported at this handoff is `349ba11f`,
the clean staged deployment. This records the production state before main publishes
the new 0.4.3 snapshot; it does not claim `349ba11f` already contains 0.4.3. This task
did not independently repeat main's remote checks or access Cloudflare credentials.

The font license is copied verbatim from the official Google Fonts source:
https://raw.githubusercontent.com/google/fonts/main/ofl/schibstedgrotesk/OFL.txt

### Staging verification history: 2026-10-03

During the 0.4.2 path-remediation task, all 18 adversarial release tests passed after updating their fixtures for the new
license and asset names. Tests include safe replacement of the two known legacy
staged media names. Source preparation, staging and final staged preparation passed.
The real-media browser suite passed against `dist/pages`, including local HTTP 404
responses for excluded paths, the branded error page/CSS, both release download
hashes, 33-second H.264 decoding, distinct decoded frames, silent looping playback,
reduced motion, offscreen pause/resume, no captions/controls, approved marketing
copy and mobile widths 320, 390 and 768. Delivery scripts passed Node syntax checks.
No renderer was run. Both renamed media copies match the originals byte-for-byte.

For the earlier 0.4.3 promotion, source preparation, staging and staged preparation
all passed with `--expected-version 0.4.3`. Staging produced 20 allowlisted files.
The staged feed is byte-identical to main's signed source feed, and both staged
0.4.3 executables were byte-identical to that release's promoted `dist` artifacts.
No stage-script change was needed: main had already
added the 0.4.2 installer and portable binary to the historical allowlist.

The full real-media browser suite passed again against the 0.4.3 staged snapshot:
both new downloads, links/checksum, approved marketing copy, no captions/controls,
local excluded-path 404s, branded error page, media decoding/playback, reduced
motion and mobile layouts. Both renamed media hashes remain unchanged. No build,
rerender, signing, packing, native source change or publication was performed here.

For the final 0.5.0 promotion, source preparation, staging and staged preparation
passed with `--expected-version 0.5.0`. The staged feed matches main's newly signed
source feed byte-for-byte, and both staged binaries match the final promoted
artifacts and exact hashes above. Main had already added 0.4.3 to the historical
allowlist, so no stage-script edit was required.

The full staged browser suite ran once and passed: both 0.5.0 downloads, current
links/checksum, the new benefit-copy assertions, initially collapsed Download
details, no em dashes/captions/video controls, local excluded-path 404s, branded
error page, real H.264 decoding, silent looping playback, preference handling and
mobile widths 320, 390 and 768. The displayed installer size is 3.9 MB, rounded
from 3,850,954 bytes using decimal megabytes. The original recording and poster
retain their complete hashes and 0.3.1 provenance. No publication was performed.

The current 0.5.0 publication snapshot contains exactly these 22 files:

```text
404.html
_headers
app.js
index.html
styles.css
assets/secblitz.svg
assets/intro-6bb434a9c067.mp4
assets/preview-33b342ab21fb.webp
assets/fonts/schibsted-grotesk-latin.woff2
assets/fonts/OFL.txt
downloads/secblitz-0.3.0-windows-x64-setup.exe
downloads/secblitz-0.4.0-windows-x64-setup.exe
downloads/secblitz-0.4.0-windows-x64.exe
downloads/secblitz-0.4.1-windows-x64-setup.exe
downloads/secblitz-0.4.1-windows-x64.exe
downloads/secblitz-0.4.2-windows-x64-setup.exe
downloads/secblitz-0.4.2-windows-x64.exe
downloads/secblitz-0.4.3-windows-x64-setup.exe
downloads/secblitz-0.4.3-windows-x64.exe
downloads/secblitz-0.5.0-windows-x64-setup.exe
downloads/secblitz-0.5.0-windows-x64.exe
releases/stable.json
```

Poster SHA-256: `33b342ab21fb062973b130b2125c7de377d11b78e4e6b1456658237630f607ad`.
OFL.txt SHA-256: `3b4f3063b6ac7c1e403e2c4a5e8ef3a58190ff83ed7b15af66511858699139ce`.
The MP4 hash is recorded above and both media records are retained in
`video/delivery.json` with their new delivery paths. Hash mismatches fail staging.
