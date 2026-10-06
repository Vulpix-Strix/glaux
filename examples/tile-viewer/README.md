# Tile viewer example

This development cartridge displays 40 consecutive OBJ tiles in an 8×5 grid
and labels each tile with its index. The selected tile is the first tile in the
visible range and has a border around it.

- Left/right moves the selected tile by one.
- Up/down moves it by one row of the 16-column object-tile grid (sixteen
  tiles).
- `START_TILE`, `COLUMNS`, and `ROWS` in `code/grid.luau` configure the initial
  range and grid.

The code is split across two files. `code/main.luau` is the entry named in
`cartridge.ron`. It loads the layout and drawing helpers with
`require("grid")`, which runs `code/grid.luau` once and returns the table that
module returns. Module names are dotted and have no extension:
`require("game.enemies")` would load `code/game/enemies.luau`. Modules can only
come from the cartridge's `code/` directory.

The included `sprites.bin` is generated from FisherG's
[Micro Asset Pack (8x8, 4 Colors)](https://fisherg.itch.io/micro-asset-pack),
which the author releases for free use in the public domain. Its indexed PNG
source is available in [`examples/sprites`](../sprites/).
