# GLAUX G1

*The computer that wanted you to make games.*

### Est. 1993 · A different timeline

In 1993, while the world was busy deciding what the future of video games should look like, a rather unusual machine appeared.

It was called the **Glaux G1**.

It didn't promise realistic graphics, revolutionary hardware, or an endless library of games.

Instead, it came with something different: the tools to make your own.

A screen, a keyboard, a few colors, four channels of sound, and a blank cartridge.

That was all you needed.

The G1 never conquered the market. As the industry moved toward 3D graphics and increasingly powerful hardware, the little machine slowly faded into obscurity.

But perhaps there was something worth preserving.

---

## Welcome back to Glaux

Glaux is a modern, open-source fantasy console inspired by an imaginary machine from an alternate history of computing.

It's a self-contained environment for creating, editing, running, and debugging small games.

No external game engine required.

### The machine

| Specification | Glaux G1 |
|---|---|
| Display | 320 × 240 pixels |
| Update rate | 60 Hz |
| Master palette | 256 colors |
| Audio | 4-channel PSG |
| Cartridges | Script-based games with optional assets |
| Programming | Luau, experimental Rhai |
| Development tools | Code, sprite, map, sound and music workspaces |

### Your first cartridge

Glaux starts in its own command shell.

Create a cartridge:

```text
new my-game
```

Run it:

```text
run
```

Or open the integrated editor and write your own game.

```luau
local x = 156

function tick()
    if btn(Button.Left) then x = x - 2 end
    if btn(Button.Right) then x = x + 2 end
end

function draw()
    cls(0)
    rectfill(x, 116, 8, 8, 7)
end
```

That's a game running on the Glaux G1.

Well, almost.

You'll probably want to add a few more things.

## Under the hood

Outside its fictional history, Glaux is a real software project written in Rust, using raylib for rendering and audio.

The console includes an integrated development environment, cartridge execution, debugging tools, and documentation.

Glaux is currently under active development. Features, APIs, and cartridge compatibility may change.

## License

Glaux is licensed under MIT OR Apache-2.0.

---

*Glaux G1. The future we imagined in 1993.*