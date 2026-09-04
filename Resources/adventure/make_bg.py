#!/usr/bin/env python3
"""Bake castle_bridge.jpg -> castle_bridge.bgra, the raw backdrop the adventure
cutscene blits.

Like Resources/icon/make_icon.py, this is a one-time, run-by-hand step whose
output is checked in - the apps never decode a JPEG at build or run time (see
CLAUDE.md). Re-run it only if the source art changes.

Output: 192 x 192, 32-bit BGRA, top-down, no row padding, alpha forced opaque
(= 192*192*4 = 147456 bytes). Byte order matches render::Canvas / the pet
sprites (blue, green, red, alpha).

    python3 Resources/adventure/make_bg.py

Needs Pillow (`pip install pillow`). On a Windows box without Python, the
equivalent System.Drawing snippet is in CLAUDE.md.
"""

from pathlib import Path

from PIL import Image

SIZE = 192
HERE = Path(__file__).resolve().parent

src = Image.open(HERE / "castle_bridge.jpg").convert("RGBA")
# NEAREST keeps the source's pixel-art blockiness instead of smearing it.
src = src.resize((SIZE, SIZE), Image.NEAREST)

out = bytearray(SIZE * SIZE * 4)
px = src.load()
i = 0
for y in range(SIZE):
    for x in range(SIZE):
        r, g, b, _a = px[x, y]
        out[i + 0] = b
        out[i + 1] = g
        out[i + 2] = r
        out[i + 3] = 255
        i += 4

(HERE / "castle_bridge.bgra").write_bytes(bytes(out))
print(f"wrote castle_bridge.bgra ({len(out)} bytes)")
