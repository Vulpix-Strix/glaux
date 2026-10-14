# t

Returns logical time in seconds, which is `frame()` divided by the tick rate of 60.

## Returns

A number. It is 0 during the entry chunk, `boot()` and the first `tick()` and `draw()`.

## Examples

### Luau

```luau
local angle = t() * 2
```

### Rhai

```rhai
let angle = t() * 2.0;
```

## See also

- [frame](glaux://api/time/frame)
