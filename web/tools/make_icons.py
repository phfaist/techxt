#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Render `web/icon.svg` into the PWA icon set and the social card (web/PLAN.md §8.6).

This is a **development aid**, like `tools/gen_symbols.py` and `fetch_fonts.py`: it is
run by hand when the mark changes, and its output under `web/public/` is committed, so
an ordinary build needs none of what is listed below.  Two runs over an unchanged
`icon.svg` produce byte-identical files.

Usage::

    python3 web/tools/make_icons.py
    python3 web/tools/make_icons.py --check   # regenerate into a temp dir and compare

What it needs
-------------

- **cairosvg** (`pip install cairosvg`) — and therefore a system **libcairo**, which is
  the one dependency here that is not pure Python.  It is what rasterises the SVG.
- **Pillow** — composes the social card and writes the PNGs.
- **fontTools** with **brotli** — not to touch a font, but to unpack
  `fonts/JuliaMono-Regular.woff2` back into a TrueType Pillow can load.  The card is
  set in the app's own default face, out of the app's own repository, so it needs no
  font installed on the machine and cannot silently fall back to something else.

If libcairo is genuinely unavailable, the honest fix is to install it — not to
re-draw the mark in Pillow primitives, which would leave two descriptions of the same
shape to keep in step.  `icon.svg` is the single source of truth: every raster below
is a rendering of it, and the two full-bleed variants are assembled from its own
`plate` and `mark` elements rather than from a copy of their geometry.

What it writes
--------------

===========================================  ============================================
`public/icon.svg`                            a copy of `icon.svg`; `index.html` links it
`public/icons/icon-192.png`                  the manifest's `any` icon
`public/icons/icon-512.png`                  the manifest's large `any` icon
`public/icons/icon-maskable-512.png`         `purpose: maskable`, mark inside the safe zone
`public/icons/apple-touch-icon.png`          180 px, full-bleed: iOS masks it itself
`public/og.png`                              1200×630 social card
===========================================  ============================================

