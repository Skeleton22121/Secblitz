#!/usr/bin/env python3
"""Move every place that carries the Secblitz version to a new version.

    python3 scripts/bump-version.py 0.8.1 [--dry-run] [--site] [--date YYYY-MM-DD]

Updates Cargo.toml, the secblitz entry in Cargo.lock, assets/secblitz.rc (both
numeric and text versions), assets/secblitz.manifest, and moves the
"Unreleased" section of CHANGELOG.md into a new dated section. With --site it
also moves the download links and version text in README.md and
website/index.html (only do that when the new setup is really published).
With --site-only it changes just those two files, for a version that Cargo.toml
already has (the step after the release is published).

The new version must be greater than the current one. One exception: when
CHANGELOG.md has "## [X] - Unreleased" for the version already in Cargo.toml
(the version being developed), `bump-version.py X` only dates that section.

Nothing is written unless every file was processed without error, and
--dry-run writes nothing at all. Standard library only.
"""
import argparse
import datetime
import os
import re
import sys
import tempfile
from pathlib import Path

VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", re.ASCII)


class BumpError(Exception):
    pass


def parse(version):
    m = VERSION.fullmatch(version)
    if not m:
        raise BumpError(f"'{version}' is not a plain major.minor.patch version (for example 0.8.1).")
    return tuple(int(x) for x in m.groups())


def read(root, rel):
    path = root / rel
    try:
        return path.read_bytes().decode("utf-8")
    except OSError as err:
        raise BumpError(f"Cannot read {rel}: {err.strerror}") from err


def sub_once(text, pattern, repl, rel, flags=0):
    new, count = re.subn(pattern, repl, text, count=0, flags=flags)
    if count != 1:
        raise BumpError(f"{rel}: expected exactly one match for {pattern!r}, found {count}.")
    return new


def current_version(root):
    text = read(root, "Cargo.toml")
    m = re.search(r'^\[package\]\s*$(.*?)(?=^\[|\Z)', text, re.M | re.S)
    v = re.search(r'^version\s*=\s*"([^"]+)"', m.group(1), re.M) if m else None
    if not v:
        raise BumpError("Cargo.toml has no package version.")
    parse(v.group(1))
    return v.group(1)


def bump_cargo_toml(text, old, new):
    def fix(m):
        body, n = re.subn(r'^(version\s*=\s*)"' + re.escape(old) + '"', r'\g<1>"' + new + '"', m.group(2), count=1, flags=re.M)
        if n != 1:
            raise BumpError("Cargo.toml: package version line not found.")
        return m.group(1) + body
    out, n = re.subn(r'(^\[package\]\s*$)(.*?)(?=^\[|\Z)', fix, text, count=1, flags=re.M | re.S)
    if n != 1:
        raise BumpError("Cargo.toml: [package] section not found.")
    return out


def bump_cargo_lock(text, old, new):
    return sub_once(text, r'(\[\[package\]\]\r?\nname = "secblitz"\r?\nversion = ")' + re.escape(old) + '"',
                    r'\g<1>' + new + '"', "Cargo.lock")


def bump_rc(text, old, new):
    o = ",".join(old.split(".")) + ",0"
    n = ",".join(new.split(".")) + ",0"
    for key in ("FILEVERSION", "PRODUCTVERSION"):
        text = sub_once(text, r'^(' + key + r' )' + re.escape(o) + r'$', r'\g<1>' + n, "assets/secblitz.rc", re.M)
    for key in ("FileVersion", "ProductVersion"):
        text = sub_once(text, r'(VALUE "' + key + r'", ")' + re.escape(old) + '"', r'\g<1>' + new + '"', "assets/secblitz.rc")
    return text


def bump_manifest(text, old, new):
    return sub_once(text, r'(<assemblyIdentity version=")' + re.escape(old) + r'\.0"', r'\g<1>' + new + '.0"',
                    "assets/secblitz.manifest")


def site_version(readme):
    m = re.search(r"downloads/secblitz-(" + VERSION.pattern + r")-windows-x64-setup\.exe", readme)
    return m.group(1) if m else None


def bump_site_text(text, old, new, rel):
    """Download links, 'Version X' text and the version badge, nothing else."""
    n_total = 0
    for pat, rep in [
        (r"secblitz-" + re.escape(old) + r"-windows-x64", "secblitz-" + new + "-windows-x64"),
        (r"Version " + re.escape(old) + r"\b", "Version " + new),
        (r"version-" + re.escape(old) + r"-", "version-" + new + "-"),
    ]:
        text, n = re.subn(pat, rep, text)
        n_total += n
    if n_total == 0:
        raise BumpError(f"{rel}: no mention of version {old} found.")
    return text


HEADING = re.compile(r"^## \[([^\]]+)\](?: - (.*))?[ \t]*$", re.M)


def changelog_sections(text):
    """Return a list of (name, label, start, end) for each '## [..]' section."""
    heads = list(HEADING.finditer(text))
    out = []
    for i, h in enumerate(heads):
        end = heads[i + 1].start() if i + 1 < len(heads) else len(text)
        out.append((h.group(1), (h.group(2) or "").strip(), h.start(), end))
    return out


