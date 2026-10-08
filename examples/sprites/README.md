# Sprite example

`sprites.png` comes from FisherG's
[Micro Asset Pack (8x8, 4 Colors)](https://fisherg.itch.io/micro-asset-pack), which the author
releases for free use in the public domain. Regenerate the native asset from the repository root
with:

```sh
mise run glaux -- convert sprites examples/sprites/sprites.png
```

The converter places the sheet on G1's 16-column object-tile grid: the sheet tile in column `c`
and row `r` becomes tile `r * 16 + c`. This 64-pixel-wide sheet fills the left half of the
grid's first eight rows.

The cartridge draws sixteen single tiles from sheet rows 2 and 3, then two composed sprites:
`spr(36, x, y, 2, 2)` draws a 16x16 block from tiles 36, 37, 52 and 53, and
`spr(0, x, y, 4, 4)` draws the sheet's top-left 32x32 block.

Below the 16x16 sprite, the same block is drawn mirrored with `spr`'s optional `flip_x` and
`flip_y` arguments: `spr(tile, x, y, [w], [h], [flip_x], [flip_y])`. Both default to `false`.
Flipping mirrors the whole composed block, reversing tile order as well as the pixels within each
tile, and costs the same as an unflipped draw.
