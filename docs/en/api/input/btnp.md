# btnp

Reports whether a button was pressed on the current tick.

## Parameters

| Name     | Meaning                                                                                                                                   |
|----------|-------------------------------------------------------------------------------------------------------------------------------------------|
| `button` | A [Button](glaux://api/input/buttons) constant. Anything else, including a name as a string or a constant of the other kind, is an error. |

## Returns

`true` or `false`. Always `false` before the first tick.

## Examples

### Luau

```luau
if btnp(Button.A) then
  jump()
end
```

### Rhai

```rhai
if btnp(Button::A) {
    jump();
}
```

## See also

- [btn](glaux://api/input/btn)
