# log.info

Records a message at info level.

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
log.info("started")
```

### Rhai

```rhai
log::info("started");
```

## See also

- [log](glaux://api/logging/index)
