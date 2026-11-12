# memcpy

Copies a block of bytes from one address to another.

## Parameters

| Name | Meaning |
|------|---------|
| `destination` | The first address written. |
| `source` | The first address read. |
| `length` | The number of bytes, from 0 to 1048576. |

## Notes

The copy behaves as if the source were read completely before anything is written, so the two blocks may overlap. Both blocks must fit in the address space, or nothing is written. A length of 0 changes nothing but still needs valid addresses. Every byte follows the rules of its device. The call is charged to the execution budget in proportion to its length, so a script that copies too much in one call is stopped like one that loops too long. See [Memory](glaux://guide/memory).

## Examples

### Luau

```luau
memcpy(0x24000, 0x00000, 256)
```

## See also

- [memset](glaux://api/memory/memset)
- [peek](glaux://api/memory/peek)
- [poke](glaux://api/memory/poke)
- [Memory](glaux://guide/memory)
