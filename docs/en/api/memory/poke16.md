# poke16

Writes a 16-bit value to two consecutive bytes, low byte first.

## Parameters

| Name | Meaning |
|------|---------|
| `address` | An address from 0 to 1048575 (`0xFFFFF`). |
| `value` | A value from 0 to 65535. |

## Notes

An address outside the address space, a value outside its range and a fraction are errors, and nothing is written. The range must fit in the address space. Both bytes are written even if they belong to different devices, each with its own rules.

## Examples

### Luau

```luau
poke16(0x3C010, 120)
```

## See also

- [poke](glaux://api/memory/poke)
- [peek16](glaux://api/memory/peek16)
- [poke32](glaux://api/memory/poke32)
- [Memory](glaux://guide/memory)
