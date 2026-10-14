# audio

Controls the four channels of the sound chip. Channels are numbered 0 to 3, and the functions work in any phase.

## Notes

Write a tone with [tone](glaux://api/audio/tone), shape it with [env](glaux://api/audio/env) and start or stop it with [gate](glaux://api/audio/gate).

### Luau

The functions are fields of the `audio` table: `audio.tone(...)`.

### Rhai

The functions live in the `audio` module: `audio::tone(...)`.

## See also

- [tone](glaux://api/audio/tone)
- [env](glaux://api/audio/env)
- [gate](glaux://api/audio/gate)
