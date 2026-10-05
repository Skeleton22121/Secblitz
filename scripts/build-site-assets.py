#!/usr/bin/env python3
"""Regenerate the website's shared design assets from the app's own sources.

Run: python3 scripts/build-site-assets.py [--check]

* Icons: the Fluent UI System Icons artwork in src/gui/icons.rs is injected
  into website/index.html between the `icons:start` / `icons:end` markers, so
  the site always uses the exact glyphs the app draws.
* Font: IBM Plex Sans (assets/fonts, SIL OFL) is subset to Latin and saved as
  WOFF2 under website/assets/fonts.
* Favicon: the app's window icon (white shield and tick on a #18181B tile).

--check exits non-zero when any generated file is out of date.
"""

import argparse
import io
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / "website"
ICONS_RS = ROOT / "src/gui/icons.rs"

# Site name -> (Icon variant, filled?)
ICONS = {
    "brand": ("BRAND", True),
    "shield-check": ("ShieldCheck", False),
    "scan": ("Scan", False),
    "wrench": ("Wrench", False),
    "sparkles": ("Sparkles", False),
    "toolbox": ("Toolbox", False),
    "history": ("History", False),
    "undo": ("Undo", False),
    "lock": ("Lock", False),
    "key": ("Key", False),
    "download": ("Download", False),
    "bug": ("Bug", False),
    "refresh": ("Refresh", False),
    "info": ("Info", False),
    "check": ("Check", False),
    "check-circle": ("CheckCircle", False),
    "alert": ("AlertTriangle", False),
    "chevron-down": ("ChevronDown", False),
    "globe": ("Globe", False),
    "bell": ("Bell", False),
    "apps": ("Apps", False),
    "password": ("Password", False),
    "hard-drive": ("HardDrive", False),
    "eye-off": ("EyeOff", False),
    "sun": ("Sun", False),
    "moon": ("Moon", False),
}

WEIGHTS = {"Regular": 400, "Medium": 500, "SemiBold": 600}
# Basic Latin, Latin-1, Latin Extended-A, general punctuation, arrows, a few symbols.
UNICODES = "U+0020-007E,U+00A0-00FF,U+0100-017F,U+2010-2027,U+2030-203A,U+20AC,U+2122,U+2190-2193,U+2212"


def icon_bodies():
    src = ICONS_RS.read_text(encoding="utf-8")
    brand = re.search(r'BRAND_SVG: &\[u8\] = s!\(r##"(.*?)"##\)', src, re.S)
    if not brand:
        sys.exit("brand mark not found in icons.rs")
    out = {}

    def section(name):
        m = re.search(rf"fn {name}\(self\) -> &'static str \{{\s*match self \{{(.*?)\n        \}}\n    \}}", src, re.S)
        if not m:
            sys.exit(f"fn {name} not found in icons.rs")
        return dict(re.findall(r'Icon::(\w+) => s!\(\s*r##"(.*?)"##', m[1], re.S))

    regular, filled = section("regular"), section("filled")
    for site_name, (variant, use_filled) in ICONS.items():
        if variant == "BRAND":
            body = brand[1]
        else:
            table = filled if use_filled else regular
            if variant not in table:
                sys.exit(f"icon {variant} not found in icons.rs")
            body = table[variant]
        out[site_name] = body.strip()
    return out


def sprite():
    lines = ['<svg class="sprite" width="0" height="0" aria-hidden="true">']
    for name, body in icon_bodies().items():
        lines.append(f'    <symbol id="i-{name}" viewBox="0 0 24 24">{body}</symbol>')
    lines.append("  </svg>")
    return "\n".join(lines)


def inject(html):
    new, count = re.subn(
        r"(<!-- icons:start[^>]*-->).*?(<!-- icons:end -->)",
        lambda m: m[1] + "\n  " + sprite() + "\n  " + m[2],
        html,
        flags=re.S,
    )
    if count != 1 or new.count("<symbol") != len(ICONS):
        sys.exit("icons markers missing or injection failed in index.html")
    return new


def fonts():
    from fontTools import subset
    from fontTools.ttLib import TTFont

    out = {}
    for style, weight in WEIGHTS.items():
        font = TTFont(ROOT / f"assets/fonts/IBMPlexSans-{style}.ttf")
        options = subset.Options()
        options.flavor = "woff2"
        options.layout_features = ["kern", "liga", "calt", "tnum", "case", "ccmp", "locl", "mark", "mkmk"]
        options.name_IDs = ["*"]
        options.notdef_outline = True
        sub = subset.Subsetter(options)
        sub.populate(unicodes=subset.parse_unicodes(UNICODES))
        sub.subset(font)
        buf = io.BytesIO()
        font.flavor = "woff2"
        font.save(buf)
        out[f"assets/fonts/ibm-plex-sans-{weight}.woff2"] = buf.getvalue()
    out["assets/fonts/IBMPlexSans-LICENSE.txt"] = (ROOT / "assets/fonts/IBMPlexSans-LICENSE.txt").read_bytes()
    return out


FAVICON = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <rect width="24" height="24" rx="5" fill="#18181B"/>
  <path fill="#FFFFFF" fill-rule="evenodd" d="M12 2.5 19.5 5.5V12L18 15.8 15 19 12 21.5 9 19 6 15.8 4.5 12V5.5Z M8.16 12.84 11 15.67 16.05 10.42 14.75 9.17 11 13.13 9.44 11.56Z"/>
</svg>
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    wanted = {"assets/favicon.svg": FAVICON.encode()}
    wanted.update(fonts())
    index = SITE / "index.html"
    wanted["index.html"] = inject(index.read_text(encoding="utf-8")).encode()

    stale = []
    for rel, data in wanted.items():
        path = SITE / rel
        current = path.read_bytes() if path.exists() else None
        # WOFF2 output is deterministic for a given fontTools version; compare
        # fonts by existence only so --check does not depend on that version.
        same = current is not None and (rel.endswith(".woff2") or current == data)
        if same:
            continue
        stale.append(rel)
        if not args.check:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    if args.check and stale:
        sys.exit("out of date: " + ", ".join(stale))
    print("up to date" if not stale else "wrote: " + ", ".join(stale))


if __name__ == "__main__":
    main()
