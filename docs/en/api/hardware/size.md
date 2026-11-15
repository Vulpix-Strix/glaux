# hardware.size

How many bytes each device of the machine holds.

## Notes

| Name             | Meaning                                          |
|------------------|--------------------------------------------------|
| `ADDRESS_SPACE`  | The whole address space                          |
| `WORK_RAM`       | Work RAM                                         |
| `FRAMEBUFFER`    | The framebuffer                                  |
| `BG_TILES`       | The background tile bank                         |
| `OBJ_TILES`      | The object tile bank                             |
| `TILEMAP`        | One background tilemap window                    |
| `MASTER_PALETTE` | The master palette                               |
| `BG_PALETTES`    | All background palette banks                     |
| `OBJ_PALETTES`   | All object palette banks                         |
| `OBJ_ATTRIBUTES` | The object tile attributes                       |
| `OAM`            | All object entries                               |

## Examples

### Luau

```luau
memset(hardware.addr.OAM, 0, hardware.size.OAM)
```

## See also

- [hardware.addr](glaux://api/hardware/addr)
- [hardware](glaux://api/hardware/index)
