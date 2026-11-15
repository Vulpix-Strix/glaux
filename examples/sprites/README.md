# Sprites

Draws single 8×8 tiles, a 16×16 and a 32×32 sprite composed from several
tiles, and the 16×16 sprite mirrored horizontally, vertically and on both
axes.

## Running it

```sh
mise run sync-examples -- sprites
mise run glaux
```

Then type `load sprites` and `run` in the Glaux shell.

## What it shows

How `spr(tile, x, y, [w], [h], [flip_x], [flip_y])` addresses the 16-column
object-tile grid: the sheet tile in column `c` and row `r` is tile
`r * 16 + c`, and a `w` by `h` block is drawn from the tiles to its right
and below.

## Credits

`sprites.png` comes from FisherG's
[Micro Asset Pack (8x8, 4 Colors)](https://fisherg.itch.io/micro-asset-pack),
released into the public domain.

## Rebuilding the assets

From the repository root:

```sh
mise run glaux -- convert sprites examples/sprites/sprites.png
```
