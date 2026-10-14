# draw

An optional callback that runs right after [tick](glaux://api/lifecycle/tick) on every logical tick, and draws the frame. It is the only place the drawing functions may be called.

## Notes

Glaux calls `draw()` with no parameters and ignores its result. The commands it records, in call order, become the frame the screen shows. The frame is complete only when `draw()` returns normally. One that fails leaves the previous frame, and the cartridge stops.

Each call starts with an empty recording, so draw the whole frame every time. Begin with [cls](glaux://api/graphics/cls) to choose the background. A cartridge without a `draw` presents an empty frame.

The drawing functions are [cls](glaux://api/graphics/cls), [pset](glaux://api/graphics/pset), [rectfill](glaux://api/graphics/rectfill), [text](glaux://api/graphics/text) and [spr](glaux://api/graphics/spr). Calling one from the top level, `boot()` or `tick()` fails with `can only be called during draw()`.

One `draw()` may spend a fixed amount of draw cost. A clear, pixel or filled rectangle costs 1, a sprite costs 64 per tile and text costs 16 per character. The limit is 65,536, and going past it is an error. Execution has a separate limit, which every call has.

`draw()` can read the state `tick()` updated. Updating the game inside `draw()` works, but it is better kept in `tick()`.

### Luau

`draw` must be a global function. A `local function draw` is not found and is skipped without a warning. Drawing text uses `print`, which draws in Luau. Write to the log with `log.info`.

### Rhai

`draw` must be a public function without parameters. A `private fn draw` or `fn draw(x)` is an error on the first tick. Drawing text uses `text`, because Rhai's own `print` writes to the log.

Helpers that read `this` are called as `this.draw_sky()`. A helper called as `draw_sky()` cannot see it, so pass it what it needs.

## Examples

### Luau

```luau
function draw()
  cls(0)
  print("Hello, Glaux", 8, 8, 7)
end
```

### Rhai

```rhai
fn draw() {
    cls(0);
    text("Hello, Glaux", 8, 8, 7);
}
```

## See also

- [boot](glaux://api/lifecycle/boot)
- [tick](glaux://api/lifecycle/tick)
- [The game loop](glaux://guide/game-loop)
