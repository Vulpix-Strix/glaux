# memset

Fills a block of bytes with one value.

## Parameters

| Name | Meaning |
|------|---------|
| `destination` | The first address written. |
| `value` | A byte from 0 to 255. |
| `length` | The number of bytes, from 0 to 1048576. |

## Notes

The block must fit in the address space, or nothing is written. A length of 0 changes nothing but still needs a valid address. Every byte follows the rules of its device. The call is charged to the execution budget in proportion to its length. See [Memory](glaux://guide/memory).

## Examples

### Luau

```luau
memset(0x00000, 0, 0x10000)
```

## See also

- [memcpy](glaux://api/memory/memcpy)
- [poke](glaux://api/memory/poke)
- [peek](glaux://api/memory/peek)
- [Memory](glaux://guide/memory)
