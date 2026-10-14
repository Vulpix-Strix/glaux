# keyp

Reports whether a key was pressed on the current tick.

## Parameters

| Name  | Meaning                                                                                                                             |
|-------|-------------------------------------------------------------------------------------------------------------------------------------|
| `key` | A [Key](glaux://api/input/keys) constant. Anything else, including a name as a string or a constant of the other kind, is an error. |

## Returns

`true` or `false`. Always `false` before the first tick.

## Examples

### Luau

```luau
if keyp(Key.A) then
  jump()
end
```

### Rhai

```rhai
if keyp(Key::A) {
    jump();
}
```

## See also

- [key](glaux://api/input/key)
