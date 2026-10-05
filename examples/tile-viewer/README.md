# Tile viewer example

This development cartridge displays 40 consecutive OBJ tiles in an 8×5 grid
and labels each tile with its index. The selected tile is the first tile in the
visible range and has a border around it.

- Left/right moves the selected tile by one.
- Up/down moves it by one visible row (eight tiles).
- `START_TILE`, `COLUMNS`, and `ROWS` in `main.luau` configure the initial
  range and grid.

The included `sprites.bin` is generated from FisherG's
[Micro Asset Pack (8x8, 4 Colors)](https://fisherg.itch.io/micro-asset-pack),
which the author releases for free use in the public domain. Its indexed PNG
source is available in [`examples/sprites`](../sprites/).
