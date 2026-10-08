# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow>=11"]
# ///
"""Builds Signal Lost's Glaux sprite sheet from 0x72's DungeonTileset II.

Usage:
    mise run signal-lost-sheet -- <path to 0x72_DungeonTilesetII_v1.7/frames>

Writes, next to this script's parent directory:
    sprites.png       4-bit indexed sheet for `glaux convert sprites`
    code/sheet.luau   object-tile indices of every sprite in the sheet

The sheet is packed on G1's 16-column object-tile grid in bands of equal
height, so no multi-tile sprite crosses a grid row. Each sprite is stored
once, facing right; the game draws the left-facing versions with `spr`'s
`flip_x`.

G1 writes object-tile indices 1-15 straight to the framebuffer as master
palette indices, so every source color is mapped to the closest of Zughy 32
entries 1-15 (the system ROM's first entries). Index 0 is transparent.
"""

import sys
from pathlib import Path

from PIL import Image

TILE = 8
GRID_COLUMNS = 16

# Zughy 32 entries 0-15, as in rom/palette/system.hex. Entry 0 only appears
# in the PNG palette so the sheet previews sensibly; it is never drawn.
ZUGHY = [
    0x472D3C, 0x5E3643, 0x7A444A, 0xA05B53, 0xBF7958, 0xEEA160, 0xF4CCA1, 0xB6D53C,
    0x71AA34, 0x397B44, 0x3C5956, 0x302C2E, 0x5A5353, 0x7D7071, 0xA0938E, 0xCFC6B8,
]
RGB = [((c >> 16) & 255, (c >> 8) & 255, c & 255) for c in ZUGHY]

# Source colors whose nearest match reads badly in context, pinned by hand.
# Without them the goblin's two greens and the coin's two golds each collapse
# into one entry, and the heart red lands on a brown too close to the empty
# heart's interior.
OVERRIDES: dict[tuple[int, int, int], int] = {
    (0x5C, 0x7A, 0x56): 8,
    (0xFA, 0xCB, 0x3E): 6,
    (0xDA, 0x4E, 0x38): 5,
}


def nearest(color):
    """Closest palette entry 1-15 by the "redmean" weighted RGB distance."""
    if color in OVERRIDES:
        return OVERRIDES[color]
    r1, g1, b1 = color

    def distance(index):
        r2, g2, b2 = RGB[index]
        mean = (r1 + r2) / 2
        dr, dg, db = r1 - r2, g1 - g2, b1 - b2
        return (2 + mean / 256) * dr * dr + 4 * dg * dg + (2 + (255 - mean) / 256) * db * db

    return min(range(1, 16), key=distance)


class Frame:
    """One source PNG, optionally cropped, pasted at `at` in a w x h tile cell."""

    def __init__(self, file, crop=None, at=(0, 0)):
        self.file, self.crop, self.at = file, crop, at


def frames(stem, count, **kw):
    return [Frame(f"{stem}{i}.png", **kw) for i in range(count)]


KNIGHT = dict(crop=(0, 4, 16, 28))  # 16x28 with empty top rows -> 16x24
ORC = dict(at=(0, 1))  # 16x23 -> bottom-aligned in 16x24
OGRE = dict(crop=(0, 4, 32, 36))  # 32x36 with empty top rows -> 32x32
knight_idle = frames("knight_m_idle_anim_f", 4, **KNIGHT)
knight_run = frames("knight_m_run_anim_f", 4, **KNIGHT)
orc_run = frames("masked_orc_run_anim_f", 4, **ORC)
goblin_run = frames("goblin_run_anim_f", 4)
ogre_run = frames("ogre_run_anim_f", 4, **OGRE)
axe = Frame("weapon_throwing_axe.png", at=(3, 1))

