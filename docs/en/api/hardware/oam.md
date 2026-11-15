# hardware.oam

The layout of the object entries, the sprites the machine draws itself.

## Notes

Each entry holds a 16-bit X and Y, a 16-bit first tile, an attribute byte and a size byte. Entries are composed before `draw()` runs, so set them in `tick()`. Between objects of the same priority, the lower entry is drawn in front.

| Name                  | Meaning                                                        |
|-----------------------|----------------------------------------------------------------|
| `ENTRIES`             | How many entries there are                                     |
| `ENTRY_BYTES`         | Bytes in one entry                                             |
| `FIELD_X`             | Offset of the signed X position                                |
| `FIELD_Y`             | Offset of the signed Y position                                |
| `FIELD_TILE`          | Offset of the first tile on the object tile grid               |
| `FIELD_ATTRIBUTES`    | Offset of the attribute byte                                   |
| `FIELD_SIZE`          | Offset of the size byte                                        |
| `TILE_MASK`           | The bits of the tile field that are used                       |
| `ATTR_BANK_MASK`      | The attribute bits that hold the object palette bank           |
| `ATTR_FLIP_X`         | The attribute bit that mirrors the object horizontally         |
| `ATTR_FLIP_Y`         | The attribute bit that mirrors the object vertically           |
| `ATTR_PRIORITY_SHIFT` | Where the object's priority starts in the attribute byte       |
| `ATTR_PRIORITY_MASK`  | The priority bits once shifted down, 0 in front to 3 behind    |
| `SIZE_MASK`           | The bits of the size byte that hold the size code              |
| `ENABLED`             | The bit of the size byte that switches the entry on            |
| `SIZE_8X8`            | Size code of an 8 by 8 object                                  |
| `SIZE_16X16`          | Size code of a 16 by 16 object                                 |
| `SIZE_32X32`          | Size code of a 32 by 32 object                                 |
| `SIZE_8X16`           | Size code of an 8 by 16 object                                 |
| `SIZE_16X8`           | Size code of a 16 by 8 object                                  |
| `SIZE_16X32`          | Size code of a 16 by 32 object                                 |
| `SIZE_32X16`          | Size code of a 32 by 16 object                                 |
| `SIZE_RESERVED`       | A reserved size code. An object with it is not drawn.          |

## Examples

### Luau

```luau
local o = hardware.oam
local at = hardware.addr.OAM + 0 * o.ENTRY_BYTES
poke16(at + o.FIELD_X, 152)
poke16(at + o.FIELD_Y, 120)
poke16(at + o.FIELD_TILE, 0)
poke(at + o.FIELD_ATTRIBUTES, o.ATTR_FLIP_X)
poke(at + o.FIELD_SIZE, bit32.bor(o.SIZE_16X16, o.ENABLED))
```

## See also

- [hardware.obj](glaux://api/hardware/obj)
- [hardware](glaux://api/hardware/index)
