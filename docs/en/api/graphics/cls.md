# cls

Clears the whole screen to one palette color.

## Parameters

| Name    | Meaning                        |
|---------|--------------------------------|
| `color` | A palette index from 0 to 255. |

## Notes

Drawing functions may only be called during `draw()`. Everything is clipped to the 320 by 240 screen. `cls` costs one draw unit however much it covers.

## Examples

### Luau

```luau
function draw()
  cls(1)
end
```

### Rhai

```rhai
fn draw() {
    cls(1);
}
```

## See also

- [pset](glaux://api/graphics/pset)
- [rectfill](glaux://api/graphics/rectfill)
