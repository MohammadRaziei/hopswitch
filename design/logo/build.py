#!/usr/bin/env python3
"""Builds everything derived from the three hand-drawn logo sources.

Sources (edit these by hand):
    icon.svg            app icon, for 48 px and up
    icon-small.svg      the same idea reduced to a switch, for 16-32 px
    tray-template.svg   black + transparency, for the macOS menu bar

Generated (never edit):
    logo-light.svg / logo-dark.svg   icon + "Hopswitch" (text converted to paths,
                                     so no font is needed to display them)
    export/                          PNG / ICO files named like `tauri icon` output,
                                     plus tray icons and a GitHub social preview

Requirements: pip install novasvg pillow fonttools
Fonts: Poppins Bold / Regular (OFL); set POPPINS_DIR if they are not in the default place.
Run from anywhere: python design/logo/build.py
"""

import io
import os
import struct
from pathlib import Path

import novasvg
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from PIL import Image

HERE = Path(__file__).resolve().parent
OUT = HERE / "export"
FONTS = Path(os.environ.get("POPPINS_DIR", "/usr/share/fonts/truetype/google-fonts"))

# Theme colours from the UI mockups (design/preview): dark first, emerald accent.
INK_ON_LIGHT, GREEN_ON_LIGHT = "#0F1115", "#059669"
TEXT_ON_DARK, GREEN_ON_DARK = "#E8EAF0", "#34D399"


def inner(svg_path: Path) -> str:
    """The drawing inside an <svg> element, without the wrapper."""
    text = svg_path.read_text()
    start = text.index(">", text.index("<svg")) + 1
    return text[start : text.rindex("</svg>")]


def text_path(text: str, font_file: str, size: float, x: float, baseline: float):
    """`text` as one SVG path `d` string, and its advance width. No kerning."""
    font = TTFont(FONTS / font_file)
    glyphs, cmap = font.getGlyphSet(), font.getBestCmap()
    scale = size / font["head"].unitsPerEm
    pen = SVGPathPen(glyphs)
    cursor = 0.0
    for ch in text:
        name = cmap[ord(ch)]
        # font units are y-up; SVG is y-down
        transform = (scale, 0, 0, -scale, x + cursor * scale, baseline)
        glyphs[name].draw(TransformPen(pen, transform))
        cursor += glyphs[name].width
    return pen.getCommands(), cursor * scale


def cap_height(font_file: str) -> float:
    font = TTFont(FONTS / font_file)
    return font["OS/2"].sCapHeight / font["head"].unitsPerEm


def lockup(dark: bool) -> str:
    """Icon on the left, two-tone wordmark on the right (height 160)."""
    main, accent = (TEXT_ON_DARK, GREEN_ON_DARK) if dark else (INK_ON_LIGHT, GREEN_ON_LIGHT)
    size, icon_box, gap = 112, 160, 20
    baseline = icon_box / 2 + cap_height("Poppins-Bold.ttf") * size / 2
    x = icon_box + gap
    d_hop, w_hop = text_path("hop", "Poppins-Bold.ttf", size, x, baseline)
    d_switch, w_switch = text_path("switch", "Poppins-Bold.ttf", size, x + w_hop, baseline)
    width = round(x + w_hop + w_switch + 8)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {icon_box}" width="{width}" height="{icon_box}">
  <title>Hopswitch</title>
  <g transform="scale({icon_box / 1024})">{inner(HERE / "icon.svg")}</g>
  <path d="{d_hop}" fill="{main}"/>
  <path d="{d_switch}" fill="{accent}"/>
</svg>
"""


def png(svg: Path | str, size: int) -> bytes:
    buffer = OUT / "_tmp.png"
    novasvg.svg2png(str(svg) if isinstance(svg, Path) else svg, buffer, width=size, height=size)
    data = buffer.read_bytes()
    buffer.unlink()
    return data


def write_ico(path: Path, entries: list[tuple[int, bytes]]) -> None:
    """A multi-size .ico with PNG-encoded images (one PNG per size)."""
    head = struct.pack("<HHH", 0, 1, len(entries))
    offset = 6 + 16 * len(entries)
    table, blobs = b"", b""
    for size, data in entries:
        dim = 0 if size >= 256 else size
        table += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset + len(blobs))
        blobs += data
    path.write_bytes(head + table + blobs)


def social_preview() -> bytes:
    """1280x640 GitHub social preview: dark card, lockup and tagline."""
    lock = lockup(dark=True)
    lock_w = int(lock.split('viewBox="0 0 ')[1].split()[0])
    total_w = 900  # the lockup is scaled to this width, whatever the text measures
    scale = total_w / lock_w
    x0 = (1280 - total_w) / 2
    tagline, tag_w = text_path(
        "Switch your system proxy and DNS in one click.", "Poppins-Regular.ttf", 34, 0, 0
    )
    tag_x = (1280 - tag_w) / 2
    svg = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1280 640" width="1280" height="640">
  <rect width="1280" height="640" fill="#0F1115"/>
  <g transform="translate({x0} 190) scale({scale})">{lock[lock.index('>') + 1 : lock.rindex('</svg>')]}</g>
  <g transform="translate({tag_x} 470)"><path d="{tagline}" fill="#9AA3B2"/></g>
</svg>"""
    buffer = OUT / "_tmp.png"
    novasvg.svg2png(svg, buffer, width=1280, height=640)
    data = buffer.read_bytes()
    buffer.unlink()
    return data


def main() -> None:
    OUT.mkdir(exist_ok=True)
    (HERE / "logo-light.svg").write_text(lockup(dark=False))
    (HERE / "logo-dark.svg").write_text(lockup(dark=True))

    icon, small, tray = HERE / "icon.svg", HERE / "icon-small.svg", HERE / "tray-template.svg"

    # Same names and sizes as the output of `tauri icon`; the 32 px one uses the small variant.
    (OUT / "icon-1024.png").write_bytes(png(icon, 1024))
    (OUT / "icon.png").write_bytes(png(icon, 512))
    (OUT / "128x128@2x.png").write_bytes(png(icon, 256))
    (OUT / "128x128.png").write_bytes(png(icon, 128))
    (OUT / "32x32.png").write_bytes(png(small, 32))

    # Windows: 16-32 px layers use the small variant, 48 px and up the full icon.
    layers = [(s, png(small, s)) for s in (16, 24, 32)] + [(s, png(icon, s)) for s in (48, 64, 128, 256)]
    write_ico(OUT / "icon.ico", layers)

    # Tray: Windows notification area (16 / 32) and macOS template image (22 / 44, black + alpha).
    (OUT / "tray-16.png").write_bytes(png(small, 16))
    (OUT / "tray-32.png").write_bytes(png(small, 32))
    (OUT / "trayTemplate.png").write_bytes(png(tray, 22))
    (OUT / "trayTemplate@2x.png").write_bytes(png(tray, 44))

    (OUT / "social-preview.png").write_bytes(social_preview())

    # sanity: every PNG must be square RGBA with real transparency where expected
    for name in ("icon.png", "32x32.png", "trayTemplate.png"):
        image = Image.open(io.BytesIO((OUT / name).read_bytes()))
        assert image.mode == "RGBA" and image.width == image.height, name
    print("built:", ", ".join(sorted(p.name for p in OUT.iterdir())))


if __name__ == "__main__":
    main()
