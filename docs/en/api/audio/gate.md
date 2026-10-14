# audio.gate

Opens or releases the gate of a channel. Opening an open gate retriggers the envelope.

## Parameters

| Name      | Meaning                                        |
|-----------|------------------------------------------------|
| `channel` | Channel number, from 0 to 3.                   |
| `enabled` | `true` opens the gate and `false` releases it. |

## Examples

### Luau

```luau
audio.gate(0, true)
audio.gate(0, false)
```

### Rhai

```rhai
audio::gate(0, true);
audio::gate(0, false);
```

## See also

- [tone](glaux://api/audio/tone)
- [env](glaux://api/audio/env)
