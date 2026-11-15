# hardware.bg

The background layers BG0 to BG2: their sizes, and the addresses of each layer's tilemap window and registers.

## Notes

| Name            | Meaning                                                   |
|-----------------|-----------------------------------------------------------|
| `LAYERS`        | How many background layers there are                      |
| `MAP_SIDE`      | Cells along each side of a tilemap window                 |
| `TILE_BYTES`    | Bytes in one background tile                              |
| `TILE_CAPACITY` | Tiles in the background tile bank                         |
| `PALETTE_BANKS` | Background palette banks                                  |
| `PRIORITY_MASK` | The bits of a priority register that are used             |

`hardware.bg[0]`, `hardware.bg[1]` and `hardware.bg[2]` describe BG0, BG1 and BG2:

| Name       | Meaning                                                                                  |
|------------|------------------------------------------------------------------------------------------|
| `TILEMAP`  | Address of the layer's tilemap window, one 16-bit entry per cell, row by row             |
| `SCROLL_X` | Address of the 16-bit horizontal scroll register, in pixels. The window wraps around.    |
| `SCROLL_Y` | Address of the 16-bit vertical scroll register, in pixels                                |
| `PRIORITY` | Address of the priority register, 0 in front to 3 behind                                 |

A layer is drawn only while its bit of [DISPLAY_ENABLE](glaux://api/hardware/display) is set. The bits of a tilemap entry are in [hardware.tile](glaux://api/hardware/tile).

## Examples

### Luau

```luau
poke16(hardware.bg[0].SCROLL_X, 8)
poke(hardware.bg[2].PRIORITY, 0)
```

## See also

- [hardware.tile](glaux://api/hardware/tile)
- [map](glaux://api/map/index)
- [hardware](glaux://api/hardware/index)
