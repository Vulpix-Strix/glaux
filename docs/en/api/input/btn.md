# btn

Reports whether a button is held on the current tick.

## Parameters

| Name     | Meaning                                                                                                                                   |
|----------|-------------------------------------------------------------------------------------------------------------------------------------------|
| `button` | A [Button](glaux://api/input/buttons) constant. Anything else, including a name as a string or a constant of the other kind, is an error. |

## Returns

`true` or `false`. Always `false` before the first tick.

## Examples

### Luau

```luau
if btn(Button.A) then
  jump()
end
```

### Rhai

```rhai
if btn(Button::A) {
    jump();
}
```

## See also

- [btnp](glaux://api/input/btnp)
