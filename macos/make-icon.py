#!/usr/bin/env python3
"""Draws macos/berth.icns: the rail's amber cube on a warm charcoal tile.

Run from the repository root:  python3 macos/make-icon.py
Needs Pillow and macOS's iconutil (both are present on a stock dev Mac).
"""
import os
import shutil
import subprocess
from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
S = 4096  # drawn large, then reduced; iconutil takes the 1024 master
BG = (27, 23, 18, 255)      # #1b1712, the dark page
AMBER = (227, 164, 75, 255)  # #e3a44b, the dark accent


def draw() -> Image.Image:
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    pad, radius = 256, 880
    d.rounded_rectangle((pad, pad, S - pad, S - pad), radius=radius, fill=BG)

    # The cube's hexagon and the three edges that meet at its near corner.
    c = S // 2
    top, bottom = 1000, 3096
    left, right = 1136, 2960
    mid_y = 2048
    near = (c, 2048)
    pts = [(c, top), (right, 1432), (right, mid_y + 400),
           (c, bottom), (left, mid_y + 400), (left, 1432)]
    w = 150
    d.line(pts + [pts[0]], fill=AMBER, width=w, joint="curve")
    for end in [(left, 1432), (right, 1432), (c, bottom)]:
        d.line([near, end], fill=AMBER, width=w)
    # Round the vertices so the stroke joins match the rail's glyph.
    r = w // 2
    for x, y in pts + [near]:
        d.ellipse((x - r, y - r, x + r, y + r), fill=AMBER)
    return im


def main() -> None:
    master = draw().resize((1024, 1024), Image.LANCZOS)
    iconset = os.path.join(HERE, "berth.iconset")
    os.makedirs(iconset, exist_ok=True)
    sizes = [16, 32, 128, 256, 512]
    for n in sizes:
        master.resize((n, n), Image.LANCZOS).save(os.path.join(iconset, f"icon_{n}x{n}.png"))
        master.resize((n * 2, n * 2), Image.LANCZOS).save(os.path.join(iconset, f"icon_{n}x{n}@2x.png"))
    subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(HERE, "berth.icns")], check=True)
    shutil.rmtree(iconset)
    print("wrote macos/berth.icns")


if __name__ == "__main__":
    main()
