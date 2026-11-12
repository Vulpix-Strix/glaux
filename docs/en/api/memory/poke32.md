# poke32

Writes a 32-bit value to four consecutive bytes, low byte first.

## Parameters

| Name | Meaning |
|------|---------|
| `address` | An address from 0 to 1048575 (`0xFFFFF`). |
| `value` | A value from 0 to 4294967295. |

## Notes

An address outside the address space, a value outside its range and a fraction are errors, and nothing is written. The range must fit in the address space. Each byte follows the rules of its own device.

## Examples

### Luau

```luau
poke32(0, 0xDEADBEEF)
```

## See also

- [poke](glaux://api/memory/poke)
- [poke16](glaux://api/memory/poke16)
- [peek32](glaux://api/memory/peek32)
- [Memory](glaux://guide/memory)
