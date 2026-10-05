#!/usr/bin/env python3
"""Render the deliberately small SVG vocabulary with Pillow; no downloads.

Run: python assets/generate-icon.py [--check]
Pillow 12.1.1 is the reference generator. Each ICO frame is independently
supersampled, including the tiny taskbar sizes (not scaled from a 256px PNG).
"""

import argparse
import io
from pathlib import Path
import re
import struct
import xml.etree.ElementTree as ET

from PIL import Image, ImageChops, ImageDraw

ROOT = Path(__file__).resolve().parent
SIZES = (16, 20, 24, 32, 40, 48, 64, 128, 256)


def contours(data):
    tokens = iter(re.findall(r"[MLQZ]|-?\d+(?:\.\d+)?", data))
    points = []
    for token in tokens:
        if token in ("M", "L"):
            points.append((float(next(tokens)), float(next(tokens))))
        elif token == "Q":
            start = points[-1]
            control = (float(next(tokens)), float(next(tokens)))
            end = (float(next(tokens)), float(next(tokens)))
            for step in range(1, 65):
                t = step / 64
                points.append(tuple((1 - t) ** 2 * start[i]
                                    + 2 * (1 - t) * t * control[i]
                                    + t ** 2 * end[i] for i in (0, 1)))
        elif token == "Z":
            yield points
            points = []
        else:
            raise ValueError(f"Unsupported SVG path token: {token}")
    if points:
        raise ValueError("Unclosed SVG contour")


def render(size):
    svg = ET.parse(ROOT / "secblitz.svg").getroot()
    scale = size * 8 / 256
    canvas = Image.new("RGBA", (size * 8, size * 8))
    for element in svg:
        kind = element.tag.rsplit("}", 1)[-1]
        if kind in ("title", "desc"):
            continue
        if kind == "rect":
            ImageDraw.Draw(canvas).rounded_rectangle(
                (0, 0, canvas.width - 1, canvas.height - 1),
                radius=float(element.attrib["rx"]) * scale,
                fill=element.attrib["fill"],
            )
        elif kind == "path" and element.attrib["fill-rule"] == "evenodd":
            mask = Image.new("1", canvas.size)
            for contour in contours(element.attrib["d"]):
                part = Image.new("1", canvas.size)
                ImageDraw.Draw(part).polygon(
                    [(x * scale, y * scale) for x, y in contour], fill=1
                )
                mask = ImageChops.logical_xor(mask, part)
            canvas.paste(element.attrib["fill"], (0, 0), mask.convert("L"))
        else:
            raise ValueError(f"Unsupported SVG element: {kind}")
    return canvas.resize((size, size), Image.Resampling.LANCZOS)


def generate():
    frames = []
    for size in SIZES:
        output = io.BytesIO()
        render(size).save(output, format="PNG", optimize=False)
        frames.append(output.getvalue())
    offset = 6 + 16 * len(SIZES)
    result = struct.pack("<HHH", 0, 1, len(SIZES))
    for size, frame in zip(SIZES, frames):
        result += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0,
                              1, 32, len(frame), offset)
        offset += len(frame)
    return result + b"".join(frames)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    data = generate()
    destination = ROOT / "secblitz.ico"
    if args.check:
        if not destination.exists() or destination.read_bytes() != data:
            raise SystemExit("secblitz.ico is stale; regenerate with Pillow 12.1.1")
    else:
        destination.write_bytes(data)
    with Image.open(io.BytesIO(data)) as icon:
        assert icon.ico.sizes() == {(size, size) for size in SIZES}
        for size in SIZES:
            assert icon.ico.getimage((size, size)).mode == "RGBA"
    print("Verified ICO frames: " + ", ".join(map(str, SIZES)))


if __name__ == "__main__":
    main()
