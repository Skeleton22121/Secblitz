"""Control tests plus opt-in real decoding: scripts/test-video.py --real.

The hidden-document preference is injected because this harness keeps tabs visible.
Media methods, decoding and IntersectionObserver are not mocked in real tests.
Screenshots are written outside the site under /tmp/opencode.
"""

import argparse
import hashlib
from io import BytesIO
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread
from urllib.parse import urlsplit

from playwright.sync_api import sync_playwright, expect


ROOT = Path(__file__).resolve().parents[1] / "website"
VIDEO = "assets/intro-6bb434a9c067.mp4"
POSTER = "assets/preview-33b342ab21fb.webp"
MOCK = """
(() => {
  let paused = true;
  window.demoTest = { calls: 0, denied: false, hidden: false };
  Object.defineProperty(document, 'hidden', {get: () => demoTest.hidden});
  Object.defineProperty(navigator, 'connection', {value: new EventTarget()});
  navigator.connection.saveData = SAVE_DATA;
  Object.defineProperty(HTMLMediaElement.prototype, 'paused', {get: () => paused});
  HTMLMediaElement.prototype.play = function () {
    demoTest.calls++;
    if (demoTest.denied) return Promise.reject(new DOMException('Denied', 'NotAllowedError'));
    paused = false;
    this.dispatchEvent(new Event('play'));
    return Promise.resolve();
  };
  HTMLMediaElement.prototype.pause = function () {
    paused = true;
    this.dispatchEvent(new Event('pause'));
  };
  window.IntersectionObserver = class {
    constructor(callback) { demoTest.intersect = visible => callback([{isIntersecting: visible, intersectionRatio: visible ? 1 : 0}]); }
    observe() {}
  };
})();
"""


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def send_head(self):
        # SimpleHTTPRequestHandler lacks byte ranges needed for reliable seeking.
        if urlsplit(self.path).path == "/" + VIDEO and self.headers.get("Range"):
            data = (ROOT / VIDEO).read_bytes()
            start, end = self.headers["Range"].removeprefix("bytes=").split("-", 1)
            start = int(start or 0)
            end = min(int(end) if end else len(data) - 1, len(data) - 1)
            self.send_response(206)
            self.send_header("Content-Type", "video/mp4")
            self.send_header("Accept-Ranges", "bytes")
            self.send_header("Content-Range", f"bytes {start}-{end}/{len(data)}")
            self.send_header("Content-Length", str(end - start + 1))
            self.end_headers()
            return BytesIO(data[start:end + 1])
        return super().send_head()

    def end_headers(self):
        # Exercise the site's actual CSP locally, without claiming a deployed check.
        for line in (ROOT / "_headers").read_text().splitlines():
            if line.strip().startswith("Content-Security-Policy:"):
                self.send_header("Content-Security-Policy", line.split(":", 1)[1].strip())
        super().end_headers()


def marketing_page(page):
    expect(page.locator("h1")).to_have_text("A safer PC. Without headaches.")
    assert page.locator("figcaption, #video-caption, video[aria-describedby]").count() == 0
    expect(page.locator("video")).to_have_attribute("aria-label", "See Secblitz in action")
    expect(page.locator(".step-d")).to_have_text([
        "See what needs attention, without changing settings.",
        "Review recommended fixes or choose your own.",
        "Approve once. Secblitz applies your fixes and checks the results.",
        "Changed your mind? Review saved settings to restore.",
    ])
    expect(page.locator("#checks .section-head p")).to_have_text(
        "Recognizes protection Windows already provides, so you avoid unnecessary changes.")
    expect(page.locator(".check-grid li").last.locator("h3")).to_have_text("Ready before repairs")
    expect(page.locator(".check-grid li").last.locator("p")).to_have_text(
        "Check storage, power and whether Windows needs a restart before you begin repairs. These checks don't change settings.")
    for selector, attribute, expected in (
        ("video", "src", VIDEO), ("video", "poster", POSTER),
        ("#video-still", "src", POSTER),
        ('link[rel="stylesheet"]', "href", "styles.css?v=marketing"),
        ('meta[property="og:image"]', "content", "https://secblitz.lol/" + POSTER),
    ):
        expect(page.locator(selector)).to_have_attribute(attribute, expected)
    details = page.locator(".download-details")
    assert details.get_attribute("open") is None
    expect(page.locator("#sha")).to_be_hidden()
    for summary in page.locator("details summary").all():
        summary.click()
    text = page.locator("body").inner_text()
    for forbidden in ("\u2014", "test fixtures", "isolated Windows VM", "Ed25519", "0.4.0", "SMB", "DACL", "SAM"):
        assert forbidden not in text, forbidden
    expect(page.locator("#sha")).to_be_visible()
    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
    for summary in page.locator("details summary").all():
        summary.click()
    page.evaluate("scrollTo(0, 0)")


