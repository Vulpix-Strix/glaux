# peek32

Reads a 32-bit value from four consecutive bytes, low byte first.

## Parameters

| Name | Meaning |
|------|---------|
| `address` | An address from 0 to 1048575 (`0xFFFFF`). |

## Returns

The value, from 0 to 4294967295.

## Notes

The bytes may belong to different devices. The range must fit in the address space, so the last valid address is 1048572 (`0xFFFFC`).

## Examples

### Luau

```luau
local word = peek32(0)
```

## See also

- [peek](glaux://api/memory/peek)
- [peek16](glaux://api/memory/peek16)
- [poke32](glaux://api/memory/poke32)
- [Memory](glaux://guide/memory)
