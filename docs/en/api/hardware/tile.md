# hardware.tile

The layout of a background tilemap entry, the 16-bit value of one cell.

## Notes

| Name          | Meaning                                                                       |
|---------------|-------------------------------------------------------------------------------|
| `ENTRY_BYTES` | Bytes in one entry                                                            |
| `INDEX_MASK`  | The bits that hold the background tile index                                 |
| `FLIP_X`      | The bit that mirrors the tile horizontally                                    |
| `FLIP_Y`      | The bit that mirrors the tile vertically                                      |
| `PRIORITY`    | The bit that draws the tile one priority level in front of its layer         |
| `BANK_SHIFT`  | Where the background palette bank starts                                      |
| `BANK_MASK`   | The bank's bits once shifted down by `BANK_SHIFT`                             |

Tile 0 is the empty cell and draws nothing. Inside a tile, color 0 is transparent.

## Examples

### Luau

```luau
local t = hardware.tile
local entry = bit32.bor(12, t.FLIP_X, bit32.lshift(1, t.BANK_SHIFT))
poke16(hardware.bg[0].TILEMAP, entry)
```

## See also

- [hardware.bg](glaux://api/hardware/bg)
- [get](glaux://api/map/get)
- [hardware](glaux://api/hardware/index)
