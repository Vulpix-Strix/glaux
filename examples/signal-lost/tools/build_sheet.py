# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow>=11"]
# ///
"""Builds Signal Lost's Glaux sprite sheet from 0x72's DungeonTileset II.

Usage:
    mise run signal-lost-sheet -- <path to 0x72_DungeonTilesetII_v1.7/frames>

Writes, next to this script's parent directory:
    sprites.png          RGBA sheet in the source colors, for `glaux import sprites`
    sprites.sprites.ron  one sprite group per sprite, covering all its frames
    code/sheet.luau      object-tile indices of every sprite in the sheet

The sheet is packed on G1's 16-column object-tile grid in bands of equal
height, so no multi-tile sprite crosses a grid row. Each sprite is stored
once, facing right; the game draws the left-facing versions with `spr`'s
`flip_x`.

Source pixels with alpha of at least 128 become opaque in their own color,
the rest transparent. `glaux import sprites` then assigns the colors to
palettes; each sprite group shares one palette.
"""

import sys
from pathlib import Path

from PIL import Image

TILE = 8
GRID_COLUMNS = 16


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


def render(src_dir, frame, width, height):
    """The frame as RGBA pixels, or None for transparent ones, in a width x
    height pixel cell."""
    image = Image.open(src_dir / frame.file).convert("RGBA")
    if frame.crop:
        image = image.crop(frame.crop)
    cell = [[None] * width for _ in range(height)]
    ax, ay = frame.at
    if ax + image.width > width or ay + image.height > height:
        sys.exit(f"{frame.file}: {image.size} at {frame.at} does not fit {width}x{height}")
    for y in range(image.height):
        for x in range(image.width):
            r, g, b, a = image.getpixel((x, y))
            if a >= 128:
                cell[ay + y][ax + x] = (r, g, b, 255)
    return cell


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    src_dir = Path(sys.argv[1])
    out_dir = Path(__file__).resolve().parent.parent

    rows = sum(height for height, _ in BANDS)
    sheet = Image.new("RGBA", (GRID_COLUMNS * TILE, rows * TILE), (0, 0, 0, 0))

    entries = []
    groups_out = []
    top = 0
    for height, groups in BANDS:
        column = 0
        for name, width, group in groups:
            first_column = column
            tiles = []
            for frame in group:
                if column + width > GRID_COLUMNS:
                    sys.exit(f"band at tile row {top} overflows the 16-column grid at {name}")
                cell = render(src_dir, frame, width * TILE, height * TILE)
                for y, row in enumerate(cell):
                    for x, pixel in enumerate(row):
                        if pixel:
                            sheet.putpixel((column * TILE + x, top * TILE + y), pixel)
                tiles.append(top * GRID_COLUMNS + column)
                column += width
            entries.append((name, width, height, tiles))
            groups_out.append((name, (first_column, top), (column - first_column, height)))
        top += height

    sheet.save(out_dir / "sprites.png", optimize=False)

    group_lines = ["(groups: ["]
    for name, (x, y), (w, h) in groups_out:
        group_lines.append(f'    (name: "{name}", tile: ({x}, {y}), size: ({w}, {h})),')
    group_lines.append("])")
    (out_dir / "sprites.sprites.ron").write_text("\n".join(group_lines) + "\n")

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


if __name__ == "__main__":
    main()
