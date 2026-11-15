# Tile viewer

Shows 40 consecutive object tiles in an 8×5 grid, each labeled with its
index. The selected tile is the first one shown and has a border around it.

## Controls

| Virtual button | Desktop key      | Action                                   |
|----------------|------------------|------------------------------------------|
| Left / Right   | Arrow keys       | Move the selection by one tile           |
| Up / Down      | Arrow keys       | Move it by one row of the grid (16 tiles) |

## Running it

```sh
mise run sync-examples -- tile-viewer
mise run glaux
```

Then type `load tile-viewer` and `run` in the Glaux shell.

## Structure

`code/main.luau` handles input and loads the layout and drawing helpers from
`code/grid.luau` with `require("grid")`. `START_TILE`, `COLUMNS` and `ROWS`
in `grid.luau` set the first tile and the size of the grid.

## What it shows

Splitting a cartridge into modules with `require()`, and how the object
tiles are numbered on the 16-column grid.

## Credits

`sprites.bin` is built from FisherG's
[Micro Asset Pack (8x8, 4 Colors)](https://fisherg.itch.io/micro-asset-pack),
released into the public domain. Its PNG is in [`examples/sprites`](../sprites/).
