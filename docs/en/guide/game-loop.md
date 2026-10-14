# The game loop

A Glaux cartridge is a script that defines up to three callbacks, and Glaux calls them in a fixed order. This guide explains when each one runs, where state lives between calls, and where Luau and Rhai differ.

## The order of events

When a cartridge runs, Glaux does this:

1. Runs the script's top level once. Functions and constants are defined here.
2. Calls [boot](glaux://api/lifecycle/boot) once, if the script defines it.
3. Then, 60 times per second, calls [tick](glaux://api/lifecycle/tick) and right after it [draw](glaux://api/lifecycle/draw), if the script defines them.

| Callback | Runs                                  | Use it for                                  | Drawing allowed |
|----------|---------------------------------------|---------------------------------------------|-----------------|
| `boot()` | once, before the first tick           | creating the starting state                 | no              |
| `tick()` | every tick, before `draw()`           | reading input, moving things, game rules    | no              |
| `draw()` | every tick, after `tick()`            | drawing the whole frame from the state      | yes             |

All three are optional and none takes parameters or returns a value. A cartridge with only a `draw()` is valid, and so is one with no callbacks at all, although it shows nothing.

## Logical time

The loop counts ticks, not host frames. [frame](glaux://api/time/frame) is the number of ticks that completed before the current one, and [t](glaux://api/time/t) is that count in seconds. Both are 0 during the top level, `boot()` and the first `tick()` and `draw()`. Because they never read the host clock, a cartridge behaves the same on any machine.

Input is sampled once per tick. [btn](glaux://api/input/btn) and [btnp](glaux://api/input/btnp) give the same answer in `tick()` and in the `draw()` that follows it.

## Keeping state

Nothing is reset between ticks. The script runs in one persistent VM, so whatever the callbacks store survives until the cartridge stops.

### Luau

Keep state in globals or in `local` variables declared at the top level. Callbacks are closures over those locals.

```luau
local score = 0

function tick()
  if btnp(Button.A) then score = score + 1 end
end

function draw()
  cls(0)
  print(score, 8, 8, 7)
end
```

Callbacks must be global functions. `local function tick()` creates a function Glaux cannot see, and it is skipped silently. Glaux looks the callback up again on every call, so a script may replace it while running.

### Rhai

A Rhai function cannot see the variables of the top level that defines it, so `let score = 0;` at the top is not available inside `tick()`. State lives in `this`, an object map that starts empty and is shared by `boot()`, `tick()` and `draw()` for the whole run.

```rhai
fn boot() {
    this.score = 0;
}

fn tick() {
    if btnp(Button::A) { this.score += 1; }
}

fn draw() {
    cls(0);
    text(this.score, 8, 8, 7);
}
```

Constants are the exception. A top-level `const` is read inside functions as `global::NAME`.

`this` is bound in the callbacks and in functions they call as methods, such as `this.draw_sky()`. A plain call such as `draw_sky()` does not bind it, and neither does a function of an imported module, so pass those the state they need and store what they return.

Callbacks must be public and take no parameters. Glaux finds them once, when the cartridge starts.

## Modules

A module's top level runs once, the first time it is loaded, inside the call that loads it. See [require](glaux://api/modules/require) for Luau and [import](glaux://api/modules/import) for Rhai. Define the callbacks in the entry script. In Rhai, only the entry script's functions can be callbacks. In Luau a module shares the entry script's globals, so a global `tick` defined there would replace yours, which is best avoided.

## When something goes wrong

An error in the top level or in `boot()` stops the cartridge from starting. An error in `tick()` or `draw()` stops it on that tick. If `tick()` fails, `draw()` does not run for that tick, and a `draw()` that fails does not replace the last completed frame. The error is reported in the shell with the script line.

After an error the cartridge is not resumed, because the script may have stopped halfway through updating its state. Fix the script and run it again.

## Limits

Every call into the script has its own execution limit: the top level, `boot()`, each `tick()` and each `draw()`. Running out stops the cartridge with `Execution limit exceeded`, and a `pcall` or `try` cannot catch it.

### Luau

The limit counts loop iterations and function calls, about 5 million per call. Straight-line code is free.

### Rhai

The limit counts operations, about 1.5 million per call. Every statement and expression costs at least one, so even code without loops adds up.

`draw()` also has a draw cost limit. See [draw](glaux://api/lifecycle/draw). In Rhai, strings, arrays and object maps have size limits, and `this` is checked after every callback, so state cannot grow without bound.

## Common mistakes

| Mistake                                     | What happens                                             |
|---------------------------------------------|----------------------------------------------------------|
| Drawing in `tick()` or `boot()`             | Error: `can only be called during draw()`                |
| `local function draw()` in Luau             | The callback is never called                             |
| Top-level `let` used inside a Rhai function | The variable is not found; use `this` or a `const`       |
| `private fn tick()` or `fn tick(x)` in Rhai | An error when the callback is called                     |
| Expecting `draw()` to keep the last frame   | It starts empty, so draw everything and begin with `cls` |

## See also

- [Getting started](glaux://guide/getting-started)
- [boot](glaux://api/lifecycle/boot), [tick](glaux://api/lifecycle/tick) and [draw](glaux://api/lifecycle/draw)
