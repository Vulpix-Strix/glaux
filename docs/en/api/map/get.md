# get

Reads one cell of a map.

## Parameters

| Name    | Meaning                                                |
|---------|--------------------------------------------------------|
| `id`    | The map's id, the name of its directory under `maps/`. |
| `layer` | The map layer, from 0 to 2.                            |
| `x`     | World column of the cell.                              |
| `y`     | World row of the cell.                                 |

## Returns

The cell's background tile index, and its whole 16-bit tilemap entry with the flip, priority and palette bits. A cell outside the map gives `0, 0`, the empty cell. The bits of the entry are described in [hardware.tile](glaux://api/hardware/tile).

## Notes

The entry can be written to a window with [poke16](glaux://api/memory/poke16) to copy one cell to a different place than [copy](glaux://api/map/copy) would.

## Examples

### Luau

```luau
local tile, entry = map.get("arena", 1, 10, 4)
local solid = tile ~= 0
```

## See also

- [size](glaux://api/map/size)
- [map](glaux://api/map/index)
