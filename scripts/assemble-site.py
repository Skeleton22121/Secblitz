#!/usr/bin/env python3
"""Build the exact website tree to deploy from a verified release. Deploys nothing.

    assemble-site.py --version 0.8.1 --assets release-assets --historical historical --output dist/pages

--assets holds the four release files (already checked against SHA256SUMS, the
build attestations and the pinned update key by publish-website.yml):

    secblitz-X.Y.Z-windows-x64-setup.exe, secblitz-X.Y.Z-windows-x64.exe, SHA256SUMS, stable.json

--historical holds the older downloads fetched by historical-downloads.py.
The portable exe is larger than the Pages file limit, so it stays on the GitHub
release and is not copied into the site.
Steps: copy website/ into a scratch tree, add the downloads and the signed
feed, write the real checksum and size into the page (finalize-site.py), run the
release gate (prepare-pages.py --require-feed) and then stage-pages.py, which
copies only allowlisted files. The result is --output, ready for wrangler.
Standard library only. Needs the Python package cryptography for the gate.
"""
import argparse
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", re.ASCII)


class AssembleError(Exception):
    pass


def regular_files(directory):
    found = {}
    for path in sorted(directory.iterdir()):
        if path.is_symlink() or not path.is_file():
            raise AssembleError(f"{directory.name}/{path.name} is not a regular file.")
        found[path.name] = path
    return found


def run(*args):
    result = subprocess.run([sys.executable, *map(str, args)], cwd=ROOT, capture_output=True, text=True)
    sys.stdout.write(result.stdout)
    if result.returncode:
        raise AssembleError(f"{Path(str(args[0])).name} failed: {(result.stderr or result.stdout).strip()}")


def assemble(version, assets, historical, output, website=None):
    if not VERSION.fullmatch(version):
        raise AssembleError("Version must be plain X.Y.Z.")
    website = website or ROOT / "website"
    setup = f"secblitz-{version}-windows-x64-setup.exe"
    portable = f"secblitz-{version}-windows-x64.exe"
    release = regular_files(assets)
    if set(release) != {setup, portable, "SHA256SUMS", "stable.json"}:
        raise AssembleError("The release must carry exactly: " + ", ".join(sorted((setup, portable, "SHA256SUMS", "stable.json")))
                            + ". Found: " + ", ".join(sorted(release)) + ".")
    old = regular_files(historical)
    if set(old) & {setup, portable}:
        raise AssembleError("The historical downloads already contain this version.")
    with tempfile.TemporaryDirectory(prefix="assemble-site-") as scratch:
        scratch = Path(scratch)
        site, reference = scratch / "site", scratch / "reference"
        shutil.copytree(website, site, symlinks=False)
        (site / "downloads").mkdir(exist_ok=True)
        for path in old.values():
            shutil.copyfile(path, site / "downloads" / path.name)
        shutil.copyfile(release[setup], site / "downloads" / setup)
        (site / "releases").mkdir(exist_ok=True)
        shutil.copyfile(release["stable.json"], site / "releases" / "stable.json")
        reference.mkdir()
        shutil.copyfile(release[setup], reference / setup)
        run(ROOT / "scripts/finalize-site.py", "--site", site, "--version", version, "--setup", release[setup])
        run(ROOT / "scripts/prepare-pages.py", "--site", site, "--require-feed", "--expected-version", version)
        if output.exists():
            raise AssembleError("The output directory must not exist yet.")
        run(ROOT / "scripts/stage-pages.py", "--source", site, "--output", output,
            "--reference-dir", reference, "--expected-version", version)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--version", required=True)
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--historical", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        assemble(args.version, args.assets, args.historical, args.output.absolute())
    except (AssembleError, OSError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    print(f"Site for {args.version} staged in {args.output}. Nothing was deployed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
