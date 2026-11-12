# peek16

Reads a 16-bit value from two consecutive bytes, low byte first.

## Parameters

| Name | Meaning |
|------|---------|
| `address` | An address from 0 to 1048575 (`0xFFFFF`). |

## Returns

The value, from 0 to 65535.

## Notes

The two bytes may belong to different devices. The range must fit in the address space, so the last valid address is 1048574 (`0xFFFFE`).

## Examples

### Luau

```luau
local scroll = peek16(0x3C010)
```

## See also

- [peek](glaux://api/memory/peek)
- [poke16](glaux://api/memory/poke16)
- [peek32](glaux://api/memory/peek32)
- [Memory](glaux://guide/memory)
