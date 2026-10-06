# Website and update-delivery security review

Review date: 2026-10-03 UTC. Live observations: approximately 05:11 to 05:20 UTC.
Authorized production reference: Pages deployment `7d84031b`, marketing edition,
as supplied by the owner. No authenticated deployment lookup was performed.

## Conclusion

No exploitable website injection, redirect, secret disclosure or signed-update
authentication bypass was confirmed within the bounded review. No website source
change was justified. There are two informational deployment observations and an
important existing initial-download trust limitation, detailed below.

The HTTP checks, live artifact hashes, pinned-key feed verification, source
comparison and local release gate passed. Browser initial-load, reduced-motion
and clipboard checks produced positive evidence. **Full browser acceptance did
not pass:** renderer/browser crashes prevented a reliable playback and framing
enforcement result. This is not represented as a website vulnerability, a passed
test, or a resolved issue.

## Scope and preservation

- Reviewed `website/app.js`, `website/index.html`, `website/_headers`,
  `website/styles.css` and the static SVG. Read the updater's origin, verification
  and HTTP client code to assess the website-to-installed-client trust boundary.
- Live targets were only `secblitz.lol`, `www.secblitz.lol` and the authorized
  `beacons.lol` compatibility endpoints. Browser routes blocked external hosts.
  No Cloudflare account/API, Pages preview host, login or credentials were used.
- Only fixed, low-volume public HTTP requests and browser interactions were used.
  No bulk fuzzing, write methods, load tests, exploit uploads or executable runs.
- Config/key-like paths received **HEAD only**. No private-key or environment
  response body was requested, printed or saved. The test stops on a successful
  non-HTML response to those probes for owner investigation.
- Added only this report and `scripts/test-web-security.py`. All four website
  files, marketing copy, media, downloads, signed feed, installer/core/signing and
  deployment configuration were left untouched. No deployment or VM operation.
- `docs/windows-v042-results.md` remains unchanged and is the authoritative prior
  native upgrade record. This review does not claim a new Windows upgrade run.

## Findings and observations

### W-01: Public operational documentation and test source

**Severity: informational. Status: confirmed live, unchanged.**

`GET /README.md` returned `200`, `text/markdown`, 10,353 bytes.
`GET /test_video.py` returned `200`, `application/octet-stream`, 16,459 bytes.
Both matched the corresponding local website files byte-for-byte. They describe
build/test workflow, local temporary paths, release hashes and hosting behavior.
The reviewed files contain no credentials or signing private key. Serving Python
source does not execute it on Pages. `nosniff` and the global CSP were present.

Evidence:

| Public file | SHA-256 |
| --- | --- |
| `/README.md` | `4db57814276fdc1e67fbf69cfdeb7132f79b96deabb0676932c990343d5c00b4` |
| `/test_video.py` | `1ca99257fc35c6a301d90959dbaf4a25ee7e4089fba0d6ecbb6cf713fe736359` |

Recommendation for the deployment owner: decide whether these operational files
belong in the published output. Moving/excluding them is outside this task's
website edit allowlist. Adding headers would not make their contents private.
No removal, deployment or remediation is claimed.

### W-02: Live asset caching differs from the local download rule

**Severity: informational, deployment consistency. Status: confirmed live.**

Local `_headers` declares `Cache-Control: no-cache` for `/downloads/*`.
Both HTTPS origins instead returned `Cache-Control: max-age=14400` on the actual
installer and portable binary. Responses included `CF-Cache-Status: REVALIDATED`
or `MISS`. JavaScript, CSS, SVG, poster and MP4 also had a four-hour freshness
lifetime; font caching was one year immutable. The homepage remained `no-cache`
and both signed feeds remained **`no-store`**.

This does not demonstrate cache poisoning or an update signature bypass. Both
versioned artifacts matched their expected immutable bytes. A change at an
existing asset URL can nevertheless remain fresh in a browser for four hours.
`must-revalidate` does not force revalidation while a response is still fresh.

The edge configuration responsible for the difference was not inspected using
credentials. Main should reconcile its cache rules with the intended policy and
recheck actual responses if it changes them. The existing local download rule
already expresses `no-cache`; duplicating it here would not verify an edge fix.
No cache poisoning or cross-user cache manipulation was attempted.

### T-01: Initial unsigned installer has a different trust boundary

**Classification: existing architectural trust limitation, not a newly proven
exploit or an assigned CVSS score.**

The download page explicitly identifies the installer as unsigned. A first-time
visitor obtains both the executable and the displayed SHA-256 from the same
HTTPS site. A hosting/deployment compromise could replace both. Comparing that
page's hash catches accidental mismatch, but does not independently authenticate
the publisher when the site itself is compromised. The browser download flow
does not verify the Ed25519 feed. Current hashes are correct at review time.