def real_video(playwright, url):
    asset = ROOT / VIDEO
    assert asset.stat().st_size > 0
    digest = hashlib.sha256(asset.read_bytes()).hexdigest()
    assert (ROOT / POSTER).stat().st_size > 0
    assert digest.startswith("6bb434a9c067")
    assert hashlib.sha256((ROOT / POSTER).read_bytes()).hexdigest().startswith("33b342ab21fb")
    output = Path("/tmp/opencode")
    assert output.is_dir()
    browser = playwright.chromium.launch()
    context = browser.new_context(viewport={"width": 1440, "height": 1000}, reduced_motion="reduce")
    page = context.new_page()
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.on("console", lambda message: errors.append(message.text) if message.type == "error" else None)
    page.goto(url)
    marketing_page(page)
    page.screenshot(path=str(output / "secblitz-marketing-desktop.png"), full_page=True)
    artifacts = {
        "secblitz-0.5.0-windows-x64-setup.exe": (3850954, "c17a543fccbb0ec1c37c487aeb8da2e7bfd8a832e04996f6ead32e6dd2b77f3b"),
        "secblitz-0.5.0-windows-x64.exe": (4851712, "036d8b69367cb7422ca5d6e821e73749f1c36ce35df437ac47b7d83ba829a6d9"),
    }
    for filename, (size, digest_expected) in artifacts.items():
        response = context.request.get(f"{url}/downloads/{filename}")
        assert response.status == 200
        assert len(response.body()) == size
        assert hashlib.sha256(response.body()).hexdigest() == digest_expected
    links = page.locator("a[download]").evaluate_all("links => links.map(a => a.getAttribute('href'))")
    assert links == ["downloads/secblitz-0.5.0-windows-x64-setup.exe"] * 2
    expect(page.locator("#sha")).to_have_text(artifacts["secblitz-0.5.0-windows-x64-setup.exe"][1])
    print("PASS: both 0.5.0 artifacts served byte-for-byte with expected sizes and SHA-256; installer links and displayed checksum match.")
    video = page.locator("#demo-video")
    assert page.locator("#recording button, #recording video[controls], .video-controls").count() == 0
    assert video.evaluate("v => v.defaultMuted && v.disablePictureInPicture && v.disableRemotePlayback && v.controlsList.contains('nofullscreen')")
    expect(page.locator("#recording")).to_be_visible()
    video.scroll_into_view_if_needed()
    metadata = video.evaluate("v => ({width: v.videoWidth, height: v.videoHeight, duration: v.duration, muted: v.muted, inline: v.playsInline, loop: v.loop})")
    assert metadata == {"width": 1280, "height": 720, "duration": 33, "muted": True, "inline": True, "loop": True}, metadata
    page.wait_for_timeout(500)
    assert video.evaluate("v => v.paused && v.currentTime === 0 && !v.autoplay")
    expect(page.locator("#video-still")).to_be_visible()
    assert page.evaluate("""async () => {
      const image = new Image(); image.src = document.querySelector('video').poster;
      await image.decode(); return image.naturalWidth > 0;
    }""")
    page.screenshot(path=str(output / "secblitz-real-poster-desktop.png"))

    page.emulate_media(reduced_motion="no-preference")
    page.wait_for_function("() => document.querySelector('video').currentTime > 0.3")
    expect(page.locator("#video-still")).to_be_hidden()
    page.emulate_media(reduced_motion="reduce")
    page.wait_for_function("() => document.querySelector('video').paused")
    expect(page.locator("#video-still")).to_be_visible()
    paused_at = video.evaluate("v => v.currentTime")
    page.wait_for_timeout(300)
    assert video.evaluate("v => v.currentTime") == paused_at

    # Pause only through the test API for decoded-frame inspection, not site UI.
    page.emulate_media(reduced_motion="no-preference")
    page.wait_for_function("() => !document.querySelector('video').paused")
    video.evaluate("v => v.pause()")
    # Seek actual media and wait for a decoded frame before capturing each image.
    frame_hashes = []
    for seconds in (5, 20):
        video.evaluate("""(v, seconds) => new Promise((resolve, reject) => {
          const timer = setTimeout(() => reject(new Error('Decoded seek frame timed out')), 10000);
          v.requestVideoFrameCallback(() => { clearTimeout(timer); resolve(); });
          v.currentTime = seconds;
        })""", seconds)
        assert abs(video.evaluate("v => v.currentTime") - seconds) < 0.1
        assert video.evaluate("v => v.readyState >= 2 && !v.error")
        pixels = video.evaluate("""v => {
          const canvas = document.createElement('canvas');
          canvas.width = v.videoWidth; canvas.height = v.videoHeight;
          canvas.getContext('2d').drawImage(v, 0, 0);
          return canvas.toDataURL();
        }""")
        frame_hashes.append(hashlib.sha256(pixels.encode()).hexdigest())
        video.screenshot(path=str(output / f"secblitz-real-frame-{seconds}s.png"))
    assert frame_hashes[0] != frame_hashes[1], "Decoded frame pixels must differ"
    assert video.evaluate("v => v.getVideoPlaybackQuality().totalVideoFrames") > 0

    # Real IntersectionObserver and media playback.
    page.locator("footer").scroll_into_view_if_needed()
    page.wait_for_function("() => document.querySelector('video').paused")
    video.scroll_into_view_if_needed()
    page.wait_for_function("() => !document.querySelector('video').paused")
    # This harness keeps background tabs visible. Inject only document visibility,
    # leaving pause(), play(), time advancement and decoding entirely native.
    page.evaluate("Object.defineProperty(document, 'hidden', {configurable: true, get: () => true}); document.dispatchEvent(new Event('visibilitychange'))")
    page.wait_for_function("() => document.hidden && document.querySelector('video').paused", polling=100)
    page.evaluate("delete document.hidden; document.dispatchEvent(new Event('visibilitychange'))")
    page.wait_for_function("() => !document.hidden && !document.querySelector('video').paused")

    # Fresh load without reduced motion must really autoplay and loop.
    page.emulate_media(reduced_motion="no-preference")
    page.reload()
    video.scroll_into_view_if_needed()
    page.wait_for_function("() => document.querySelector('video').currentTime > 0.3")
    video.evaluate("v => { v.currentTime = 32.8; }")
    page.wait_for_function("() => document.querySelector('video').currentTime < 2 && !document.querySelector('video').paused")
    page.screenshot(path=str(output / "secblitz-real-desktop.png"))
    assert not errors, errors
    context.close()

    # Emulated phone with real decoding, Save-Data preference only is injected.
    context = browser.new_context(viewport={"width": 390, "height": 844}, is_mobile=True, has_touch=True)
    page = context.new_page()
    page.add_init_script("window.saveData = true; Object.defineProperty(navigator.connection, 'saveData', {get: () => window.saveData})")
    page.goto(url)
    video = page.locator("#demo-video")
    expect(page.locator("#recording")).to_be_visible()
    video.scroll_into_view_if_needed()
    page.wait_for_timeout(400)
    assert video.evaluate("v => v.paused && v.currentTime === 0 && !v.autoplay")
    expect(page.locator("#video-still")).to_be_visible()
    page.evaluate("window.saveData = false; navigator.connection.dispatchEvent(new Event('change'))")
    page.wait_for_function("() => document.querySelector('video').currentTime > 0.3")
    assert video.evaluate("v => v.playsInline && !document.fullscreenElement && !v.webkitDisplayingFullscreen")
    expect(page.locator("#video-still")).to_be_hidden()
    for width in (320, 390, 768):
        page.set_viewport_size({"width": width, "height": 844})
        marketing_page(page)
        assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
        page.screenshot(path=str(output / f"secblitz-real-mobile-{width}.png"), full_page=True)
    context.close()
    browser.close()
    print(f"PASS: REAL H.264 decode, metadata {metadata}, distinct frames {frame_hashes}, SHA-256 {digest}")
    print("PASS: no controls, silent autoplay, loop, offscreen pause/resume, reduced-motion poster, mobile inline and no overflow. Injected preferences: hidden-document state and Save-Data; media playback stays native.")
    print(f"Screenshots: {output}/secblitz-real-*.png")