# Bands of (tile height, [(name, tile width, frames)]). A name with one frame
# becomes a number in sheet.luau; several frames become a list.
BANDS = [
    (3, [("knight_idle", 2, knight_idle), ("knight_run", 2, knight_run)]),
    (3, [("orc_run", 2, orc_run)]),
    (2, [("goblin_run", 2, goblin_run)]),
    (2, [
        ("axe", 2, [axe]),
        ("heart_full", 2, [Frame("ui_heart_full.png", at=(1, 2))]),
        ("heart_half", 2, [Frame("ui_heart_half.png", at=(1, 2))]),
        ("heart_empty", 2, [Frame("ui_heart_empty.png", at=(1, 2))]),
        ("flask", 2, [Frame("flask_big_red.png")]),
        ("wall_mid", 2, [Frame("wall_mid.png")]),
        ("wall_top", 2, [Frame("wall_top_mid.png")]),
    ]),
    (2, [("floor", 2, [Frame(f"floor_{i}.png") for i in range(1, 9)])]),
    (4, [("ogre_run", 4, ogre_run)]),
    (1, [
        ("coin", 1, [Frame(f"coin_anim_f{i}.png", at=(1, 1)) for i in range(4)]),
        ("skull", 1, [Frame("skull.png", crop=(5, 7, 11, 13), at=(1, 1))]),
    ]),
]


def render(src_dir, frame, width, height, mapping):
    """The frame as palette indices in a width x height pixel cell."""
    image = Image.open(src_dir / frame.file).convert("RGBA")
    if frame.crop:
        image = image.crop(frame.crop)
    cell = [[0] * width for _ in range(height)]
    ax, ay = frame.at
    if ax + image.width > width or ay + image.height > height:
        sys.exit(f"{frame.file}: {image.size} at {frame.at} does not fit {width}x{height}")
    for y in range(image.height):
        for x in range(image.width):
            r, g, b, a = image.getpixel((x, y))
            if a >= 128:
                index = nearest((r, g, b))
                mapping.setdefault((r, g, b), index)
                cell[ay + y][ax + x] = index
    return cell


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    src_dir = Path(sys.argv[1])
    out_dir = Path(__file__).resolve().parent.parent

    rows = sum(height for height, _ in BANDS)
    sheet = Image.new("P", (GRID_COLUMNS * TILE, rows * TILE), 0)
    palette = [channel for rgb in RGB for channel in rgb]
    sheet.putpalette(palette)

    mapping = {}
    entries = []
    top = 0
    for height, groups in BANDS:
        column = 0
        for name, width, group in groups:
            tiles = []
            for frame in group:
                if column + width > GRID_COLUMNS:
                    sys.exit(f"band at tile row {top} overflows the 16-column grid at {name}")
                cell = render(src_dir, frame, width * TILE, height * TILE, mapping)
                for y, row in enumerate(cell):
                    for x, index in enumerate(row):
                        sheet.putpixel((column * TILE + x, top * TILE + y), index)
                tiles.append(top * GRID_COLUMNS + column)
                column += width
            entries.append((name, width, height, tiles))
        top += height

    sheet.save(out_dir / "sprites.png", bits=4, transparency=0, optimize=False)

    lines = [
        "-- Generated by tools/build_sheet.py; do not edit by hand.",
        "-- Object-tile index of each sprite's top-left tile, with its size in tiles.",
        "return {",
    ]
    for name, width, height, tiles in entries:
        value = str(tiles[0]) if len(tiles) == 1 else "{ " + ", ".join(map(str, tiles)) + " }"
        lines.append(f"    {name} = {value}, -- {width}x{height}")
    lines.append("}")
    (out_dir / "code" / "sheet.luau").write_text("\n".join(lines) + "\n")

    for name, width, height, tiles in entries:
        print(f"{name:14} {width}x{height} tiles {tiles}")
    print("color mapping (source -> Zughy index):")
    for color, index in sorted(mapping.items(), key=lambda item: item[1]):
        print(f"  #{color[0]:02x}{color[1]:02x}{color[2]:02x} -> {index:2} #{ZUGHY[index]:06x}")


if __name__ == "__main__":
    main()
