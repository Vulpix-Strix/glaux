# copy

Copies a rectangle of map cells into a background window, at the window cells of the same world position.

## Parameters

| Name           | Meaning                                                                         |
|----------------|---------------------------------------------------------------------------------|
| `id`           | The map's id, the name of its directory under `maps/`.                          |
| `window`       | The background window to write, from 0 to 2.                                    |
| `x`            | World column of the rectangle's first cell.                                     |
| `y`            | World row of the rectangle's first cell.                                        |
| `width`        | Width in cells, from 1 to 64.                                                   |
| `height`       | Height in cells, from 1 to 64.                                                  |
| `source_layer` | The map layer to read, from 0 to 2. Optional, the same number as `window` by default. |

## Notes

A cell keeps its world position: map cell `(x, y)` is written to window cell `(x mod 64, y mod 64)`. To show a different part of a map, scroll the background and copy the rows or columns that come into view. To put other cells at the same place, copy them from another map of the same size.

Cells outside the map are written empty. An unknown id is an error, and a call with invalid arguments writes nothing.

## Examples

### Luau

```luau
-- Bring in the column that scrolled into view on the right.
map.copy("level1", 0, camera_x // 8 + 40, camera_y // 8, 1, 31)
```

## See also

- [load](glaux://api/map/load)
- [map](glaux://api/map/index)
