#!/usr/bin/env python3
"""Write the released installer's checksum and size into a staged copy of the site.

    finalize-site.py --site build/site --version 0.8.1 --setup assets/secblitz-0.8.1-windows-x64-setup.exe \\
        --setup-arm64 assets/secblitz-0.8.1-windows-arm64-setup.exe

website/index.html in git cannot hold the checksum: the installer is built after
the version bump, so the page carries a placeholder. At publish time, after the
installer has passed its checksum and attestation checks, this copies the real
SHA-256 and size into the staged copy (never into git). prepare-pages.py then
proves the page, the installer and the signed feed all agree.

Standard library only. Writes only inside --site.
"""
import argparse
import hashlib
from pathlib import Path
import re
import sys

VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", re.ASCII)
SHA_ELEMENT = re.compile(r'(<code id="sha">)[0-9a-f]{64}(</code>)')
SHA_ARM_ELEMENT = re.compile(r'(<code id="sha-arm64">)[0-9a-f]{64}(</code>)')
DETAILS = re.compile(r"(<p>Version )(\d+\.\d+\.\d+)( · )[0-9.]+( MB installer</p>)")
ARM_DETAILS = re.compile(r"(<p>ARM version · )[0-9.]+( MB installer</p>)")
LIMIT = 25 * 1024 * 1024


class FinalizeError(Exception):
    pass


def finalize_html(html, version, digest, size, arm_digest=None, arm_size=None):
    html, count = SHA_ELEMENT.subn(lambda m: m[1] + digest + m[2], html)
    if count != 1:
        raise FinalizeError(f"index.html: expected exactly one checksum element, found {count}.")
    megabytes = f"{size / 1_000_000:.1f}"

    def details(match):
        if match[2] != version:
            raise FinalizeError(f"index.html shows version {match[2]}, expected {version}.")
        return match[1] + version + match[3] + megabytes + match[4]

    html, count = DETAILS.subn(details, html)
    if count != 1:
        raise FinalizeError(f"index.html: expected exactly one 'Version X · N MB installer' line, found {count}.")
    if arm_digest is not None:
        html, count = SHA_ARM_ELEMENT.subn(lambda m: m[1] + arm_digest + m[2], html)
        if count != 1:
            raise FinalizeError(f"index.html: expected exactly one ARM checksum element, found {count}.")
        html, count = ARM_DETAILS.subn(lambda m: m[1] + f"{arm_size / 1_000_000:.1f}" + m[2], html)
        if count != 1:
            raise FinalizeError(f"index.html: expected exactly one 'ARM version · N MB installer' line, found {count}.")
    return html


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--site", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--setup", type=Path, required=True)
    parser.add_argument("--setup-arm64", type=Path)
    args = parser.parse_args(argv)
    try:
        if not VERSION.fullmatch(args.version):
            raise FinalizeError("Version must be plain X.Y.Z.")
        expected = f"secblitz-{args.version}-windows-x64-setup.exe"
        if args.setup.name != expected or args.setup.is_symlink() or not args.setup.is_file():
            raise FinalizeError(f"--setup must be a regular file named {expected}.")
        data = args.setup.read_bytes()
        if not 0 < len(data) <= LIMIT:
            raise FinalizeError("The installer is empty or larger than 25 MiB.")
        arm = None
        if args.setup_arm64:
            expected_arm = f"secblitz-{args.version}-windows-arm64-setup.exe"
            if args.setup_arm64.name != expected_arm or args.setup_arm64.is_symlink() or not args.setup_arm64.is_file():
                raise FinalizeError(f"--setup-arm64 must be a regular file named {expected_arm}.")
            arm = args.setup_arm64.read_bytes()
            if not 0 < len(arm) <= LIMIT:
                raise FinalizeError("The ARM installer is empty or larger than 25 MiB.")
        page = args.site / "index.html"
        if page.is_symlink() or not page.is_file():
            raise FinalizeError("The site has no index.html.")
        html = finalize_html(page.read_bytes().decode("utf-8"), args.version, hashlib.sha256(data).hexdigest(), len(data),
                         hashlib.sha256(arm).hexdigest() if arm is not None else None, len(arm) if arm is not None else None)
        page.write_bytes(html.encode("utf-8"))
        print(f"index.html now shows {args.version}, {len(data) / 1_000_000:.1f} MB, SHA-256 {hashlib.sha256(data).hexdigest()}.")
    except (FinalizeError, OSError, UnicodeError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
