# log.warn

Records a message at warn level, for something that looks wrong but does not stop the cartridge.

## Parameters

| Name    | Meaning                                         |
|---------|-------------------------------------------------|
| `value` | What to record. Any value is converted to text. |

## Notes

### Luau

Accepts any number of values.

## Examples

### Luau

```luau
log.warn("started")
```

### Rhai

```rhai
log::warn("started");
```

## See also

- [log](glaux://api/logging/index)
