//! Persistent Rhai runtime for cartridge scripts. **Experimental:** the
//! scripting API may change without backward compatibility guarantees.
//!
//! A [`Runtime`] owns one engine and one compiled script for the whole time
//! a cartridge runs. Starting it runs the script's top level once and calls
//! its optional `boot()`. Each [`Runtime::tick`] takes that tick's
//! [`InputState`] and calls the optional `tick()` and then `draw()`. All
//! three callbacks take no parameters.
//!
//! # State
//!
//! Rhai functions cannot see variables of the top level that defines them,
//! so callbacks keep their state in `this`: one object map, empty at start,
//! bound as `this` in every `boot()`, `tick()` and `draw()` call for the
//! whole run:
//!
//! ```text
//! fn boot() { this.x = 10; }
//! fn tick() { if btn(Button::Right) { this.x += 1; } }
//! fn draw() { cls(0); pset(this.x, 20, 7); }
//! ```
//!
//! `this` is bound only in those callbacks and in functions they call as
//! methods (`this.step()`). Functions of an imported module are called
//! through the module's namespace, never as methods of `this`, so they
//! cannot reach it. Using `this` there is an error. Pass them the state they
//! need and store what they return, as in
//! `this.player = player::step(this.player);`. Object maps are passed by
//! value, so a module function's changes to its argument are lost unless it
//! returns them.
//!
//! # Glaux API
//!
//! Besides Rhai's standard library, without `eval`, scripts see:
//!
//! - `import "name" as alias;`, which loads a cartridge module through the
//!   [`ModuleSource`] given to [`Runtime::start`]. `name` is a
//!   [`ModuleName`], as in `import "game.enemies" as enemies;` for
//!   `code/game/enemies.rhai`. Nothing else, such as host paths, can be
//!   named. A module's top level runs at most once per runtime and its
//!   functions and exported variables are cached for every later `import`.
//!   A module that failed fails the same way on every later `import`, and
//!   importing a module that is still loading is a cyclic import and fails.
//!   A module runs inside the call that first imports it, so it counts
//!   against that call's [`Budget`];
//! - `log::trace(value)`, `log::info(value)` and `log::warn(value)`, which
//!   record [`LogEntry`]s in any phase, and Rhai's own `print(value)` and
//!   `debug(value)`, which record at info and debug level. `debug` is a
//!   Rhai keyword, so there is no `log::debug`. `debug` uses Rhai's debug
//!   formatting, so strings appear quoted;
//! - `cls(color)`, `pset(x, y, color)`, `rectfill(x, y, width, height,
//!   color)`, `text(value, x, y, color)` and
//!   `spr(tile, x, y, [w, h, flip_x, flip_y, palette])`, which record
//!   [`DrawCommand`]s and may only be called during `draw()`. They behave as
//!   their Luau counterparts do, `text` standing for Luau's `print`, which
//!   Rhai reserves for logging. Optional `spr` arguments may be left out or
//!   given as `()`;
//! - `Button` and `Key`, modules of opaque constants: `Button::A`,
//!   `Button::Up`, `Key::F1`, `Key::Space` and so on (see [`Key`]).
//!   `to_string` gives `Button::A`, and `type_of` gives `Button` or `Key`;
//! - `btn(button)`, `btnp(button)`, `key(key)` and `keyp(key)`, as in Luau.
//!   Any other argument, including a name as a string or a constant of the
//!   other kind, is a runtime error. All four return `false` before the
//!   first tick;
//! - `frame()`, an integer, and `t()`, a float, which report logical time as
//!   in Luau: ticks completed before the current one, and that count in
//!   seconds;
//! - `audio::tone(channel, frequency, volume, waveform)`,
//!   `audio::env(channel, attack_ms, decay_ms, sustain, release_ms)` and
//!   `audio::gate(channel, enabled)`, with the ranges of their Luau
//!   counterparts.
//!
//! Numbers may be integers or floats wherever Luau accepts a number.
//!
//! `==`, `!=`, `<`, `<=`, `>` and `>=` compare floats, and floats with
//! integers, exactly, as Luau does. Rhai's own operators allow a relative
//! epsilon, under which `29.999999999999996 < 30` is `false`.
//!
//! A value read through `global::NAME` or a module's namespace is constant
//! in Rhai 1.26, and a map field, array slot or literal entry given it
//! directly stays constant: `this.lock = global::LOCK; this.lock -= 1;`
//! fails with "Cannot modify constant". Passing the value through a
//! function parameter, or binding it with `let` first, gives a writable
//! copy.
//!
//! # Limits
//!
//! Each call runs under a [`Budget`] of Rhai operations. Running out stops
//! the call with an error that `try`/`catch` cannot catch, including inside
//! a module being imported.
//!
//! Rhai also bounds the depth of function calls and expressions and the size
//! of each single string, array and object map, counting nested arrays' and
//! maps' entries. These bound one value each, not the memory a script holds
//! in total. An array of strings may hold many times the string limit. Rhai
//! checks a value when it is passed to or returned from a function, not when
//! an index assignment grows it, so the runtime also checks `this` after
//! every callback. A local value grown only by index assignment within one
//! call is bounded only by that call's budget.
//!
//! Errors are located as `script:line: message`. Rhai 1.26 raises some
//! errors without a position, among them size errors found on `this`. Those
//! name the script only.
//!
//! Names starting with `__glaux_` are reserved.
//!
//! The same API is described, with parameters and examples, in the
//! repository's `docs/api/` catalog and `docs/en/api/` pages.
//!
//! [`InputState`]: glaux_input::InputState
//! [`Key`]: glaux_input::Key
//! [`ModuleSource`]: glaux_modules::ModuleSource
//! [`ModuleName`]: glaux_modules::ModuleName
//! [`LogEntry`]: glaux_script::LogEntry
//! [`DrawCommand`]: glaux_script::DrawCommand

mod api;
mod args;
mod debug;
mod error;
mod modules;
mod runtime;

pub use runtime::{Budget, DEFAULT_OPERATION_BUDGET, Runtime};

#[cfg(test)]
mod tests;
