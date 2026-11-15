# hardware

Read-only constants that describe the G1 machine: memory addresses, sizes and the bit layouts of its registers, tilemaps and objects.

## Notes

Use them with the [memory functions](glaux://guide/memory) instead of writing numbers, so a script says what it means: `poke(hardware.addr.DISPLAY_ENABLE, hardware.display.BG0)`. The values come from the machine itself.

| Group                                            | Contents                                              |
|--------------------------------------------------|-------------------------------------------------------|
| [addr](glaux://api/hardware/addr)                | Where each device and register starts                 |
| [size](glaux://api/hardware/size)                | How many bytes each device holds                      |
| [bg](glaux://api/hardware/bg)                    | The background layers and their registers             |
| [display](glaux://api/hardware/display)          | The screen and the display enable bits                |
| [tile](glaux://api/hardware/tile)                | The bits of a background tilemap entry                |
| [oam](glaux://api/hardware/oam)                  | The layout of an object entry                         |
| [obj](glaux://api/hardware/obj)                  | Object tiles and palettes                             |
| [palette](glaux://api/hardware/palette)          | The master palette and palette banks                  |

### Luau

The tables are read-only. Writing to one, or to a group inside it, is an error.

## See also

- [Memory](glaux://guide/memory)
- [map](glaux://api/map/index)