An already-installed genuine client instead uses its compiled public key and
origin. A compromised web origin alone cannot generate a valid signature for
arbitrary replacement installer bytes. Authenticode signing, independent first-
download provenance and release-key/build security are separate concerns. There
was no request to change signing or installer behavior, and none was changed.

## Source and live injection review

| Surface | Evidence and result |
| --- | --- |
| DOM XSS / raw HTML | `app.js` has no `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `document.write`, `eval`, `Function` or HTML-template sink. UI updates use `textContent`, `hidden` and class changes. No untrusted content source reaches HTML parsing. |
| Query / fragment | No URL-query or fragment parser in app code. Plain marker and URL-encoded `<plain>`, quotes and ampersand returned exactly the baseline HTML. A browser query/fragment marker did not enter visible body text. No executable or external-network payload was submitted. |
| Clipboard | Click handler reads `textContent` from fixed `#sha` via static `data-copy="sha"`. Browser copy returned exactly the 64-character expected installer hash, with no command or newline. Failure fallback selects that same node. No URL parameter controls the target or copied value. |
| Open redirect | No JavaScript navigation or dynamic URL construction. `next=` on the apex did not redirect; on www/legacy roots it was preserved as query data while the destination host remained the fixed apex. A double-slash www path also remained on the apex. |
| SVG / JS dependencies | SVG has two static paths, no script, event handler, external reference or `foreignObject`. CSS font/image references are local. Browser initial HTML/DOM listed only `https://secblitz.lol/app.js` as a script. |
| CSP injection | Query probes did not change page bytes or the static CSP. No template interpolates request data into that policy. Header-control-byte injection was not sent. A Playwright in-page eval polling helper was blocked by the live `unsafe-eval` restriction; the harness was changed instead of the policy. |
| Clickjacking | Live apex sends `frame-ancestors 'none'` and `X-Frame-Options: DENY`. These are correct protective headers. The separate cross-origin iframe browser test did not complete reliably, so browser enforcement is not claimed as verified. |
| CORS | Public static responses, including feed and downloads, send `Access-Control-Allow-Origin: *`; tested responses had no `Access-Control-Allow-Credentials`. The Origin probe used the owned www host. These public unauthenticated resources have no demonstrated cross-origin confidential-data exposure. CORS is not the updater's authenticity mechanism. |

Live baseline HTML was 10,569 bytes:
`9b5aa24382ee165118a71f2d3d4f295753f7611bfc989bd50bfa5a37cdce3946`.
Supplemental identity-encoded GETs confirmed exact local/live matches for HTML,
JavaScript, CSS, SVG, README and the existing video test script.

### Security headers observed on successful static responses

```text
Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; media-src 'self'; font-src 'self'; connect-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; upgrade-insecure-requests
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
Referrer-Policy: no-referrer
Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=(), usb=()
Strict-Transport-Security: max-age=31536000
```

No `unsafe-inline`, `unsafe-eval`, broad script host or remote font dependency was
observed. The policy permits same-origin script files, so future upload or dynamic
script-serving features would need their own review. None exists in the reviewed
static application. Source-controlled CSP does not protect against an attacker
who can replace both the page and response policy.

Cloudflare supplied `Speculation-Rules: "/cdn-cgi/speculation"` on the primary
host, and the browser requested that same-origin endpoint. Responses also carried
Cloudflare NEL/reporting headers in the initial curl probe. Those headers are not
an injected remote script. No external request was observed in the successful
initial-page/clipboard capture, but that short observation does not establish
that all browser-managed reporting is absent under other conditions.

## File exposure and traversal probes

Public paths were compared with a deliberately missing-page control, rather than
treating every `200` as a leaked file.

| Probe | Result |
| --- | --- |
| `/security-review-missing-20261003`, `/public/README.md` | `200 text/html`, byte-for-byte baseline index. Confirmed SPA fallback, not leaked documents. |
| `/assets/../README.md` | Same public website README, not repository-parent contents. Normalized access inside the public output. |
| `/assets/%2e%2e/README.md` | `200`, exact baseline index fallback. |
| `/%2e%2e/README.md`, `/%2e%2e/%2e%2e/README.md` | `400`, 155-byte generic response; no file disclosure. |
| HEAD `/.git/config`, `/.env`, `/wrangler.jsonc`, `/_headers`, `/assets/update-public-key.hex` | `200 text/html`, headers consistent with the missing-page fallback. Bodies deliberately not fetched. Not evidence of a configuration leak. |
| HEAD `/release-signing-key.pem`, `/.config/secblitz/release-signing-key.pem` | `200 text/html`, consistent with fallback. No private-key body requested. |
| HEAD `/assets/%2e%2e/%2e%2e/.env`, `/%2e%2e/.git/config`, `/%2e%2e/.config/secblitz/release-signing-key.pem` | `400`; no body requested. |

