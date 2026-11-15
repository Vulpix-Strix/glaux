# hardware.addr

The address where each device of the machine starts, and the addresses of the display registers.

## Notes

| Name             | Meaning                                                        |
|------------------|----------------------------------------------------------------|
| `WORK_RAM`       | Memory for the script's own use                                |
| `FRAMEBUFFER`    | The screen, one master palette index per pixel                 |
| `BG_TILES`       | The background tile bank                                       |
| `OBJ_TILES`      | The object tile bank                                           |
| `MASTER_PALETTE` | The master palette, three bytes per color                      |
| `BG_PALETTES`    | The background palette banks                                   |
| `OBJ_PALETTES`   | The object palette banks                                       |
| `OBJ_ATTRIBUTES` | Each object tile's default palette bank                        |
| `OAM`            | The object entries                                             |
| `DISPLAY_ENABLE` | The register that switches the backgrounds and objects on      |
| `BACKDROP_COLOR` | The register with the color the frame is cleared to            |

The background tilemaps and the per-layer registers are in [hardware.bg](glaux://api/hardware/bg).

## Examples

### Luau

```luau
poke(hardware.addr.BACKDROP_COLOR, 1)
```

## See also

- [hardware.size](glaux://api/hardware/size)
- [hardware](glaux://api/hardware/index)
