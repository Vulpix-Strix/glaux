# tick

An optional callback that runs once per logical tick, 60 times a second, to update the game. It always runs before [draw](glaux://api/lifecycle/draw).

## Notes

Glaux calls `tick()` with no parameters and ignores its result. Read input with [btn](glaux://api/input/btn), [btnp](glaux://api/input/btnp), [key](glaux://api/input/key) and [keyp](glaux://api/input/keyp). They report the input of the current tick, and keep reporting it during `draw()`.

Time is logical. [frame](glaux://api/time/frame) counts ticks completed before the current one, so it is 0 during the first `tick()` and the first `draw()`. It advances only after a tick whose `tick()` and `draw()` both succeed, and never depends on the host clock.

Move and update things here, and leave `draw()` to show them. Drawing functions fail inside `tick()`.

If `tick()` fails, `draw()` is skipped for that tick and the cartridge stops. Every call runs under its own execution limit, so a long loop in one tick does not eat into the next. A cartridge without a `tick` still draws every tick.

### Luau

`tick` must be a global function. A `local function tick` is not found and is skipped without a warning. Glaux looks the function up again on every tick, so a script may replace it while running.

### Rhai

`tick` must be a public function without parameters, and it is found once, when the cartridge starts. A `private fn tick` or `fn tick(x)` is an error on the first tick. A later `tick` cannot be added.

`this` is the same object map that `boot()` and `draw()` see. Call helper functions as `this.helper()` when they need it. A plain `helper()` call does not bind `this`.

## Examples

### Luau

```luau
x = 10

function tick()
  if btn(Button.Right) then x = x + 1 end
  if btnp(Button.A) then log.info("jump at frame", frame()) end
end
```

### Rhai

```rhai
fn boot() {
    this.x = 10;
}

fn tick() {
    if btn(Button::Right) { this.x += 1; }
    if btnp(Button::A) { log::info(`jump at frame ${frame()}`); }
}
```

## See also

- [boot](glaux://api/lifecycle/boot)
- [draw](glaux://api/lifecycle/draw)
- [The game loop](glaux://guide/game-loop)
