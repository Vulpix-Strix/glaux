# spr

Draws a block of 8 by 8 object tiles from the object-tile grid.

## Parameters

| Name      | Meaning                                                                                                |
|-----------|--------------------------------------------------------------------------------------------------------|
| `tile`    | Index of the top-left tile of the block, from 0 to 1023. The grid is 16 tiles wide.                    |
| `x`       | Column of the block's top-left corner.                                                                 |
| `y`       | Row of the block's top-left corner.                                                                    |
| `w`       | Width in tiles. Optional, 1 by default.                                                                |
| `h`       | Height in tiles. Optional, 1 by default.                                                               |
| `flip_x`  | Mirror the whole block horizontally. Optional, `false` by default.                                     |
| `flip_y`  | Mirror the whole block vertically. Optional, `false` by default.                                       |
| `palette` | Object palette bank for every tile of the block. Optional. Without it each tile uses its default bank. |

## Notes

Drawing functions may only be called during `draw()`. Everything is clipped to the 320 by 240 screen.

The block may not run past the right or bottom edge of the grid. Flipping reverses the order of tile columns or rows and the pixels inside each tile, and does not change the draw cost. Color index 0 of a tile is transparent and is skipped.

### Luau

Leave optional arguments out or pass `nil` to take their defaults.

### Rhai

Leave optional arguments out or pass `()` to take their defaults. A call takes from 3 to 8 arguments.

## Examples

### Luau

```luau
spr(1, 40, 40)
spr(16, 80, 40, 2, 2, true)
```

### Rhai

```rhai
spr(1, 40, 40);
spr(16, 80, 40, 2, 2, true);
```

## See also

- [pset](glaux://api/graphics/pset)
