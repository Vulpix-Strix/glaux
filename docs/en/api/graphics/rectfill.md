# rectfill

Fills a rectangle with a palette color.

## Parameters

| Name     | Meaning                                                                        |
|----------|--------------------------------------------------------------------------------|
| `x`      | Column of the top-left corner.                                                 |
| `y`      | Row of the top-left corner.                                                    |
| `width`  | Width in pixels, a positive whole number. It is a size, not an end coordinate. |
| `height` | Height in pixels, a positive whole number.                                     |
| `color`  | A palette index from 0 to 255.                                                 |

## Notes

Drawing functions may only be called during `draw()`. Everything is clipped to the 320 by 240 screen. Like `cls`, `rectfill` costs one draw unit however large it is.

## Examples

### Luau

```luau
rectfill(10, 10, 100, 4, 3)
```

### Rhai

```rhai
rectfill(10, 10, 100, 4, 3);
```

## See also

- [cls](glaux://api/graphics/cls)