def bump_changelog(text, old, new, today, allow_empty, warnings):
    sections = changelog_sections(text)
    by_name = {s[0]: s for s in sections}
    if new in by_name:
        name, label, start, end = by_name[new]
        if label.lower() != "unreleased" or new != old:
            raise BumpError(f"CHANGELOG.md already has a section for {new}.")
        # Date the version that is already in Cargo.toml.
        body = text[start:end]
        if not re.search(r"^- ", body, re.M):
            raise BumpError(f"CHANGELOG.md section {new} has no entries.")
        return text[:start] + f"## [{new}] - {today}\n" + text[start:end].split("\n", 1)[1] + text[end:]
    if "Unreleased" not in by_name:
        raise BumpError("CHANGELOG.md has no '## [Unreleased]' section.")
    name, label, start, end = by_name["Unreleased"]
    body = text[start:end].split("\n", 1)[1] if "\n" in text[start:end] else ""
    if not re.search(r"^- ", body, re.M):
        if not allow_empty:
            raise BumpError("CHANGELOG.md: nothing under Unreleased. Write the release notes first (or pass --allow-empty).")
        warnings.append("CHANGELOG.md: the Unreleased section is empty; the new section has no entries.")
    pending = by_name.get(old)
    if pending and pending[1].lower() == "unreleased":
        warnings.append(f"CHANGELOG.md: version {old} is still marked Unreleased. Release it first with: bump-version.py {old}")
    body = body.strip("\n")
    section = f"## [Unreleased]\n\n## [{new}] - {today}\n" + ("\n" + body + "\n" if body else "") + "\n"
    return text[:start] + section + text[end:]


def plan(root, new, today, site=False, allow_empty=False, site_only=False):
    """Return ({relative path: new text}, [warnings], current version). Reads only."""
    new_t = parse(new)
    old = current_version(root)
    old_t = parse(old)
    changes, warnings = {}, []
    if site_only:
        if new != old:
            raise BumpError(f"--site-only needs the version already in Cargo.toml ({old}).")
        site = True
    changelog = read(root, "CHANGELOG.md")
    dating_only = new == old and any(
        s[0] == new and s[1].lower() == "unreleased" for s in changelog_sections(changelog))
    if new_t <= old_t and not dating_only and not site_only:
        raise BumpError(f"New version {new} must be greater than the current version {old}.")
    if not site_only:
        changes["CHANGELOG.md"] = bump_changelog(changelog, old, new, today, allow_empty, warnings)
    if not dating_only and not site_only:
        changes["Cargo.toml"] = bump_cargo_toml(read(root, "Cargo.toml"), old, new)
        changes["Cargo.lock"] = bump_cargo_lock(read(root, "Cargo.lock"), old, new)
        changes["assets/secblitz.rc"] = bump_rc(read(root, "assets/secblitz.rc"), old, new)
        changes["assets/secblitz.manifest"] = bump_manifest(read(root, "assets/secblitz.manifest"), old, new)
    if site:
        readme = read(root, "README.md")
        shown = site_version(readme)
        if not shown:
            raise BumpError("README.md has no setup download link to update.")
        if parse(shown) >= new_t:
            raise BumpError(f"The site already shows {shown}, which is not older than {new}.")
        changes["README.md"] = bump_site_text(readme, shown, new, "README.md")
        changes["website/index.html"] = bump_site_text(read(root, "website/index.html"), shown, new, "website/index.html")
        warnings.append("The installer size text (for example '8.2 MB') in README.md and website/index.html is not updated. Check it.")
    return {k: v for k, v in changes.items() if v != read(root, k)}, warnings, old


def write_all(root, changes):
    """Write every file through a temp file in the same folder, then rename."""
    staged = []
    try:
        for rel, text in changes.items():
            path = root / rel
            fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=".bump-", suffix=".tmp")
            with os.fdopen(fd, "wb") as handle:
                handle.write(text.encode("utf-8"))
            staged.append((tmp, path))
        for tmp, path in staged:
            os.replace(tmp, path)
        staged = []
    finally:
        for tmp, _ in staged:
            try:
                os.unlink(tmp)
            except OSError:
                pass


def main(argv=None):
    ap = argparse.ArgumentParser(description="Bump the Secblitz version everywhere.")
    ap.add_argument("version", help="new version, for example 0.8.1")
    ap.add_argument("--dry-run", action="store_true", help="show what would change, write nothing")
    ap.add_argument("--site", action="store_true", help="also update README.md and website/index.html download links")
    ap.add_argument("--site-only", action="store_true",
                    help="only move the README and website download links to the version already in Cargo.toml")
    ap.add_argument("--allow-empty", action="store_true", help="allow an empty Unreleased section")
    ap.add_argument("--date", help="release date YYYY-MM-DD (default: today, UTC)")
    ap.add_argument("--root", default=str(Path(__file__).resolve().parents[1]), help=argparse.SUPPRESS)
    args = ap.parse_args(argv)
    today = args.date or datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d")
    try:
        datetime.date.fromisoformat(today)
        root = Path(args.root)
        changes, warnings, old = plan(root, args.version, today, args.site, args.allow_empty, args.site_only)
        print(f"Secblitz {old} -> {args.version} ({today})")
        for rel in sorted(changes):
            print(f"  update {rel}")
        for w in warnings:
            print(f"  warning: {w}")
        if args.dry_run:
            print("Dry run: nothing was written.")
            return 0
        write_all(root, changes)
        print("Done. Review with git diff, then open the release pull request.")
        return 0
    except (BumpError, ValueError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
