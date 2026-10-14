# Button

The virtual buttons of the Glaux G1 controller: `Up`, `Down`, `Left`, `Right`, `A` and `B`.

## Notes

Buttons are opaque constants. Pass them to [btn](glaux://api/input/btn) and [btnp](glaux://api/input/btnp), not strings or numbers. Which physical input drives each button is up to the host.

### Luau

Write `Button.A`. `tostring(Button.A)` gives `Button.A` and `typeof(Button.A)` gives `Button`. The table is read-only.

### Rhai

Write `Button::A`. `to_string` gives `Button::A` and `type_of` gives `Button`.

## Examples

### Luau

```luau
if btn(Button.Left) then
  x = x - 1
end
```

### Rhai

```rhai
if btn(Button::Left) {
    this.x -= 1;
}
```

## See also

- [btn](glaux://api/input/btn)
- [btnp](glaux://api/input/btnp)