For HEAD-only paths, content type and matching fallback headers are suggestive,
not a cryptographic proof of identical response bodies. No claim of exhaustive
secret-path coverage is made. The local Pages validator also passed its targeted
secret/symlink/output checks across 18 files; that is not a universal secret scan.

## HTTPS, redirects and updater compatibility

TLS certificate chain and hostname validation succeeded on all three hosts using
TLS 1.3 / `TLS_AES_256_GCM_SHA384`. Certificate expiration dates were 2027-01-01.
Separate TLS 1.2 handshakes succeeded for the two updater apex hosts using
`ECDHE-ECDSA-CHACHA20-POLY1305`. Older-protocol rejection and all cipher suites
were not enumerated.

| Request | Observed direct response |
| --- | --- |
| HTTP apex root | `301 https://secblitz.lol/` |
| HTTP/HTTPS www root | `301 https://secblitz.lol/` |
| HTTP/HTTPS legacy root | `301 https://secblitz.lol/` |
| HTTPS www feed and installer | `301` to matching apex path, as intended for the canonical website host |
| HTTPS apex feed / installer / portable | `200`, no `Location` |
| HTTPS legacy feed / installer / portable | `200`, no `Location` |
| HTTP apex feed and installer | `301` to their HTTPS apex paths |
| HTTP legacy feed and installer | `301` to their HTTPS legacy paths, preserving compatibility origin |

The source's no-redirect policy is implemented as
`reqwest::redirect::Policy::none()` in `src/updater/windows.rs:734-750`, with
`https_only(true)`, `no_proxy()`, a 15-second connection timeout and a 120-second
shared download budget. Genuine clients use compiled apex HTTPS origins, not
www or HTTP. Direct HTTPS compatibility endpoints therefore match that policy.
The probe client did not follow redirects, avoiding a false success from an
HTML homepage after a redirect.

## Signed feed and actual downloaded binaries

Both hosts returned the same 450-byte envelope, SHA-256:
`1369eecdbb9c3f7055f9843a0b12e6f8fc882df93d79bbe70119233219907a8b`.
The exact decoded payload's Ed25519 signature verified using only the repository
public pin. Version was `0.4.2`, target `windows-x86_64`, publication
`1790999969`, expiration `1798775969`, with a 90-day validity interval. It was
fresh at review time. The public feed was never changed or re-signed.

Actual full GET downloads were hashed in memory from **each** HTTPS apex:

| Artifact | Bytes | SHA-256 on both origins |
| --- | ---: | --- |
| `secblitz-0.4.2-windows-x64-setup.exe` | 3,813,017 | `28c93869508923b2ea865267025dc6c0d9e92e5d343e5a78f175d32e5b1b83c4` |
| `secblitz-0.4.2-windows-x64.exe` | 4,697,088 | `78c666e456b3f72fbe503e4aad6213b1baec4f7172e3e8cacd17f9bf893b4672` |

The signed feed binds the **installer**, not the separately served portable file.
The portable digest comparison uses the existing release record, not a claim
that the feed signs it. Both agree with `docs/windows-v042-results.md`.

Read-only source review of `src/updater.rs:58-139` confirmed HTTPS origin
validation, strict Ed25519 verification before payload use, fixed filename and
target checks, manifest/payload/installer bounds, freshness, installed-version
downgrade rejection and exact installer size/SHA-256 verification before staging.
Unknown and duplicate fields are rejected by typed serde deserialization. No
runtime URL, key or path supplied by the website is accepted as a trust anchor.

Residual boundary: origin compromise can withhold updates or replay still-valid
signed metadata. Version comparison prevents downgrading below the running
version, but is not a persistent highest-ever-observed-release state. Expiry uses
the local clock. These documented constraints are not a newly demonstrated
signature forgery. Native protected staging and installation were not retested.

## MIME, compression, range and cache evidence

- HTML: `text/html; charset=utf-8`; JS: `application/javascript`; CSS:
  `text/css`; SVG: `image/svg+xml`; feed: `application/json`; font:
  `font/woff2`; poster: `image/webp`; video: `video/mp4`.
- Both executables: `application/octet-stream`, `Content-Disposition: attachment`
  and `nosniff`, for full responses on both origins.
- A single `Range: bytes=0-63` GET returned `206`, exactly 64 bytes, and
  `Content-Range: bytes 0-63/3813017` for the installer. The video returned
  `bytes 0-63/613240` with `video/mp4`. No multipart/oversized-range fuzzing.
