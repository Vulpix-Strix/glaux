# Glaux guide

Glaux is a fantasy console. A cartridge is a folder with a manifest, a script and optional assets, and runs on a fixed 320 by 240 screen at 60 ticks per second.

## Where to start

New to Glaux? [Getting started](glaux://guide/getting-started) creates a cartridge and runs it. [The game loop](glaux://guide/game-loop) explains `boot()`, `tick()` and `draw()`, and how state lives between them.

The script API is described in the [Glaux script API](glaux://api/index) reference. Each function has a page with its parameters and examples in both Luau and Rhai.

## Writing a cartridge

A cartridge script defines up to three optional callbacks: [boot](glaux://api/lifecycle/boot) runs once, [tick](glaux://api/lifecycle/tick) runs 60 times a second to update state, and [draw](glaux://api/lifecycle/draw) runs after it to draw the frame.

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
