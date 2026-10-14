# log.trace

Records a message at trace level, for the most detailed output.

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
log.trace("started")
```

### Rhai

```rhai
log::trace("started");
```

## See also

- [log](glaux://api/logging/index)
