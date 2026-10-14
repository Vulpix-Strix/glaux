# Glaux script API

Everything a cartridge script can call, in Luau and in Rhai. The two languages share the same functions with a few differences in spelling, which each page notes.

## Drawing

[cls](glaux://api/graphics/cls), [pset](glaux://api/graphics/pset), [rectfill](glaux://api/graphics/rectfill), [text](glaux://api/graphics/text) and [spr](glaux://api/graphics/spr). Drawing functions may only be called during `draw()`.

## Input

[Button](glaux://api/input/buttons) and [Key](glaux://api/input/keys) constants, read with [btn](glaux://api/input/btn), [btnp](glaux://api/input/btnp), [key](glaux://api/input/key) and [keyp](glaux://api/input/keyp).

## Sound

The [audio](glaux://api/audio/index) functions: [tone](glaux://api/audio/tone), [env](glaux://api/audio/env) and [gate](glaux://api/audio/gate).

## Time, logging and modules

[frame](glaux://api/time/frame) and [t](glaux://api/time/t) report logical time. [log](glaux://api/logging/index) records messages. [require](glaux://api/modules/require) and [import](glaux://api/modules/import) load cartridge modules.

## Differences between the languages

| Luau                                  | Rhai                                  |
|---------------------------------------|---------------------------------------|
| `print(text, x, y, color)` draws text | `text(value, x, y, color)` draws text |
| `audio.tone(...)`                     | `audio::tone(...)`                    |
| `Button.A`                            | `Button::A`                           |
| `require("game.enemies")`             | `import "game.enemies" as enemies;`   |
| `log.debug(...)`                      | no `log::debug`; use `debug(value)`   |
| scripts keep state in globals         | callbacks keep state in `this`        |