- `Accept-Encoding: gzip, br` produced Brotli with `Vary: accept-encoding` for
  HTML, JS, CSS, SVG and feed. Feed signature verification used the separate
  identity-encoded response; compressed wire hashes are not installer hashes.
- Matching app.js ETag produced `304` with protective headers retained. A
  deliberately nonmatching homepage ETag produced `200` with the same HTML.
- No reflected secret and attacker-controlled text combination was found that
  would justify claiming a compression side-channel vulnerability on this site.

## Browser results and limits

Successful observations used Playwright with installed Chromium 153.0.8010.12:

- Initial response hash matched HTTP and local source; one self-hosted script.
- No marker reflection, console/page error or external request in the captured
  initial-page and clipboard sequence.
- Video source: `https://secblitz.lol/assets/secblitz-demo.mp4?v=marketing`.
- With reduced motion: paused at time zero, autoplay false, muted/defaultMuted
  true, loop true, controls false and the still visible. No figcaption or playback
  button. Source retains preference-aware autoplay for ordinary motion settings.
- Real clipboard interaction copied the exact published checksum.

**Incomplete:** the local renderer repeatedly returned `Target crashed` while
interacting or attempting playback. Both installed Chromium 153 and 149 were
tried; disabling GPU allowed initial-page/clipboard observations but did not
establish reliable playback. An isolated framing attempt also encountered
browser failure, including a later launch terminating with `SIGTRAP` before
navigation. This prevents attributing the crashes specifically to website code.
No CSP bypass, browser-policy weakening or site edit was used to force a pass.

The framing harness uses a locally fulfilled minimal parent at the owned www
URL, whose iframe requests the live apex. Earlier execution did not satisfy its
enforcement assertion; the separate diagnostic retry failed at browser launch.
Consequently the **headers are verified, browser clickjacking enforcement is a
remaining verification item**. Playback advancement, actual loop wraparound,
offscreen/tab pause, Save-Data and mobile behavior are not newly certified by this
review. Prior media tests are separate evidence, not substituted for these gaps.

## Reproduction and evidence

Run from the project root. No authentication is required:

```sh
python3 scripts/test-web-security.py --http
python3 scripts/test-web-security.py --browser
python3 scripts/test-web-security.py --frame
python3 scripts/prepare-pages.py --require-feed --expected-version 0.4.2
```

The HTTP run has 53 sequential fixed requests, three TLS checks, per-request
timeouts, response-size caps and a short inter-request delay. Executable reads
are capped at 5 MiB each; the feed at 16 KiB; ordinary public responses at 128 KiB.
It never follows redirects or uses an arbitrary caller-supplied host. Browser
navigation is restricted to the owned hosts; the main context has a 60-request
cap. HTTP mode needs `cryptography` in system Python; browser modes use the
already-installed Playwright virtual environment. No packages were installed.

The script emits sanitized response headers, sizes and hashes, not secret bodies.
HTTP header inventories are observations, not a blanket assertion that every
deployment policy is ideal. Failed browser checks exit nonzero and retain the
observations collected before failure. Current browser modes must not be cited
as passing until successfully rerun in a stable browser environment.

Additional minimal commands:

```sh
curl --silent --show-error --max-time 20 --head https://secblitz.lol/
curl --silent --show-error --max-time 20 --head https://beacons.lol/releases/stable.json
curl --silent --show-error --max-time 20 --head https://www.secblitz.lol/releases/stable.json
curl --silent --show-error --max-time 20 --head https://secblitz.lol/.env
curl --silent --show-error --max-time 20 --path-as-is --head 'https://secblitz.lol/%2e%2e/.git/config'
```

Do not add `--location` when checking updater compatibility. Do not replace HEAD
with GET for key/config-like probes.

Local raw bounded evidence, outside the webroot:

- `secblitz-security-http.json`: successful fixed HTTP run.
- `secblitz-security-extra.json`: live/local public-source matches,
  HTTP endpoint redirects, TLS 1.2 and conditional-request observations.
- `secblitz-security-browser.json`: successful initial/clipboard
  observations and explicitly failed playback from the final full browser run.
- `secblitz-security-frame.json`: start record of the isolated
  framing retry; browser launch failure is recorded in the command transcript.
- `secblitz-security-extra.py`: supplemental fixed public checks.

Local release-gate output:

```text
Signed feed and installer verified against pinned key.
HTML assets, installer links and displayed SHA-256 verified.
Static output validated: 18 files. No deployment performed.
```

Main's remaining verification work is limited to the recorded browser gaps and
any independently chosen hosting cleanup/cache adjustment. Any future function
or worker response must set its own content/security/cache headers consistently
with these public static routes; `_headers` alone is not proof of function
response policy. No function or default-cache source fix was made in this review.