def run(real=False, staged=False):
    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(ROOT)))
    Thread(target=server.serve_forever, daemon=True).start()
    try:
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch()
            # Exercise the browser's real media-error handling for missing assets.
            page = browser.new_page()
            if staged:
                base = f"http://127.0.0.1:{server.server_port}"
                for missing in ("README.md", "test_video.py", "scripts/test-video.py", "assets/secblitz-demo.mp4", "assets/poster.webp"):
                    response = page.request.get(f"{base}/{missing}")
                    assert response.status == 404, missing
                page.goto(base + "/404.html")
                expect(page.locator("h1")).to_have_text("Page not found.")
                expect(page.get_by_role("link", name="Back to Secblitz")).to_have_attribute("href", "/")
                assert page.locator("script").count() == 0
                assert page.evaluate("Array.from(document.styleSheets).some(s => s.href && s.href.includes('/styles.css'))")
                print("PASS: staged source/legacy paths return local HTTP 404; branded 404 page loads with local CSS.")
            page.route("**/" + VIDEO + "*", lambda route: route.fulfill(status=404, body=""))
            page.goto(f"http://127.0.0.1:{server.server_port}")
            expect(page.locator("#recording")).to_be_hidden()
            assert page.locator("#term-out").count() == 0
            page.close()

            for reduced, save_data, denied in [(False, False, False), (True, False, False), (False, True, False), (False, False, True)]:
                page = browser.new_page(reduced_motion="reduce" if reduced else "no-preference")
                page.add_init_script(MOCK.replace("SAVE_DATA", str(save_data).lower()))
                # Suppress real media errors; inject metadata explicitly below.
                page.route("**/" + VIDEO + "*", lambda route: route.fulfill(status=200, content_type="video/mp4", body=b""))
                page.goto(f"http://127.0.0.1:{server.server_port}")
                page.evaluate("""denied => {
                  demoTest.denied = denied;
                  document.querySelector('video').dispatchEvent(new Event('loadedmetadata'));
                  demoTest.intersect(true);
                }""", denied)
                expect(page.locator("#recording")).to_be_visible()
                assert page.locator("#recording button, .video-controls, video[controls], #video-status").count() == 0
                page.wait_for_function("paused => document.querySelector('video').paused === paused", arg=reduced or save_data or denied)
                if reduced or save_data:
                    assert page.evaluate("demoTest.calls") == 0
                if reduced or save_data or denied:
                    expect(page.locator("#video-still")).to_be_visible()
                    calls = page.evaluate("demoTest.calls")
                    page.evaluate("demoTest.intersect(false); demoTest.intersect(true); document.dispatchEvent(new Event('visibilitychange'))")
                    assert page.evaluate("demoTest.calls") == calls
                else:
                    expect(page.locator("#video-still")).to_be_hidden()
                    page.evaluate("demoTest.intersect(false)")
                    assert page.evaluate("document.querySelector('video').paused")
                    page.evaluate("demoTest.intersect(true)")
                    page.wait_for_function("() => !document.querySelector('video').paused")
                    page.evaluate("demoTest.hidden = true; document.dispatchEvent(new Event('visibilitychange'))")
                    assert page.evaluate("document.querySelector('video').paused")
                    page.evaluate("demoTest.hidden = false; document.dispatchEvent(new Event('visibilitychange'))")
                    page.wait_for_function("() => !document.querySelector('video').paused")
                for width in (375, 1280):
                    page.set_viewport_size({"width": width, "height": 900})
                    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                page.evaluate("document.querySelector('video').dispatchEvent(new Event('error'))")
                expect(page.locator("#recording")).to_be_hidden()
                page.close()
            browser.close()
            print("PASS: missing-media state, no controls, mocked autoplay policy and quiet rejection.")
            if real:
                real_video(playwright, f"http://127.0.0.1:{server.server_port}")
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--real", action="store_true", help="Verify supplied assets with actual browser decoding and capture screenshots")
    parser.add_argument("--site", type=Path, help="Serve a staged directory, including exclusion/404 checks")
    args = parser.parse_args()
    if args.site:
        ROOT = args.site.resolve()
    run(args.real, staged=bool(args.site))
