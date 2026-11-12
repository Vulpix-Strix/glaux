# peek

Reads one byte from memory.

## Parameters

| Name | Meaning |
|------|---------|
| `address` | An address from 0 to 1048575 (`0xFFFFF`). |

## Returns

The byte, from 0 to 255.

## Notes

An address that no device uses reads as 0. Reading is allowed in every callback and at the top level. A few devices hide some bits, so a byte can read back different from what was written. See [Memory](glaux://guide/memory).

## Examples

### Luau

```luau
local backdrop = peek(0x3C001)
```

## See also

- [poke](glaux://api/memory/poke)
- [peek16](glaux://api/memory/peek16)
- [peek32](glaux://api/memory/peek32)
- [Memory](glaux://guide/memory)
