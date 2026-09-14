#!/usr/bin/env python3
"""Generates the app icon (a key on a rounded dark square) with stdlib only."""
import struct, zlib, math, pathlib

BG = (0x1b, 0x21, 0x2b)
FG = (0x7c, 0xd4, 0x9a)

def pixels(s):
    rows = []
    r = 0.22 * s
    for y in range(s):
        row = bytearray()
        for x in range(s):
            cx, cy = min(max(x, r), s - r), min(max(y, r), s - r)
            inside = math.hypot(x - cx, y - cy) <= r
            d = math.hypot(x - 0.34 * s, y - 0.5 * s)
            on_key = 0.11 * s <= d <= 0.18 * s \
                or (0.45 * s <= x <= 0.78 * s and abs(y - 0.5 * s) <= 0.035 * s) \
                or any(abs(x - t * s) <= 0.028 * s and 0.5 * s <= y <= 0.60 * s for t in (0.62, 0.72))
            if not inside:
                row += bytes((0, 0, 0, 0))
            else:
                row += bytes(FG if on_key else BG) + b"\xff"
        rows.append(bytes(row))
    return rows

def chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))

here = pathlib.Path(__file__).parent
big = pixels(1024)
png = (b"\x89PNG\r\n\x1a\n"
       + chunk(b"IHDR", struct.pack(">IIBBBBB", 1024, 1024, 8, 6, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress(b"".join(b"\0" + r for r in big), 9))
       + chunk(b"IEND", b""))
(here / "icon.png").write_bytes(png)
# raw RGBA for the runtime dock icon: eframe sets NSApp's icon itself
(here / "icon.rgba").write_bytes(b"".join(pixels(256)))
print(here / "icon.png", here / "icon.rgba")
