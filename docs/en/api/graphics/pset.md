# pset

Sets one pixel to a palette color.

## Parameters

| Name    | Meaning                                       |
|---------|-----------------------------------------------|
| `x`     | Column in pixels. Fractions are rounded down. |
| `y`     | Row in pixels. Fractions are rounded down.    |
| `color` | A palette index from 0 to 255.                |

## Notes

Drawing functions may only be called during `draw()`. Everything is clipped to the 320 by 240 screen. A point outside the screen is ignored, however far away it is.

## Examples

### Luau

```luau
pset(160, 120, 7)
```

### Rhai

```rhai
pset(160, 120, 7);
```

## See also

- [rectfill](glaux://api/graphics/rectfill)
