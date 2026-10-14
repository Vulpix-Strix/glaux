# audio.tone

Sets the oscillator of a channel. It changes only the oscillator, not the envelope or the gate.

## Parameters

| Name        | Meaning                                                |
|-------------|--------------------------------------------------------|
| `channel`   | Channel number, from 0 to 3.                           |
| `frequency` | Pitch in hertz, a whole number from 0 to 24000.        |
| `volume`    | Loudness, a whole number from 0 to 15.                 |
| `waveform`  | One of `"square"`, `"triangle"`, `"saw"` or `"noise"`. |

## Notes

Every number must be a whole number inside its range.

## Examples

### Luau

```luau
audio.tone(0, 440, 12, "square")
audio.gate(0, true)
```

### Rhai

```rhai
audio::tone(0, 440, 12, "square");
audio::gate(0, true);
```

## See also

- [env](glaux://api/audio/env)
- [gate](glaux://api/audio/gate)
