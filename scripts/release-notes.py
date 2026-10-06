#!/usr/bin/env python3
"""Print the CHANGELOG.md section of one version, for a release body.

    python3 scripts/release-notes.py 0.8.0 [--changelog CHANGELOG.md]

Fails (exit 1) when the version has no dated section or the section has no
entries, so a release cannot go out without notes. Standard library only.
"""
import argparse
import re
import sys
from pathlib import Path


def section(text, version):
    heads = list(re.finditer(r"^## \[([^\]]+)\](?: - (.*))?[ \t]*$", text, re.M))
    for i, h in enumerate(heads):
        if h.group(1) != version:
            continue
        label = (h.group(2) or "").strip()
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", label):
            raise ValueError(f"CHANGELOG.md section {version} is not dated (found '{label or 'no date'}'). Run scripts/bump-version.py {version} first.")
        end = heads[i + 1].start() if i + 1 < len(heads) else len(text)
        body = text[h.end():end].strip("\n")
        if not re.search(r"^- ", body, re.M):
            raise ValueError(f"CHANGELOG.md section {version} has no entries.")
        return body + "\n"
    raise ValueError(f"CHANGELOG.md has no section for {version}.")


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("version")
    ap.add_argument("--changelog", default=str(Path(__file__).resolve().parents[1] / "CHANGELOG.md"))
    args = ap.parse_args(argv)
    try:
        sys.stdout.write(section(Path(args.changelog).read_text(encoding="utf-8"), args.version))
        return 0
    except (OSError, ValueError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
