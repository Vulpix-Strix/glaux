# Signal Lost (Rhai)

A Rhai port of [Signal Lost](../signal-lost/README.md), the arena survival
shooter. It plays the same game with the same rules, timing, difficulty
curve, sprites, palettes and sounds. Controls, objective and assets are
described in the original's README; this one covers what is specific to the
port.

> **Rhai support is experimental.** Glaux's Rhai scripting API may change
> without backward compatibility guarantees, and this cartridge may need
> changes to keep running on later Glaux versions.

## Running it

Glaux loads cartridges from its data directory (`~/.local/share/glaux/cartridges`
on Linux). Install the example there, start Glaux and load it from the shell:

```sh
mise run sync-examples -- signal-lost-rhai
mise run glaux
```

```text
load signal-lost-rhai
run
```

Glaux shows a warning that Rhai is experimental each time the cartridge runs.

## Relationship to the Luau version

The cartridge is a separate copy; `examples/signal-lost` is unchanged.

- **Assets** (`sprites.png`, `sprites.bin`, `objattr.bin`, `obp.bin`,
  `palette.bin`, `sprites.import.ron`, `sprites.sprites.ron`) are byte-for-byte
  copies of the original's. A test fails if they drift apart, so rebuild or
  reimport them in `examples/signal-lost` and copy them over.
- **`code/sheet.rhai`** is a hand copy of the generated `sheet.luau`.
  `tools/build_sheet.py` only writes the Luau file; after rebuilding the sheet,
  update `sheet.rhai` to match.
- **Modules** keep the original's split and names (`arena`, `audio`,
  `enemies`, `particles`, `pickups`, `player`, `projectiles`, `sheet`, `util`).
  `audio.rhai` is imported as `sfx`, as `audio.luau` is required as `sfx`,
  because `audio` names Glaux's sound API.
- **Behaviour is identical.** Given the same input, both versions draw the
  same commands, write the same audio and log the same lines on every tick:
  `the_rhai_port_plays_in_lockstep_with_the_original` in
  `apps/glaux/src/system/running/tests.rs` checks this over 7,000 scripted
  ticks that include an ogre wave, game overs and restarts. The xorshift PRNG
  produces the same stream, so runs with the same inputs match too.

## How the port is written

Rhai differs from Luau in ways that shape the code:

- **State lives in `this`.** `boot()`, `tick()` and `draw()` share one object
  map. Helpers in `main.rhai` that change game state are methods called on it
  (`this.start_run()`, `this.burst(..)`, `this.play(..)`).
- **Module functions get copies.** Maps and arrays are passed by value, and a
  module function cannot reach `this`, so module functions return what they
  change: `this.run.sparks = particles::update(this.run.sparks);`. Functions
  with several results return a map, such as `player::update` returning
  `#{ player, dashed, fire }`.
- **Modules hold no mutable state.** Exported module values are read-only, so
  the sweeps that `audio.luau` keeps in a module-level table live in
  `this.sfx` and are threaded through `sfx::play`, `sfx::update` and
  `sfx::silence`.
- **The PRNG is a plain integer** in `this.rng`. Code that draws from it
  advances it explicitly (`rng = util::rng_next(rng)`). The two module
  functions that need randomness, `particles::burst` and
  `enemies::advance_spawns`, take the state and return it. Draws happen in the
  original's order, so both versions share the stream.
- **Loops that change list entries use indices** (`list[i].x += ..`), because
  `for e in list` iterates over copies. `util.sweep` becomes `retain`.
- **Division is explicit.** Luau's `/` always yields a float, and its `//`
  floors. The port writes float division with floats (`w.to_float() / 2.0`).
  It uses Rhai's integer `/`, which truncates, only where Luau's `//` had two
  non-negative integers, where truncating and flooring agree.
- **Printed numbers stay integers.** Rhai shows `150.0` for a float, so
  scores, counters and times are kept as integers (`.floor().to_int()`).
- **Small renames:** `print` for drawing text becomes `text`.
  `string.format` becomes string interpolation and `util::pad`. `bit32` becomes
  `^`, `<<` and `>>` masked to 32 bits. `new` and `spawn` are reserved words
  in Rhai, so constructors are called `create` and `enemies.spawn` is
  `enemies::advance_spawns`. Constants are uppercase (`sheet::KNIGHT_RUN`) and
  are read as `global::NAME` inside their own module's functions.
- **Constants that seed mutable state go through `util::writable`.** In
  Rhai 1.26, a value read as `global::NAME` or `module::NAME` stays constant
  when stored directly in a map field, so a later `this.lock -= 1` would fail
  with "Cannot modify constant". Passing it through a parameter gives a
  writable copy.
- **Enemies carry their hitbox.** Each enemy copies its kind's hitbox when it
  spawns, so the separation pass, which compares every pair of enemies, does
  not look the kind up for each pair. The numbers are the same.

## Differences from the Luau version

- **Debug logging.** `debug` is a keyword in Rhai, so there is no
  `log::debug`. The ogre announcement uses Rhai's `debug(..)`, which logs at
  debug level but shows strings quoted: `"an ogre entered at 00:52"`.
- **Multi-argument logging.** Luau's `log.trace("rng state", n)` joins its
  arguments with a tab; Rhai's `log::trace` takes one value, so the port
  builds the same text itself.
- **Cost.** The Rhai version takes about five times longer per tick than the
  Luau one (about 0.6 ms against 0.1 ms in a release build). Its busiest
  `tick()` uses about 2% of the runtime's per-call operation budget.

Everything else, from spawns and knockback to flashes, pickups, sounds, the
HUD and the title and game-over screens, plays as in the original.

## Runtime findings

Porting the game uncovered two Rhai behaviours that matter to Glaux
cartridges:

- **Float comparisons.** Rhai's built-in comparison operators allow a relative
  epsilon, so `29.999999999999996 < 30` was `false` and the knight's clamp
  to the arena's top edge sometimes left it one pixel outside. `glaux-rhai`
  now registers exact comparisons for floats, as Luau has.
- **Constants stay read-only when stored.** This is the issue that
  `util::writable` works around (see above). It is Rhai's own behaviour and
  is documented in `glaux-rhai`.
