# audio.env

Sets the volume envelope of a channel.

## Parameters

| Name         | Meaning                                                               |
|--------------|-----------------------------------------------------------------------|
| `channel`    | Channel number, from 0 to 3.                                          |
| `attack_ms`  | Time to rise to full volume, in milliseconds, up to 10000.            |
| `decay_ms`   | Time to fall to the sustain level, in milliseconds, up to 10000.      |
| `sustain`    | Level held while the gate is open, from 0 to 15.                      |
| `release_ms` | Time to fade out after the gate closes, in milliseconds, up to 10000. |

## Examples

### Luau

```luau
audio.env(0, 5, 80, 8, 200)
```

### Rhai

```rhai
audio::env(0, 5, 80, 8, 200);
```

## See also

- [tone](glaux://api/audio/tone)
- [gate](glaux://api/audio/gate)
