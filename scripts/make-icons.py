#!/usr/bin/env python3
"""Generates the Dev Console icon set deterministically (no image libraries needed).

Run: python3 scripts/make-icons.py
Writes src-tauri/icons/{32x32,128x128,icon,icon@2x}.png
"""
import os
import struct
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICONS = os.path.join(ROOT, "src-tauri", "icons")

BG = (32, 36, 46, 255)
FG = (86, 156, 255, 255)
CURSOR = (240, 243, 250, 255)


def render(size: int) -> bytes:
    scale = size / 32.0
    rows = []
    for y in range(size):
        row = bytearray()
        for x in range(size):
            fx, fy = x / scale, y / scale
            # rounded square background
            pad = 2.0
            inside = pad <= fx <= size / scale - pad and pad <= fy <= size / scale - pad
            corner = 5.0
            if inside:
                dx = max(pad + corner - fx, fx - (size / scale - pad - corner), 0)
                dy = max(pad + corner - fy, fy - (size / scale - pad - corner), 0)
                inside = (dx * dx + dy * dy) <= corner * corner
            # ">" prompt glyph
            glyph = False
            if 8 <= fx <= 15 and 9 <= fy <= 23:
                t = (fy - 9) / 14.0
                center = 8 + 6 * t
                glyph = abs(fx - center) < 1.5
            # underscore cursor
            cursor = 12 <= fx <= 23 and 21 <= fy <= 23
            if glyph:
                px = FG
            elif cursor:
                px = CURSOR
            elif inside:
                px = BG
            else:
                px = (0, 0, 0, 0)
            row += bytes(px)
        rows.append(bytes(row))
    raw = b"".join(b"\x00" + r for r in rows)

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def main() -> None:
    os.makedirs(ICONS, exist_ok=True)
    targets = {
        "32x32.png": 32,
        "128x128.png": 128,
        "icon.png": 256,
        "icon@2x.png": 512,
    }
    for name, size in targets.items():
        with open(os.path.join(ICONS, name), "wb") as handle:
            handle.write(render(size))
        print(f"wrote src-tauri/icons/{name} ({size}x{size})")


if __name__ == "__main__":
    main()
