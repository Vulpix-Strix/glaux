//! Persistent Luau runtime for cartridge scripts.
//!
//! A [`Runtime`] owns one VM for the whole time a cartridge runs. Starting it
//! executes the entry chunk once and calls the script's optional `boot()`;
//! each [`Runtime::tick`] takes that tick's [`ButtonState`] and calls the
//! optional `tick()` and then `draw()`.
//!
//! Scripts see Luau's safe standard library, plus:
//!
//! - `require(name)`, which loads a cartridge module through the
//!   [`ModuleSource`] given to [`Runtime::start`]. `name` is a
//!   [`ModuleName`], as in `require("game.enemies")` for
//!   `code/game/enemies.luau`; nothing else, such as host paths, can be
//!   named. A module runs at most once per runtime, in the same VM and with
//!   the same globals as the entry chunk, and its first return value (`true`
//!   if it returns nothing) is cached and returned to every later `require`.
//!   A module that failed raises the same error on every later `require`.
//!   Requiring a module that is still loading is a cyclic require and
//!   fails. A module runs inside the call that first requires it, so it
//!   counts against that call's [`Budget`];
//! - `log.trace(...)`, `log.debug(...)`, `log.info(...)` and `log.warn(...)`,
//!   which record [`LogEntry`]s in any phase;
//! - `cls(color)`, `pset(x, y, color)`, `print(text, x, y, color)` and
//!   `spr(tile, x, y, [w, h])`, which record [`DrawCommand`]s and may only
//!   be called during `draw()`. `spr` draws a `w`x`h` block of 8x8 object
//!   tiles (default 1x1) taken from the 16-column object-tile grid, with
//!   `tile` at its top-left; the block may not run past the grid's right or
//!   bottom edge. Transparent index 0 is skipped and indices 1 through 15
//!   currently map directly to framebuffer indices;
//! - `btn(button)` and `btnp(button)`, which report whether a game button
//!   is held, or was just pressed, on the current tick. `button` is one of
//!   `"up"`, `"down"`, `"left"`, `"right"`, `"a"` or `"b"`; any other value
//!   is a runtime error. Both return `false` before the first tick.
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
//!   changes only the oscillator; `gate(channel, true)` opens the gate, or
//!   retriggers it if already open, and `gate(channel, false)` releases it.
//!
//! [`ButtonState`]: glaux_input::ButtonState
//! [`ModuleSource`]: glaux_modules::ModuleSource
//! [`ModuleName`]: glaux_modules::ModuleName
//! [`AudioState`]: glaux_audio::AudioState

mod api;
mod args;
mod error;
mod modules;
mod runtime;

pub use api::draw::{DrawCommand, MAX_DRAW_COMMANDS, SPRITE_TILE_COST, TEXT_CHAR_COST};
pub use api::log::{LogEntry, LogLevel};
pub use error::{RequireError, RequireFailure, RuntimeError, ScriptError, StartFailure};
pub use runtime::{Budget, DEFAULT_INTERRUPT_BUDGET, Phase, Runtime};

#[cfg(test)]
mod tests;
