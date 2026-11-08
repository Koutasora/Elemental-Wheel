"""Generates assets/icon.svg and assets/elemental-wheel.ico from one geometry (standard library only).

Usage: python tools/make-icon.py
"""

import math
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SIZE = 256
CX = CY = 128.0
R_DISK = 122.0
R_OUT, R_IN = 104.0, 62.0
R_DOT = 26.0
GAP = 7.0  # degrees between segments
STROKE = 1.5  # half of the 3 px SVG stroke that rounds the segments
ACCENT = (0x28, 0x51, 0xD0)
FAINT = 0.38
ICO_SIZES = (16, 20, 24, 32, 40, 48, 64, 256)


def point(r, deg):
    a = math.radians(deg - 90)
    return CX + r * math.cos(a), CY + r * math.sin(a)


def segment_path(i):
    a0 = i * 30 - 15 + GAP / 2
    a1 = i * 30 + 15 - GAP / 2
    x0, y0 = point(R_OUT, a0)
    x1, y1 = point(R_OUT, a1)
    x2, y2 = point(R_IN, a1)
    x3, y3 = point(R_IN, a0)
    return (
        f"M{x0:.2f} {y0:.2f}A{R_OUT:g} {R_OUT:g} 0 0 1 {x1:.2f} {y1:.2f}"
        f"L{x2:.2f} {y2:.2f}A{R_IN:g} {R_IN:g} 0 0 0 {x3:.2f} {y3:.2f}Z"
    )


def svg():
    others = "".join(segment_path(i) for i in range(1, 12))
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <circle cx="128" cy="128" r="{R_DISK:g}" fill="#2851d0"/>
  <path d="{others}" fill="#ffffff" fill-opacity="{FAINT}" stroke="#ffffff" stroke-opacity="{FAINT}" stroke-width="{2 * STROKE:g}" stroke-linejoin="round"/>
  <path d="{segment_path(0)}" fill="#ffffff" stroke="#ffffff" stroke-width="{2 * STROKE:g}" stroke-linejoin="round"/>
  <circle cx="128" cy="128" r="{R_DOT:g}" fill="#ffffff"/>
</svg>
"""


def white_alpha(x, y):
    """Opacity of the white layer at (x, y) in 256-unit space: 1 for the top segment and the dot,
    FAINT for the other segments, 0 elsewhere. The stroke widens each segment by STROKE."""
    dx, dy = x - CX, y - CY
    r = math.hypot(dx, dy)
    if r <= R_DOT:
        return 1.0
    if r < R_IN - STROKE or r > R_OUT + STROKE:
        return 0.0
    deg = math.degrees(math.atan2(dx, -dy)) % 360  # 0 at the top, clockwise
    i = int(((deg + 15) % 360) // 30)
    centre = i * 30
    off = abs((deg - centre + 180) % 360 - 180)
    half = 15 - GAP / 2
    # distance from the segment's straight edge (perpendicular, in units)
    edge = r * math.sin(math.radians(max(0.0, off - half)))
    if off > half and edge > STROKE:
        return 0.0
    return 1.0 if i == 0 else FAINT


def render(size, samples=8):
    scale = SIZE / size
    rows = []
    for py in range(size):
        row = bytearray()
        for px in range(size):
            disk = 0.0
            white = 0.0
            for sy in range(samples):
                for sx in range(samples):
                    x = (px + (sx + 0.5) / samples) * scale
                    y = (py + (sy + 0.5) / samples) * scale
                    if math.hypot(x - CX, y - CY) <= R_DISK:
                        disk += 1
                        white += white_alpha(x, y)
            n = samples * samples
            a = disk / n
            if a == 0:
                row += b"\0\0\0\0"
                continue
            w = white / disk  # share of white over the disk at covered samples
            rgb = [round(c + (255 - c) * w) for c in ACCENT]
            row += bytes([rgb[0], rgb[1], rgb[2], round(a * 255)])
        rows.append(bytes(row))
    return rows


def png(size, rows):
    raw = b"".join(b"\0" + r for r in rows)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def bmp_entry(size, rows):
    """32-bit BGRA DIB with an empty AND mask (for small sizes, read by every Windows component)."""
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = bytearray()
    for r in reversed(rows):
        for i in range(0, len(r), 4):
            pixels += bytes([r[i + 2], r[i + 1], r[i], r[i + 3]])
    mask_row = ((size + 31) // 32) * 4
    return header + bytes(pixels) + bytes(mask_row * size)


def ico():
    images = []
    for size in ICO_SIZES:
        rows = render(size)
        images.append((size, png(size, rows) if size >= 256 else bmp_entry(size, rows)))
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, data in images:
        dim = 0 if size >= 256 else size
        out += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    return out + b"".join(d for _, d in images)


def main():
    assets = ROOT / "assets"
    assets.mkdir(exist_ok=True)
    (assets / "icon.svg").write_text(svg(), encoding="utf-8", newline="\n")
    (assets / "elemental-wheel.ico").write_bytes(ico())
    print("assets/icon.svg, assets/elemental-wheel.ico")


if __name__ == "__main__":
    main()
