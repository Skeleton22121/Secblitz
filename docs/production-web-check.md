# Production web check

## Domain migration handoff: secblitz.lol

The current source targets `https://secblitz.lol/` in the title, canonical and
Open Graph metadata. This domain patch has not been published or live-verified
by this task. Current downloads remain 0.4.1; 0.4.2 is awaiting its artifact
handoff. The video remains actual 0.3.1 footage with silent autoplay and no controls.

The [official Pages redirects documentation](https://developers.cloudflare.com/pages/configuration/redirects/)
marks domain-level redirects unsupported. No `_redirects` file or path-only
fallback rule was added. Main must configure these zone-level 301 redirects:

- Old-root-only: `(http.host eq "beacons.lol" and http.request.uri.path eq "/")`
  to static `https://secblitz.lol/`.
- WWW any path: `(http.host eq "www.secblitz.lol")` to dynamic
  `concat("https://secblitz.lol", http.request.uri.path)`.
- Preserve query strings on both. See [Single Redirects settings](https://developers.cloudflare.com/rules/url-forwarding/single-redirects/settings/).

Do not redirect `beacons.lol/releases/*` or `beacons.lol/downloads/*`; old clients
must receive the signed migration feed and installer directly from their compiled
origin. After publication, verify these old-host routes still return HTTP 200
without `Location`, with `no-store` on the feed, and verify the primary host has
no redirect loop. No Cloudflare configuration or publication was performed here.

## Historical live check: beacons.lol

The following evidence describes the original checked domain and deployment.
It is retained verbatim as historical evidence, not relabeled as a secblitz.lol test.

Checked at **2026-10-03 02:20:29 UTC** using Playwright Chromium against public
HTTPS endpoints. No account authentication or Cloudflare credentials were used.

- Production: https://beacons.lol/
- Deployment: https://6d060850.secblitz.pages.dev/
- Result: **runtime and artifact checks PASS; one factual copy issue fixed in source, awaiting republish.**

## Live browser results

| Check | Result |
| --- | --- |
| Production and deployment homepage | Both HTTP 200; identical HTML bytes |
| Deployed HTML, JavaScript and CSS | Matched local source before the copy correction below |
| Canonical | `https://beacons.lol/` |
| Desktop video | Real decoded 1280 x 720 video, 33 seconds; currentTime advanced automatically |
| Playback presentation | No site buttons, native controls or control bar; muted, defaultMuted, autoplay, loop and playsInline enabled |
| Frame verification | 455 decoded frames reported; distinct nonblack frames captured at 5, 20 and 26 seconds |
| Loop | Sought to 32.8 seconds; playback wrapped below 2 seconds and continued |
| Offscreen | Paused after scrolling to footer; resumed automatically on returning |
| Mobile Chromium emulation | 390 x 844 viewport, touch and mobile enabled; silent inline autoplay with no fullscreen takeover |
| Reduced motion | Fresh load stayed paused at time 0 with autoplay disabled; loaded poster and page body remained visible |
| Responsive overflow | None at 320 or 390 pixels, in both autoplay and reduced-motion contexts |
| Desktop browser errors | No JavaScript or console errors |

The video was paused and sought through the test API only to inspect decoded
frames. No playback methods, media responses or IntersectionObserver behavior
were mocked. Three canvas pixel hashes differed; nonblack pixel fractions were
30.4%, 35.7% and 35.3%. The desktop, mobile reduced-motion and 26-second captures
were also visually reviewed. The footage is the disclosed 0.3.1 VM recording.

This run does not claim physical-phone coverage or a native hidden-tab test.
The existing local visibility-event tests remain separate from this live run.
The native signed-update E2E is owned by the parallel updater validation task.

## Public downloads

The primary download was clicked in the actual browser, then saved outside the
repository as `/tmp/opencode/secblitz-0.4.0-windows-x64-setup.exe`.

| Artifact | Result | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| Installer | Browser-suggested filename `secblitz-0.4.0-windows-x64-setup.exe`; displayed page checksum matched | 3,810,746 | `8292b3a87bbc30130ffdcb80cfa2cf1fc0bfcb680dfd12ece3000a3fbfaec37d` |
| Portable | Public `/downloads/secblitz-0.4.0-windows-x64.exe` returned HTTP 200 | 4,690,944 | `40cf5fc714e6dcff15de2186832b37f15f8ec6e4d6bc581a641fd53dd9ad4e16` |

## Headers, feed and byte ranges

- Homepage CSP includes `media-src 'self'`. Actual decoding succeeded under the
  deployed CSP. Script, style, image and font sources remain same-origin.
- `/releases/stable.json`: HTTP 200, `Cache-Control: no-store`.
- Public feed bytes matched the staged feed exactly, SHA-256
  `81f509a8f5646ad268cfad11b081042d333b34ab3cf1122290e8d7878141b5e4`.
- The downloaded feed was verified against the pinned Ed25519 public key and
  the browser-downloaded installer. No private key was used:

```sh
python3 scripts/prepare-pages.py \
  --verify-feed /tmp/opencode/secblitz-live-stable.json \
  --installer-directory /tmp/opencode --expected-version 0.4.0
```

Result: `Signed feed and installer verified against pinned key.`

- MP4 request with `Range: bytes=0-1023`: HTTP 206, 1,024 response bytes,
  `Content-Range: bytes 0-1023/638692`. Actual browser seeks also succeeded.

## Factual issue and required republish

The deployed description promised **"Every change can be undone."** The hero
made a similar blanket claim, and the FAQ implied all later changes would be
detected. `docs/FEATURES.md`, lines 254 and 288-294, documents skipped/conflicted
restorations, side effects outside Undo, and non-atomic conflict detection.

Corrected only `website/index.html`:

1. Description now says recorded fixes are available for review and undo.
2. Hero now says original settings are recorded for review and undo.
3. Undo FAQ now describes attempted restoration, skipped detected conflicts,
   and the possibility that concurrent changes are not detected.

**Main task must republish `website/index.html` to make these corrections live.**
No deployment was performed in this check. The light design, playback logic,
release binaries, signed feed, headers and other website files were unchanged.
The original deployed HTML hash, before this source-only correction, was
`2674328010fc44392bbe1dbc2a588ec8d73851b6567fdb5c3202c2b8e8e8ba20`.

## Evidence outside the repository

- Script: `/tmp/opencode/check-secblitz-production.py`
- Structured results: `/tmp/opencode/secblitz-live-results.json`
- Desktop: `/tmp/opencode/secblitz-live-desktop.png`
- Decoded frames: `/tmp/opencode/secblitz-live-frame-5s.png`,
  `/tmp/opencode/secblitz-live-frame-20s.png`,
  `/tmp/opencode/secblitz-live-frame-26s.png`
- Mobile autoplay: `/tmp/opencode/secblitz-live-mobile-320-auto.png`,
  `/tmp/opencode/secblitz-live-mobile-390-auto.png`
- Mobile reduced motion: `/tmp/opencode/secblitz-live-mobile-320-reduced.png`,
  `/tmp/opencode/secblitz-live-mobile-390-reduced.png`

The script includes a source/live equality assertion. Until the corrected HTML
is republished, a rerun will intentionally fail that assertion.
