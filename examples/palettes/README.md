# Palettes

A sprite sheet drawn in true color and imported with its colors intact. The
cartridge draws the whole sheet, each sprite on its own, and two sprites
recolored with another sprite's palette.

## Running it

```sh
mise run sync-examples -- palettes
mise run glaux
```

Then type `load palettes` and `run` in the Glaux shell.

## Structure

`code/main.luau` draws everything. `sprites.png` is the sheet and
`sprites.sprites.ron` groups the tiles that must share a palette. The
`.bin` files and `sprites.import.ron` are written by the importer.

## What it shows

`glaux import sprites` keeps a sheet's own colors by building the object
palettes for it, so the cartridge needs no palette setup. `spr()` draws each
sprite with the palette it was imported with, and its last argument draws it
through another palette.

## Rebuilding the assets

After editing the PNG or the groups, from the repository root:

```sh
mise run glaux -- import sprites examples/palettes/sprites.png
```
