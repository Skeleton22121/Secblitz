#!/usr/bin/env python3
"""Regenerate the website's shared design assets from the app's own sources.

Run: python3 scripts/build-site-assets.py [--check]

* Icons: the Fluent UI System Icons artwork in src/gui/icons.rs is injected
  into website/index.html between the `icons:start` / `icons:end` markers, so
  the site always uses the exact glyphs the app draws.
* Font: IBM Plex Sans (assets/fonts, SIL OFL) is subset to Latin and saved as
  WOFF2 under website/assets/fonts.
* Favicon: the app's small-size icon (white shield and bolt on a #18181B tile),
  from assets/secblitz-small.svg.

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

SITE_ICONS = {
    "github": ("0 0 16 16", '<path d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z" fill="currentColor"/>'),
}

WEIGHTS = {"Regular": 400, "Medium": 500, "SemiBold": 600}
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
    for name, (view_box, body) in SITE_ICONS.items():
        lines.append(f'    <symbol id="i-{name}" viewBox="{view_box}">{body}</symbol>')
    lines.append("  </svg>")
    return "\n".join(lines)


def inject(html):
    new, count = re.subn(
        r"(<!-- icons:start[^>]*-->).*?(<!-- icons:end -->)",
        lambda m: m[1] + "\n  " + sprite() + "\n  " + m[2],
        html,
        flags=re.S,
    )
    if count != 1 or new.count("<symbol") != len(ICONS) + len(SITE_ICONS):
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


SMALL_ICON = ROOT / "assets/secblitz-small.svg"


def favicon():
    """The app's small-size icon (solid shield, bolt cut out), minus its size and text."""
    svg = SMALL_ICON.read_text(encoding="utf-8")
    shapes = re.findall(r"^\s*(<(?:rect|path)\b[^>]*/>)$", svg, re.MULTILINE)
    if len(shapes) != 2:
        sys.exit(f"{SMALL_ICON}: expected one rect and one path")
    body = "".join(f"  {shape}\n" for shape in shapes)
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">\n{body}</svg>\n'


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    wanted = {"assets/favicon.svg": favicon().encode()}
    wanted.update(fonts())
    index = SITE / "index.html"
    wanted["index.html"] = inject(index.read_text(encoding="utf-8")).encode()

    stale = []
    for rel, data in wanted.items():
        path = SITE / rel
        current = path.read_bytes() if path.exists() else None
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