The two square sizes keep the rounded plate and its transparent corners, which is what
a browser tab, a bookmark and Android's "any" slot all want.  The maskable variant and
the Apple one are full-bleed instead: whoever displays them applies their own mask, and
a transparent corner under that mask shows as a notch.
"""

from __future__ import annotations

import argparse
import copy
import filecmp
import io
import shutil
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

WEB = Path(__file__).resolve().parent.parent
ICON = WEB / "icon.svg"
PUBLIC = WEB / "public"
JULIAMONO = WEB / "fonts" / "JuliaMono-Regular.woff2"

SVG_NS = "http://www.w3.org/2000/svg"
ET.register_namespace("", SVG_NS)

#: A maskable icon is guaranteed to show only inside a circle of 80 % of the canvas
#: (the "safe zone"), so the mark is shrunk until its bounding box fits that circle:
#: the mark reaches 259 units from the centre of the 512 grid, the safe radius is 205,
#: and 205/259 rounds down to this.
MASKABLE_SCALE = 0.78

#: iOS crops apple-touch-icon.png with a squircle of roughly the plate's own radius,
#: so the mark is rendered at its drawn size and only the plate's corners are lost.
APPLE_SCALE = 1.0

CARD = (1200, 630)
PAPER = (251, 250, 248, 255)  # #fbfaf8, the light theme of index.html
INK = (22, 21, 26, 255)  # #16151a, its dark counterpart
TEAL = (46, 125, 111, 255)  # the plate colour of icon.svg
MUTED = (90, 96, 100, 255)
RULE = (223, 220, 214, 255)
ARROW = (154, 160, 163, 255)


# ------------------------------------------------------------------- the SVG variants


def load_icon() -> tuple[ET.Element, ET.Element, ET.Element]:
    """`(root, plate, mark)` of `icon.svg`, by the ids the file gives them."""
    root = ET.parse(ICON).getroot()
    plate = root.find(f"./{{{SVG_NS}}}rect[@id='plate']")
    mark = root.find(f"./{{{SVG_NS}}}g[@id='mark']")
    if plate is None or mark is None:
        raise SystemExit(f"{ICON}: expected a <rect id='plate'> and a <g id='mark'>")
    return root, plate, mark


def full_bleed(scale: float) -> bytes:
    """`icon.svg` with square corners and the mark scaled about the centre.

    Rebuilt from the source document's own elements rather than from a second copy of
    the geometry, so that moving a path in `icon.svg` moves it in every output.
    """
    root, plate, mark = load_icon()
    side = 512.0
    centre = side / 2
    out = ET.Element(
        f"{{{SVG_NS}}}svg",
        {"viewBox": root.get("viewBox", "0 0 512 512"), "width": "512", "height": "512"},
    )
    ET.SubElement(
        out,
        f"{{{SVG_NS}}}rect",
        {"x": "0", "y": "0", "width": "512", "height": "512", "fill": plate.get("fill", "#000")},
    )
    group = ET.SubElement(
        out,
        f"{{{SVG_NS}}}g",
        {"transform": f"translate({centre} {centre}) scale({scale}) translate({-centre} {-centre})"},
    )
    group.append(copy.deepcopy(mark))
    return ET.tostring(out, encoding="utf-8", xml_declaration=True)


# ---------------------------------------------------------------------- rasterising


def render(source: bytes | Path, size: int):
    """One PNG of `source` at ``size``×``size``, as a Pillow image."""
    import cairosvg
    from PIL import Image

    if isinstance(source, Path):
        png = cairosvg.svg2png(url=str(source), output_width=size, output_height=size)
    else:
        png = cairosvg.svg2png(bytestring=source, output_width=size, output_height=size)
    return Image.open(io.BytesIO(png)).convert("RGBA")


def juliamono(size: int):
    """A Pillow font from the committed woff2, unpacked to TrueType in memory."""
    from fontTools.ttLib import TTFont
    from PIL import ImageFont

    if not JULIAMONO.exists():
        raise SystemExit(f"{JULIAMONO}: missing — run web/tools/fetch_fonts.py")
    font = TTFont(JULIAMONO, recalcBBoxes=False, recalcTimestamp=False)
    font.flavor = None
    buffer = io.BytesIO()
    font.save(buffer)
    buffer.seek(0)
    return ImageFont.truetype(buffer, size)


def social_card():
    """The 1200×630 card: the mark, the name, what it does, and it doing it."""
    from PIL import Image, ImageDraw

    card = Image.new("RGBA", CARD, PAPER)
    draw = ImageDraw.Draw(card)

    icon = render(ICON, 168)
    card.alpha_composite(icon, (76, 74))

    name = juliamono(116)
    tagline = juliamono(44)
    sample = juliamono(38)
    footer = juliamono(28)

    draw.text((286, 66), "techxt", font=name, fill=INK)
    draw.text((292, 196), "LaTeX-like markup → plain text", font=tagline, fill=TEAL)

    draw.line((76, 300, 1124, 300), fill=RULE, width=3)

    # The product doing its job, which says more than another sentence about it would.
    # Each right-hand column is what `techxt` actually prints for the left-hand one.
    rows = (
        (r"$\sum_{i=1}^n x_i$", "∑ᵢ₌₁ⁿ 𝑥ᵢ"),
        (r"\emph{a fact}", "𝑎 𝑓𝑎𝑐𝑡"),
        (r'\"o \c{c} \v{s}', "ö ç š"),
    )
    top = 340
    for index, (latex, text) in enumerate(rows):
        y = top + index * 62
        draw.text((80, y), latex, font=sample, fill=MUTED)
        draw.text((572, y), "→", font=sample, fill=ARROW)
        draw.text((650, y), text, font=sample, fill=INK)

    strapline = "Paste LaTeX, read plain text. Runs entirely in your browser."
    draw.text((80, 548), strapline, font=footer, fill=MUTED)

    # Nothing here is laid out by a text engine, so a longer string would run off the
    # edge silently. Two runs of the same script must not disagree about that.
    for text, font, left in (
        ("techxt", name, 286),
        ("LaTeX-like markup → plain text", tagline, 292),
        (strapline, footer, 80),
    ):
        end = left + draw.textlength(text, font=font)
        if end > CARD[0] - 76:
            raise SystemExit(f"the card's {text[:24]!r} runs to {end:.0f} px, past the margin")
    return card


# --------------------------------------------------------------------------- writing


def write_all(destination: Path) -> list[Path]:
    icons = destination / "icons"
    icons.mkdir(parents=True, exist_ok=True)
    written = []

    copied = destination / "icon.svg"
    shutil.copyfile(ICON, copied)
    written.append(copied)

    for size in (192, 512):
        target = icons / f"icon-{size}.png"
        render(ICON, size).save(target, optimize=True)
        written.append(target)

    target = icons / "icon-maskable-512.png"
    render(full_bleed(MASKABLE_SCALE), 512).save(target, optimize=True)
    written.append(target)

    target = icons / "apple-touch-icon.png"
    render(full_bleed(APPLE_SCALE), 180).save(target, optimize=True)
    written.append(target)

    target = destination / "og.png"
    social_card().convert("RGB").save(target, optimize=True)
    written.append(target)

    return written


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--check",
        action="store_true",
        help="render into a temporary directory and compare, without writing",
    )
    args = parser.parse_args(argv)

    if not ICON.exists():
        raise SystemExit(f"{ICON}: missing")

    if not args.check:
        for path in write_all(PUBLIC):
            print(f"{path.relative_to(WEB)}: {path.stat().st_size} bytes")
        return 0

    stale = 0
    with tempfile.TemporaryDirectory(prefix="techxt-icons-") as scratch:
        for fresh in write_all(Path(scratch)):
            committed = PUBLIC / fresh.relative_to(scratch)
            if not committed.exists():
                print(f"{committed.relative_to(WEB)}: missing", file=sys.stderr)
                stale += 1
            elif not filecmp.cmp(fresh, committed, shallow=False):
                print(f"{committed.relative_to(WEB)}: OUT OF DATE", file=sys.stderr)
                stale += 1
            else:
                print(f"{committed.relative_to(WEB)}: up to date")
    return 1 if stale else 0


if __name__ == "__main__":
    sys.exit(main())
