#!/usr/bin/env python3
"""After a deploy, prove the live website serves exactly the bytes that were deployed.

    verify-live-site.py --site dist/pages --version 0.8.1 [--origin https://example.org ...]
        [--feed-origin https://legacy.example ...]

Fetches, from the live origin (default: the update origin compiled into the app,
assets/update-origin.txt), the signed feed (releases/stable.json), the download
page (/) and the setup, and compares the SHA-256 of each with
the staged file that was deployed. A fresh deploy can take a short while to
reach every edge, so a mismatch is retried (--attempts, --delay) and only then
fails, loudly, with exit code 1. A --feed-origin, such as a legacy update host
whose front page redirects to the main site, is checked only for the files
installed copies download from it: the feed and the setup.
Read only: no credentials, no writes.
Standard library only.
"""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import re
import sys
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
LIMIT = 25 * 1024 * 1024
VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", re.ASCII)


class LiveError(Exception):
    pass


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def fetch(url):
    if not url.startswith("https://"):
        raise LiveError("Live checks use HTTPS only.")
    request = urllib.request.Request(url, headers={"Cache-Control": "no-cache", "Pragma": "no-cache",
                                                   "User-Agent": "secblitz-release-pipeline"})
    try:
        with urllib.request.build_opener(NoRedirect).open(request, timeout=120) as response:
            if response.status != 200:
                raise LiveError(f"{url} answered {response.status}.")
            data = response.read(LIMIT + 1)
    except (urllib.error.URLError, OSError) as err:
        raise LiveError(f"Could not fetch {url}: {err}") from err
    if len(data) > LIMIT:
        raise LiveError(f"{url} is larger than 25 MiB.")
    return data


def expected_files(site, version):
    setup = f"secblitz-{version}-windows-x64-setup.exe"
    return {
        "/releases/stable.json": site / "releases/stable.json",
        "/": site / "index.html",
        f"/downloads/{setup}": site / "downloads" / setup,
    }


def check_origin(origin, files, fetcher, attempts, delay, sleep=time.sleep):
    problems = {}
    for path, local in files.items():
        want = hashlib.sha256(local.read_bytes()).hexdigest()
        for attempt in range(1, attempts + 1):
            try:
                got = hashlib.sha256(fetcher(origin + path)).hexdigest()
                error = None if got == want else f"served SHA-256 {got}, deployed {want}"
            except LiveError as err:
                error = str(err)
            if error is None:
                print(f"OK    {origin}{path}  {want}")
                problems.pop(path, None)
                break
            problems[path] = error
            if attempt < attempts:
                sleep(delay)
    return [f"{origin}{path}: {error}" for path, error in problems.items()]


def main(argv=None, fetcher=fetch, sleep=time.sleep):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--site", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--origin", action="append", default=[], help="live origin to check (repeatable)")
    parser.add_argument("--feed-origin", action="append", default=[], help="origin that serves only the feed and downloads (repeatable)")
    parser.add_argument("--attempts", type=int, default=30)
    parser.add_argument("--delay", type=float, default=10)
    args = parser.parse_args(argv)
    if not VERSION.fullmatch(args.version) or args.attempts < 1:
        parser.error("version must be plain X.Y.Z and attempts at least 1")
    spec = importlib.util.spec_from_file_location("prepare_pages", ROOT / "scripts/prepare-pages.py")
    gate = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gate)
    origins = [gate.origin_value(o) for o in args.origin]
    if not origins:
        origins = [gate.origin_value((ROOT / "assets/update-origin.txt").read_text(encoding="utf-8").strip())]
    files = expected_files(args.site, args.version)
    feed_files = {path: local for path, local in files.items() if path != "/"}
    failures = []
    for origin in origins:
        failures += check_origin(origin, files, fetcher, args.attempts, args.delay, sleep)
    for origin in (gate.origin_value(o) for o in args.feed_origin):
        failures += check_origin(origin, feed_files, fetcher, args.attempts, args.delay, sleep)
    if failures:
        print("LIVE SITE DOES NOT MATCH THE DEPLOYED FILES:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print(f"The live site serves exactly the deployed files for {args.version}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
