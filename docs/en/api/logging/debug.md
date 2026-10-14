# log.debug

Records a message at debug level.

## Parameters

| Name    | Meaning                                         |
|---------|-------------------------------------------------|
| `value` | What to record. Any value is converted to text. |

## Notes

### Luau

Accepts any number of values. Rhai has no equivalent, because `debug` is a keyword there; use Rhai's built-in `debug(value)`.

## Examples

### Luau

```luau
log.debug("started")
```

## See also

- [log](glaux://api/logging/index)
