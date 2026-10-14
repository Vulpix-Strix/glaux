//! Persistent Luau runtime for cartridge scripts.
//!
//! A [`Runtime`] owns one VM for the whole time a cartridge runs. Starting it
//! executes the entry chunk once and calls the script's optional `boot()`;
//! each [`Runtime::tick`] takes that tick's [`InputState`] and calls the
//! optional `tick()` and then `draw()`.
//!
//! Scripts see Luau's safe standard library, plus:
//!
//! - `require(name)`, which loads a cartridge module through the
//!   [`ModuleSource`] given to [`Runtime::start`]. `name` is a
//!   [`ModuleName`], as in `require("game.enemies")` for
//!   `code/game/enemies.luau`. Nothing else, such as host paths, can be
//!   named. A module runs at most once per runtime, in the same VM and with
//!   the same globals as the entry chunk, and its first return value (`true`
//!   if it returns nothing) is cached and returned to every later `require`.
//!   A module that failed raises the same error on every later `require`.
//!   Requiring a module that is still loading is a cyclic require and
//!   fails. A module runs inside the call that first requires it, so it
//!   counts against that call's [`Budget`];
//! - `log.trace(...)`, `log.debug(...)`, `log.info(...)` and `log.warn(...)`,
//!   which record [`LogEntry`]s in any phase;
//! - `cls(color)`, `pset(x, y, color)`, `rectfill(x, y, width, height,
//!   color)`, `print(text, x, y, color)` and
//!   `spr(tile, x, y, [w, h, flip_x, flip_y, palette])`,
//!   which record [`DrawCommand`]s and may only be called during `draw()`.
//!   `rectfill` is a filled rectangle primitive: `(x, y)` is its top-left
//!   corner and `width` and `height` are positive integer sizes in pixels,
//!   not end coordinates. Like every drawing function it is clipped to the
//!   screen, and like `cls` it costs one draw unit however large it is.
//!   `spr` draws a `w`x`h` block of 8x8 object tiles (default 1x1) taken
//!   from the 16-column object-tile grid, with `tile` at its top-left. The
//!   block may not run past the grid's right or bottom edge. `flip_x` and
//!   `flip_y` are optional booleans (default `false`) that mirror the whole
//!   composed block: `flip_x` reverses both the order of its tile columns
//!   and the pixels within each tile, and `flip_y` does the same for rows.
//!   Flipping does not change a sprite's draw cost. `palette` is an optional
//!   object palette bank that every tile of the block is drawn through. When
//!   it is omitted or `nil`, each tile uses its default bank. Transparent
//!   index 0 is skipped;
//! - `Button` and `Key`, read-only tables of opaque constants: the virtual
//!   G1 buttons (`Button.Up`, `Button.Down`, `Button.Left`, `Button.Right`,
//!   `Button.A`, `Button.B`) and physical keyboard keys (`Key.A`, `Key.Num0`,
//!   `Key.F1`, `Key.Space`, `Key.PageUp`, `Key.LeftShift`, `Key.Up` and so on;
//!   see [`Key`]). `tostring` gives `Button.A` or `Key.F2`, and `typeof`
//!   gives `Button` or `Key`;
//! - `btn(button)` and `btnp(button)`, which report whether a `Button` is
//!   held, or was pressed, on the current tick, and `key(key)` and
//!   `keyp(key)`, which do the same for a `Key`. Any other argument,
//!   including a name as a string or a constant of the other kind, is a
//!   runtime error. All four return `false` before the first tick. Which
//!   physical input drives each `Button` is up to the host;
//! - `frame()` and `t()`, which report logical time in any phase: `frame()`
//!   is the number of ticks completed before the current one, and `t()` is
//!   that count in seconds, `frame() / TICK_RATE`. Both are `0` during the
//!   entry chunk, `boot()` and the first `tick()`/`draw()` pair, and advance
//!   only after a tick whose `tick()` and `draw()` both succeed. They never
//!   depend on host wall-clock time or presentation frames.
//! - `audio.tone(channel, frequency, volume, waveform)`,
//!   `audio.env(channel, attack_ms, decay_ms, sustain, release_ms)` and
//!   `audio.gate(channel, enabled)`, which write the cartridge's
//!   [`AudioState`] in any phase. Channels are `0` to `3`, frequency is in
//!   Hz, volume and sustain run from `0` to `15`, envelope times are in
//!   milliseconds, and `waveform` is `"square"`, `"triangle"`, `"saw"` or
//!   `"noise"`. Every number must be an integer within its range. `tone`
//!   changes only the oscillator. `gate(channel, true)` opens the gate, or
//!   retriggers it if already open, and `gate(channel, false)` releases it.
//!
//! [`InputState`]: glaux_input::InputState
//! [`Key`]: glaux_input::Key
//! [`ModuleSource`]: glaux_modules::ModuleSource
//! [`ModuleName`]: glaux_modules::ModuleName
//! [`AudioState`]: glaux_audio::AudioState

mod api;
mod args;
mod error;
mod modules;
mod runtime;

pub use glaux_script::{
    DrawCommand, LogEntry, LogLevel, MAX_DRAW_COMMANDS, Phase, RequireError, RequireFailure,
    RuntimeError, SPRITE_TILE_COST, ScriptError, StartFailure, TEXT_CHAR_COST,
};
pub use runtime::{Budget, DEFAULT_INTERRUPT_BUDGET, Runtime};

#[cfg(test)]
mod tests;
