# hardware.display

The screen size and the bits of the display enable register.

## Notes

| Name     | Meaning                                                 |
|----------|---------------------------------------------------------|
| `WIDTH`  | Screen width in pixels                                  |
| `HEIGHT` | Screen height in pixels                                 |
| `BG0`    | The bit that switches BG0 on                            |
| `BG1`    | The bit that switches BG1 on                            |
| `BG2`    | The bit that switches BG2 on                            |
| `OBJ`    | The bit that switches the objects on                    |
| `MASK`   | All the bits of the register that can be written        |

Everything is off at power-on. The register is at [hardware.addr.DISPLAY_ENABLE](glaux://api/hardware/addr).

## Examples

### Luau

```luau
local d = hardware.display
poke(hardware.addr.DISPLAY_ENABLE, bit32.bor(d.BG0, d.BG1, d.OBJ))
```

## See also

- [hardware](glaux://api/hardware/index)
