# Glaux guide

Glaux is a fantasy console. A cartridge is a folder with a manifest, a script and optional assets, and runs on a fixed 320 by 240 screen at 60 ticks per second.

## Where to start

The script API is described in the [Glaux script API](glaux://api/index) reference. Each function has a page with its parameters and examples in both Luau and Rhai.

## Writing a cartridge

A cartridge script defines up to three callbacks. `boot()` runs once, `tick()` runs 60 times a second to update state, and `draw()` runs after it to draw the frame.

```luau
function draw()
  cls(0)
  print("Hello, Glaux", 8, 8, 7)
end
```

```rhai
fn draw() {
    cls(0);
    text("Hello, Glaux", 8, 8, 7);
}
```
