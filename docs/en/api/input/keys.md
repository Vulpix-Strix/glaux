# Key

Physical keyboard keys: the letters `A` to `Z`, `Num0` to `Num9`, `F1` to `F12`, `Space`, `Enter`, `Tab`, `Backspace`, `Insert`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, the left and right `Shift`, `Control` and `Alt` keys, and the arrows `Up`, `Down`, `Left` and `Right`.

## Notes

Keys are opaque constants for [key](glaux://api/input/key) and [keyp](glaux://api/input/keyp). Escape is not a key, because the host reserves it to break out of a running cartridge.

### Luau

Write `Key.PageUp`. `tostring(Key.F2)` gives `Key.F2` and `typeof(Key.F2)` gives `Key`.

### Rhai

Write `Key::PageUp`. `to_string` gives `Key::F2` and `type_of` gives `Key`.

## Examples

### Luau

```luau
if keyp(Key.F1) then
  show_help = not show_help
end
```

### Rhai

```rhai
if keyp(Key::F1) {
    this.show_help = !this.show_help;
}
```

## See also

- [key](glaux://api/input/key)
- [keyp](glaux://api/input/keyp)
