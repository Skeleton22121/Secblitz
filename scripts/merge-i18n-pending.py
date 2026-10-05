#!/usr/bin/env python3
"""Merge i18n-pending/*.tsv into the six-locale catalog in src/i18n.rs.

Run: python3 scripts/merge-i18n-pending.py [--dry-run]

Rows are appended to MAINTENANCE_TEXT (en, es, fr, de, pt, it). A row is
skipped when its English key already exists in either catalog table, and
rejected (reported, not merged) when it is malformed: wrong column count, an
empty cell, an em dash in a translation, or placeholders that differ from the
English. Merged files are deleted; files with rejected rows are kept with only
the rejected rows, so they can be fixed and merged again.
"""

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
I18N = ROOT / "src/i18n.rs"
PENDING = ROOT / "i18n-pending"
KEY = re.compile(r'^\s*\["((?:[^"\\]|\\.)*)"')


def unescape(s):
    return s.replace('\\"', '"').replace("\\\\", "\\")


def escape(s):
    return s.replace("\\", "\\\\").replace('"', '\\"')


def placeholders(s):
    return sorted(re.findall(r"\{[^}]*\}", s))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    src = I18N.read_text(encoding="utf-8")
    lines = src.split("\n")
    existing = {unescape(m[1]) for line in lines if (m := KEY.match(line))}

    start = next(i for i, l in enumerate(lines) if l.startswith("const MAINTENANCE_TEXT"))
    end = next(i for i in range(start, len(lines)) if lines[i] == "];")

    added, rows, problems = set(), [], {}
    for tsv in sorted(PENDING.glob("*.tsv")):
        bad = []
        for n, raw in enumerate(tsv.read_text(encoding="utf-8").splitlines(), 1):
            if not raw.strip():
                continue
            cells = raw.split("\t")
            why = None
            if len(cells) != 6:
                why = f"{len(cells)} columns"
            elif any(not c.strip() for c in cells):
                why = "empty cell"
            elif any("—" in c for c in cells[1:]):
                why = "em dash in a translation"
            elif any(placeholders(c) != placeholders(cells[0]) for c in cells[1:]):
                why = "placeholders differ"
            if why:
                bad.append((n, why, raw))
                continue
            key = cells[0]
            if key in existing or key in added:
                continue
            added.add(key)
            rows.append("    [" + ", ".join(f'"{escape(c)}"' for c in cells) + "],")
        if bad:
            problems[tsv] = bad

    for tsv, bad in problems.items():
        for n, why, raw in bad:
            print(f"rejected {tsv.name}:{n}: {why}: {raw[:90]}", file=sys.stderr)

    print(f"{len(rows)} new rows, {sum(len(b) for b in problems.values())} rejected")
    if args.dry_run:
        return
    if rows:
        lines[end:end] = ["    // Merged from i18n-pending (checks, explanations, GUI follow-ups)."] + rows
        I18N.write_text("\n".join(lines), encoding="utf-8")
    for tsv in PENDING.glob("*.tsv"):
        if tsv in problems:
            tsv.write_text("\n".join(raw for _, _, raw in problems[tsv]) + "\n", encoding="utf-8")
        else:
            tsv.unlink()


if __name__ == "__main__":
    main()
