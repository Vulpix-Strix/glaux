# text (Luau: print)

Draws text with the system font. Luau calls this function `print`.

## Parameters

| Name    | Meaning                                             |
|---------|-----------------------------------------------------|
| `value` | What to draw. Any value is converted to text first. |
| `x`     | Column of the first character's top-left corner.    |
| `y`     | Row of the first character's top-left corner.       |
| `color` | A palette index from 0 to 255.                      |

## Notes

Drawing functions may only be called during `draw()`. Everything is clipped to the 320 by 240 screen. Each cell of the system font is 6 by 12 pixels. A character the font cannot draw shows as a placeholder box.

### Luau

Use `log.info` to write to the log. `print` always draws.

## Examples

### Luau

```luau
print("Hello", 8, 8, 7)
```

## See also

- [log](glaux://api/logging/index)
