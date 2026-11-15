# size

Reports a map's size in tiles.

## Parameters

| Name | Meaning                                                |
|------|--------------------------------------------------------|
| `id` | The map's id, the name of its directory under `maps/`. |

## Returns

The width and the height, from 1 to 256 each.

## Examples

### Luau

```luau
local width, height = map.size("arena")
```

## See also

- [get](glaux://api/map/get)
- [map](glaux://api/map/index)
