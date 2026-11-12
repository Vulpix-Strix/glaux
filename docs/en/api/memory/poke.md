# poke

Writes one byte to memory.

## Parameters

| Name | Meaning |
|------|---------|
| `address` | An address from 0 to 1048575 (`0xFFFFF`). |
| `value` | A byte from 0 to 255. |

## Notes

An address outside the address space, a value outside its range and a fraction are errors, and nothing is written. A write to an address that no device uses is ignored. Each device applies its own rules, so some bits of the value may be dropped. Pixels written to the framebuffer before `draw()` are cleared when `draw()` starts.

## Examples

### Luau

```luau
poke(0x10000, 7)
```

## See also

- [peek](glaux://api/memory/peek)
- [poke16](glaux://api/memory/poke16)
- [poke32](glaux://api/memory/poke32)
- [memset](glaux://api/memory/memset)
- [Memory](glaux://guide/memory)
