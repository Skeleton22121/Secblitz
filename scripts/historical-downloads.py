#!/usr/bin/env python3
"""Keep old download URLs alive on the website, with pinned hashes.

A Cloudflare Pages deployment replaces the whole site, so every deploy must
carry the older installers that stage-pages.py lists as HISTORICAL. They are not
in git (too big), so the publish workflow fetches them from the live site and
refuses any file whose SHA-256 is not in scripts/historical-downloads.sha256.

    historical-downloads.py check                 offline: manifest covers the list
    historical-downloads.py fetch --dest DIR      download and verify every listed file
    historical-downloads.py record 0.7.0          download one version's two files from
                                                  the live site and add their hashes
                                                  (run when a version becomes history,
                                                  review the diff, commit it)
    historical-downloads.py missing               print the versions that have no pinned hash yet
    historical-downloads.py record-missing --verified-dir DIR
                                                  the same for every listed file that has
                                                  no pinned hash yet (bump-version.yml)

Reads only. It never signs, deploys or uses credentials. Standard library only.
"""
import argparse
import hashlib
import importlib.util
import os
from pathlib import Path
import re
import sys
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = Path(__file__).with_name("historical-downloads.sha256")
LIMIT = 25 * 1024 * 1024
LINE = re.compile(r"([0-9a-f]{64})  (secblitz-[0-9.]+-windows-x64(?:-setup)?\.exe)")
VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", re.ASCII)


class HistoryError(Exception):
    pass


def load_stage():
    spec = importlib.util.spec_from_file_location("stage_pages", ROOT / "scripts/stage-pages.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def read_manifest(path=MANIFEST):
    hashes = {}
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        match = LINE.fullmatch(line)
        if not match:
            raise HistoryError(f"{path.name} line {number} is not '<sha256>  <file name>'.")
        if match[2] in hashes:
            raise HistoryError(f"{path.name} lists {match[2]} twice.")
        hashes[match[2]] = match[1]
    return hashes


def write_manifest(hashes, path=MANIFEST):
    text = "".join(f"{hashes[name]}  {name}\n" for name in sorted(hashes))
    path.write_text(text, encoding="utf-8")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def download(url):
    """One HTTPS GET, no redirects, size capped. Returns bytes."""
    if not url.startswith("https://"):
        raise HistoryError("Downloads must use HTTPS.")
    opener = urllib.request.build_opener(NoRedirect)
    request = urllib.request.Request(url, headers={"User-Agent": "secblitz-release-pipeline"})
    try:
        with opener.open(request, timeout=120) as response:
            if response.status != 200:
                raise HistoryError(f"{url} answered {response.status}.")
            data = response.read(LIMIT + 1)
    except (urllib.error.URLError, OSError) as err:
        raise HistoryError(f"Could not download {url}: {err}") from err
    if not 0 < len(data) <= LIMIT:
        raise HistoryError(f"{url} is empty or larger than 25 MiB.")
    return data


def origin():
    value = (ROOT / "assets/update-origin.txt").read_text(encoding="utf-8").strip()
    return load_stage().gate.origin_value(value)


def listed_names(stage):
    return list(stage.HISTORICAL)


def check(stage=None, hashes=None):
    stage = stage or load_stage()
    hashes = read_manifest() if hashes is None else hashes
    missing = [name for name in listed_names(stage) if name not in hashes]
    if missing:
        raise HistoryError("No pinned hash for: " + ", ".join(missing)
                           + ". Run: python3 scripts/historical-downloads.py record <version>")
    return hashes


def fetch(dest, base, fetcher=download, stage=None, hashes=None):
    stage = stage or load_stage()
    hashes = check(stage, hashes)
    dest.mkdir(parents=True, exist_ok=True)
    for name in listed_names(stage):
        data = fetcher(f"{base}/downloads/{name}")
        if hashlib.sha256(data).hexdigest() != hashes[name]:
            raise HistoryError(f"{name} from the live site does not match its pinned hash. Not deploying.")
        stage.check_content(name, data)
        target = dest / name
        if target.exists() or target.is_symlink():
            raise HistoryError(f"{name} already exists in the destination.")
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
    return len(listed_names(stage))


def record(version, base, fetcher=download, stage=None, path=MANIFEST, verified_dir=None):
    if not VERSION.fullmatch(version):
        raise HistoryError("Version must be plain X.Y.Z.")
    stage = stage or load_stage()
    hashes = read_manifest(path) if path.exists() else {}
    for name in (f"secblitz-{version}-windows-x64-setup.exe", f"secblitz-{version}-windows-x64.exe"):
        data = fetcher(f"{base}/downloads/{name}")
        stage.check_content(name, data)
        digest = hashlib.sha256(data).hexdigest()
        if verified_dir is not None:
            # Independent proof: the same file from the GitHub release, already
            # checked against its SHA256SUMS and build attestation by the workflow.
            reference = Path(verified_dir) / name
            if not reference.is_file() or reference.is_symlink():
                raise HistoryError(f"No verified copy of {name} to compare with.")
            if hashlib.sha256(reference.read_bytes()).hexdigest() != digest:
                raise HistoryError(f"{name} on the live site differs from the GitHub release. Not pinning it.")
        if hashes.get(name, digest) != digest:
            raise HistoryError(f"{name} is already pinned with a different hash. Refusing to change it.")
        hashes[name] = digest
    write_manifest(hashes, path)
    return hashes


def record_missing(base, fetcher=download, stage=None, path=MANIFEST, verified_dir=None):
    stage = stage or load_stage()
    hashes = read_manifest(path) if path.exists() else {}
    versions = []
    for name in listed_names(stage):
        if name not in hashes:
            version = re.fullmatch(r"secblitz-(.*)-windows-x64(?:-setup)?\.exe", name)[1]
            if version not in versions:
                versions.append(version)
    for version in versions:
        record(version, base, fetcher, stage, path, verified_dir)
    return versions


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("check")
    fetch_parser = sub.add_parser("fetch")
    fetch_parser.add_argument("--dest", type=Path, required=True)
    record_parser = sub.add_parser("record")
    record_parser.add_argument("version")
    missing_parser = sub.add_parser("record-missing")
    missing_parser.add_argument("--verified-dir", type=Path, required=True,
                                help="folder with the same files from the GitHub release (checked by the workflow); every live file must match")
    sub.add_parser("missing")
    args = parser.parse_args(argv)
    try:
        if args.command == "check":
            hashes = check()
            print(f"Pinned hashes cover every historical download ({len(hashes)} files).")
        elif args.command == "fetch":
            count = fetch(args.dest, origin())
            print(f"Fetched and verified {count} historical downloads.")
        elif args.command == "missing":
            stage = load_stage()
            hashes = read_manifest() if MANIFEST.exists() else {}
            seen = []
            for name in listed_names(stage):
                if name not in hashes:
                    version = re.fullmatch(r"secblitz-(.*)-windows-x64(?:-setup)?\.exe", name)[1]
                    if version not in seen:
                        seen.append(version)
                        print(version)
        elif args.command == "record-missing":
            versions = record_missing(origin(), verified_dir=args.verified_dir)
            print("Recorded hashes for: " + (", ".join(versions) or "nothing, all were pinned") + f". Review the diff of {MANIFEST.name}.")
        else:
            hashes = record(args.version, origin())  # by hand only; the workflow uses record-missing
            print(f"Recorded hashes for {args.version}. Review the diff of {MANIFEST.name}.")
    except (HistoryError, ValueError, OSError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
