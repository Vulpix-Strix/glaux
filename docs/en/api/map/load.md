# load

Fills a whole background window with the 64 by 64 cells of a map that start at a world position.

## Parameters

| Name           | Meaning                                                                         |
|----------------|---------------------------------------------------------------------------------|
| `id`           | The map's id, the name of its directory under `maps/`.                          |
| `window`       | The background window to write, from 0 to 2.                                    |
| `x`            | World column of the first cell. Optional, 0 by default.                         |
| `y`            | World row of the first cell. Optional, 0 by default.                            |
| `source_layer` | The map layer to read, from 0 to 2. Optional, the same number as `window` by default. |

## Notes

Cells outside the map are written empty. An unknown id is an error, and a call with invalid arguments writes nothing.

## Examples

### Luau

```luau
function boot()
  for layer = 0, 2 do
    map.load("arena", layer)
  end
end
```

## See also

- [copy](glaux://api/map/copy)
- [map](glaux://api/map/index)
