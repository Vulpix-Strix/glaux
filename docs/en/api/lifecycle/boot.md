# boot

An optional callback that runs once, when the cartridge starts, before the first tick. Use it to set up the state the cartridge needs.

## Notes

Glaux calls `boot()` after the script's top level has run and before the first [tick](glaux://api/lifecycle/tick). It takes no parameters and its result is ignored. A cartridge without a `boot` simply skips this step.

During `boot()`, [frame](glaux://api/time/frame) and [t](glaux://api/time/t) are 0, and the input functions report nothing pressed. Logging, audio and module loading work as usual. Drawing functions do not, because they may only be called during [draw](glaux://api/lifecycle/draw), and calling one fails.

An error in `boot()` means the cartridge never starts. Like the top level, it runs under its own execution limit, separate from every tick.

### Luau

`boot` must be a global function. A `local function boot` is not found and is skipped without a warning. Setting `boot` to something that is not a function is an error.

State can live in globals or in locals declared at the top level, which the callbacks can all see.

### Rhai

`boot` must be a public function without parameters. A `private fn boot` or `fn boot(x)` fails the start with an error.

`this` starts as an empty object map, so `boot()` is where fields are first assigned. Variables declared with `let` at the top level are not visible inside functions, so keep state in `this`. Constants are read as `global::NAME`.

## Examples

### Luau

```luau
local stars = {}

function boot()
  for i = 1, 40 do
    stars[i] = { x = (i * 73) % 320, y = (i * 37) % 120 }
  end
end
```

### Rhai

```rhai
fn boot() {
    this.stars = [];
    for i in 1..=40 {
        this.stars.push(#{ x: (i * 73) % 320, y: (i * 37) % 120 });
    }
}
```

## See also

- [tick](glaux://api/lifecycle/tick)
- [draw](glaux://api/lifecycle/draw)
- [The game loop](glaux://guide/game-loop)
