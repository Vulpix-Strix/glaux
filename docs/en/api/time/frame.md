# frame

Returns how many ticks completed before the current one.

## Returns

The count, starting at 0. It advances only after a tick whose `tick()` and `draw()` both succeed, and never depends on the host clock.

## Notes

### Luau

The result is a number.

## Examples

### Luau

```luau
function tick()
  if frame() % 30 == 0 then blink = not blink end
end
```

## See also

- [t](glaux://api/time/t)
