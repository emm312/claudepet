#!/usr/bin/env python3
"""Generates the ClaudePet app icon: the pixel-art pet face (from
Sources/ClaudePet/Pet/Sprites.swift's `idle1` grid) centered on a Big Sur
style rounded-square background. Outputs a 1024x1024 master PNG plus a
32x32 favicon-style PNG for the Windows .ico.
"""
from PIL import Image, ImageDraw

# idle1 grid from Sprites.swift, and the two palette colors it uses.
GRID = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "....11111111....",
    "...1111111111...",
    "...1122112211...",
    "1111122112211111",
    "1111111111111111",
    "...1111111111...",
    "...1111111111...",
    "...1111..1111...",
    "...1111..1111...",
]

BODY = (198, 116, 88, 255)   # Palette index 1
EYE = (20, 20, 20, 255)      # Palette index 2
BG_TOP = (237, 223, 201, 255)    # warm cream, matches Claude's paper tone
BG_BOTTOM = (226, 208, 180, 255)

CANVAS = 1024
CORNER_RADIUS = int(CANVAS * 0.2237)  # Big Sur squircle-ish proportion


def rounded_rect_mask(size, radius):
    mask = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(mask)
    d.rounded_rectangle([0, 0, size - 1, size - 1], radius=radius, fill=255)
    return mask


def make_background(size):
    bg = Image.new("RGBA", (size, size), BG_TOP)
    px = bg.load()
    for y in range(size):
        t = y / (size - 1)
        r = int(BG_TOP[0] + (BG_BOTTOM[0] - BG_TOP[0]) * t)
        g = int(BG_TOP[1] + (BG_BOTTOM[1] - BG_TOP[1]) * t)
        b = int(BG_TOP[2] + (BG_BOTTOM[2] - BG_TOP[2]) * t)
        for x in range(size):
            px[x, y] = (r, g, b, 255)
    mask = rounded_rect_mask(size, int(size * 0.2237))
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(bg, (0, 0), mask)
    return out


def make_pet(zoom):
    rows = len(GRID)
    cols = len(GRID[0])
    pet = Image.new("RGBA", (cols * zoom, rows * zoom), (0, 0, 0, 0))
    px = pet.load()
    for r, row in enumerate(GRID):
        for c, ch in enumerate(row):
            color = BODY if ch == "1" else EYE if ch == "2" else None
            if color is None:
                continue
            for zy in range(zoom):
                for zx in range(zoom):
                    px[c * zoom + zx, r * zoom + zy] = color
    return pet


def build(size, out_path):
    icon = make_background(size)
    # Pet occupies ~72% of the canvas width, nearest-neighbour crisp pixels.
    target_pet_width = int(size * 0.72)
    zoom = max(1, target_pet_width // len(GRID[0]))
    pet = make_pet(zoom)
    # Center on the pet's actual (non-transparent) bounding box, not the full
    # grid - the top ~7 rows of GRID are blank padding in the sprite sheet.
    bbox = pet.getbbox()
    bbox_w = bbox[2] - bbox[0]
    bbox_h = bbox[3] - bbox[1]
    px_off = (size - bbox_w) // 2 - bbox[0]
    py_off = (size - bbox_h) // 2 - bbox[1]
    icon.alpha_composite(pet, (px_off, py_off))
    icon.save(out_path)


if __name__ == "__main__":
    import sys
    out_dir = sys.argv[1] if len(sys.argv) > 1 else "."
    build(CANVAS, f"{out_dir}/icon-1024.png")
