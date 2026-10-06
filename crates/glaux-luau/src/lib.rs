//! Persistent Luau runtime for cartridge scripts.
//!
//! A [`Runtime`] owns one VM for the whole time a cartridge runs. Starting it
//! executes the entry chunk once and calls the script's optional `boot()`;
//! each [`Runtime::tick`] takes that tick's [`ButtonState`] and calls the
//! optional `tick()` and then `draw()`.
//!
//! Scripts see Luau's safe standard library without `require`, plus:
//!
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

use std::cell::{Cell, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use glaux_audio::{
    AudioState, CHANNELS, Envelope, MAX_ENVELOPE_MS, MAX_FREQUENCY, MAX_SUSTAIN, MAX_VOLUME, Tone,
    Waveform,
};
use glaux_input::{ButtonState, GameButton};
use glaux_machine::machine::TICK_RATE;
use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_GRID_ROWS, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH};
use mlua::{Function, Lua, MultiValue, Value, VmState};

/// Interrupt checks one call may pass with [`Budget::default`].
///
/// Measured in a debug build, `while true do end` passes about 15 million
/// checks per second, so a runaway call stops in roughly a third of a second.
pub const DEFAULT_INTERRUPT_BUDGET: u64 = 5_000_000;

/// Draw cost one `draw()` call may spend: each `cls` or `pset` costs one,
/// each `spr` costs the 64 pixels per tile it may draw, and each `print` costs
/// [`TEXT_CHAR_COST`] per character.
///
/// The completed frame is replayed on every host presentation frame, which
/// may run faster than the 60 Hz tick, so this bounds per-frame host work as
/// well as memory. Without it, the interrupt budget alone would let one
/// `draw()` record millions of commands.
pub const MAX_DRAW_COMMANDS: usize = 65_536;

/// Draw cost of one object tile drawn by `spr`: the 64 pixels it may set.
pub const SPRITE_TILE_COST: usize = OBJ_TILE_WIDTH * OBJ_TILE_HEIGHT;

/// Draw cost of one character of `print` text: about the number of pixels
/// a system font glyph sets, so that text and `pset` cost alike per pixel.
pub const TEXT_CHAR_COST: usize = 16;

/// How long each call into a script may execute before it is stopped.
///
/// The budget applies separately to the entry chunk and to every callback
/// call. It is counted in Luau interrupt checks, not VM instructions: Luau
/// checks at loop back-edges and function calls, so straight-line code costs
/// nothing and each loop iteration or call costs one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub interrupts: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            interrupts: DEFAULT_INTERRUPT_BUDGET,
        }
    }
}

/// One drawing operation recorded during `draw()`.
///
/// Colors are palette indices. Coordinates are unclipped: whoever replays
/// the commands clips them to the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawCommand {
    /// `cls(color)`: fill the whole screen.
    Clear(u8),
    /// `pset(x, y, color)`: write one pixel.
    Pixel { x: i32, y: i32, color: u8 },
    /// `spr(tile, x, y, w, h)`: draw a `width`x`height` block of object
    /// tiles from the object-tile grid, `tile` at its top-left, with `(x, y)`
    /// as its top-left corner. The block is validated to fit the grid.
    Sprite {
        tile: u16,
        x: i32,
        y: i32,
        width: u8,
        height: u8,
    },
    /// `print(text, x, y, color)`: draw text with `(x, y)` as the top-left
    /// corner of its first character cell.
    Text {
        text: String,
        x: i32,
        y: i32,
        color: u8,
    },
}

/// Severity of a [`LogEntry`], named after the `log` function that
/// recorded it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
}

impl LogLevel {
    const ALL: [Self; 4] = [Self::Trace, Self::Debug, Self::Info, Self::Warn];

    /// The field of the script's `log` table that records this level.
    fn function_name(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
        }
    }
}

/// Text a script logged: its arguments converted with `tostring` and joined
/// with tabs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub level: LogLevel,
    pub text: String,
}

/// A script that failed to compile or run. Messages name locations only by
/// the chunk name given to [`Runtime::start`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptError {
    #[error("SYNTAX ERROR: {0}")]
    Syntax(String),
    #[error("RUNTIME ERROR: {0}")]
    Runtime(String),
    #[error("EXECUTION LIMIT EXCEEDED")]
    LimitExceeded,
}

/// The part of a script's lifecycle that was executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The entry chunk, executed once by [`Runtime::start`].
    Entry,
    Boot,
    Tick,
    Draw,
}

impl Phase {
    /// The script global holding this phase's callback, if it has one.
    fn callback(self) -> Option<&'static str> {
        match self {
            Self::Entry => None,
            Self::Boot => Some("boot"),
            Self::Tick => Some("tick"),
            Self::Draw => Some("draw"),
        }
    }
}

/// A [`ScriptError`] and the phase it happened in.
///
/// Errors outside the entry chunk name the callback, as in
/// `RUNTIME ERROR IN TICK: main.luau:3: boom`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub phase: Phase,
    pub error: ScriptError,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(callback) = self.phase.callback() else {
            return self.error.fmt(f);
        };
        let callback = callback.to_uppercase();
        match &self.error {
            ScriptError::Syntax(message) => write!(f, "SYNTAX ERROR IN {callback}: {message}"),
            ScriptError::Runtime(message) => write!(f, "RUNTIME ERROR IN {callback}: {message}"),
            ScriptError::LimitExceeded => write!(f, "EXECUTION LIMIT EXCEEDED IN {callback}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Why [`Runtime::start`] failed, with the text logged before the failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartFailure {
    pub error: RuntimeError,
    pub output: Vec<LogEntry>,
}

/// Counts interrupt checks against the budget of the current call.
#[derive(Debug)]
struct Meter {
    budget: u64,
    checks: Cell<u64>,
    exhausted: Cell<bool>,
}

impl Meter {
    fn reset(&self) {
        self.checks.set(0);
        self.exhausted.set(false);
    }
}

/// Commands recorded by the `draw()` call in progress.
#[derive(Debug, Default)]
struct Recording {
    commands: Vec<DrawCommand>,
    /// Draw cost spent so far, charged against [`MAX_DRAW_COMMANDS`].
    cost: usize,
}

/// The recording of the `draw()` call in progress. `None` outside `draw()`,
/// which is how drawing functions reject other phases.
type PendingFrame = Rc<RefCell<Option<Recording>>>;

/// A live Luau VM running one cartridge script.
///
/// After any call returns an error the runtime must be dropped: the script
/// may have been interrupted halfway through updating its own state.
pub struct Runtime {
    lua: Lua,
    output: Rc<RefCell<Vec<LogEntry>>>,
    pending: PendingFrame,
    frame: Vec<DrawCommand>,
    /// The game buttons of the tick in progress.
    buttons: Rc<Cell<ButtonState>>,
    /// Logical ticks completed so far, as read by `frame()` and `t()`.
    ticks: Rc<Cell<u64>>,
    audio: Rc<RefCell<AudioState>>,
    meter: Rc<Meter>,
}

impl fmt::Debug for Runtime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runtime")
            .field("meter", &self.meter)
            .field("frame", &self.frame.len())
            .finish_non_exhaustive()
    }
}

impl Runtime {
    /// Creates a VM, executes `source` once and calls `boot()` if the script
    /// defines it.
    ///
    /// `chunk_name` is shown verbatim in error locations, as in
    /// `main.luau:3: boom`.
    pub fn start(chunk_name: &str, source: &[u8], budget: Budget) -> Result<Self, StartFailure> {
        let meter = Rc::new(Meter {
            budget: budget.interrupts,
            checks: Cell::new(0),
            exhausted: Cell::new(false),
        });
        let mut runtime = Self {
            lua: Lua::new(),
            output: Rc::default(),
            pending: Rc::default(),
            frame: Vec::new(),
            buttons: Rc::default(),
            ticks: Rc::default(),
            audio: Rc::default(),
            meter,
        };

        let started = runtime
            .call(Phase::Entry, |lua| {
                runtime.install_globals()?;
                lua.load(source).set_name(format!("={chunk_name}")).exec()
            })
            .and_then(|()| runtime.callback(Phase::Boot).map(drop));
        match started {
            Ok(()) => Ok(runtime),
            Err(error) => {
                tracing::debug!(chunk = chunk_name, %error, "script failed to start");
                Err(StartFailure {
                    error,
                    output: runtime.take_output(),
                })
            }
        }
    }

    /// Calls `tick()` and then, if it succeeded, `draw()`. A callback the
    /// script does not define is skipped.
    ///
    /// `btn` and `btnp` report `buttons` during both callbacks, and keep
    /// reporting it until the next tick.
    ///
    /// A `draw()` that returns normally replaces [`frame`](Self::frame) with
    /// the commands it recorded; one that fails leaves the previous frame.
    ///
    /// A successful call advances the logical time read by `frame()` and
    /// `t()`; a failed one does not.
    pub fn tick(&mut self, buttons: ButtonState) -> Result<(), RuntimeError> {
        self.buttons.set(buttons);
        self.callback(Phase::Tick)?;

        *self.pending.borrow_mut() = Some(Recording::default());
        let drawn = self.callback(Phase::Draw);
        let recording = self.pending.borrow_mut().take();
        if drawn? && let Some(recording) = recording {
            self.frame = recording.commands;
        }
        self.ticks.set(self.ticks.get() + 1);
        Ok(())
    }

    /// The commands recorded by the last `draw()` that completed, in call
    /// order. Empty until one does.
    pub fn frame(&self) -> &[DrawCommand] {
        &self.frame
    }

    /// Entries logged since the last call, oldest first.
    pub fn take_output(&mut self) -> Vec<LogEntry> {
        std::mem::take(&mut *self.output.borrow_mut())
    }

    /// The audio the script has written. It persists for the life of the
    /// VM; whoever applies it commits the writes it has applied.
    pub fn audio_mut(&mut self) -> RefMut<'_, AudioState> {
        self.audio.borrow_mut()
    }

    /// Removes `require`, installs the Glaux API and the interrupt that
    /// enforces the budget.
    fn install_globals(&self) -> mlua::Result<()> {
        let meter = Rc::clone(&self.meter);
        // The failure is sticky for the rest of the call so a `pcall` around
        // a runaway loop cannot resume it.
        self.lua.set_interrupt(move |_| {
            if !meter.exhausted.get() {
                let passed = meter.checks.get() + 1;
                meter.checks.set(passed);
                if passed <= meter.budget {
                    return Ok(VmState::Continue);
                }
                meter.exhausted.set(true);
            }
            Err(mlua::Error::runtime("execution limit exceeded"))
        });

        let globals = self.lua.globals();
        // The default Luau `require` resolves modules on the host filesystem.
        globals.raw_set("require", Value::Nil)?;
        // Captured before the script runs so that redefining the global
        // cannot change how Glaux converts values to text.
        let tostring: Function = globals.get("tostring")?;
        self.install_log(&tostring)?;
        self.install_draw(&tostring)?;
        self.install_input()?;
        self.install_time()?;
        self.install_audio()
    }

    /// Installs the `log` table.
    fn install_log(&self, tostring: &Function) -> mlua::Result<()> {
        let log = self.lua.create_table()?;
        for level in LogLevel::ALL {
            let name = level.function_name();
            let qualified = format!("log.{name}");
            let tostring = tostring.clone();
            let output = Rc::clone(&self.output);
            let function = self.lua.create_function(move |lua, args: MultiValue| {
                let mut parts = Vec::with_capacity(args.len());
                for arg in args {
                    parts.push(to_text(lua, &tostring, arg, &qualified)?);
                }
                output.borrow_mut().push(LogEntry {
                    level,
                    text: parts.join("\t"),
                });
                Ok(())
            })?;
            log.raw_set(name, function)?;
        }
        self.lua.globals().raw_set("log", log)
    }

    /// Installs `cls`, `pset`, `spr` and `print`.
    fn install_draw(&self, tostring: &Function) -> mlua::Result<()> {
        let globals = self.lua.globals();

        let pending = Rc::clone(&self.pending);
        let cls = self.lua.create_function(move |lua, color: Value| {
            expect_draw(lua, &pending, "cls")?;
            let color = color_arg(lua, "cls", 1, &color)?;
            record(lua, &pending, "cls", DrawCommand::Clear(color))
        })?;
        globals.raw_set("cls", cls)?;

        let pending = Rc::clone(&self.pending);
        let pset = self
            .lua
            .create_function(move |lua, (x, y, color): (Value, Value, Value)| {
                expect_draw(lua, &pending, "pset")?;
                let command = DrawCommand::Pixel {
                    x: coordinate_arg(lua, "pset", 1, &x)?,
                    y: coordinate_arg(lua, "pset", 2, &y)?,
                    color: color_arg(lua, "pset", 3, &color)?,
                };
                record(lua, &pending, "pset", command)
            })?;
        globals.raw_set("pset", pset)?;

        let pending = Rc::clone(&self.pending);
        let spr = self.lua.create_function(
            move |lua, (tile, x, y, w, h): (Value, Value, Value, Value, Value)| {
                expect_draw(lua, &pending, "spr")?;
                let tile = tile_arg(lua, "spr", 1, &tile)?;
                let x = coordinate_arg(lua, "spr", 2, &x)?;
                let y = coordinate_arg(lua, "spr", 3, &y)?;
                let column = usize::from(tile) % OBJ_GRID_COLUMNS;
                let row = usize::from(tile) / OBJ_GRID_COLUMNS;
                let width = tile_span_arg(lua, 4, &w, "width", column, OBJ_GRID_COLUMNS, "column")?;
                let height = tile_span_arg(lua, 5, &h, "height", row, OBJ_GRID_ROWS, "row")?;
                let command = DrawCommand::Sprite {
                    tile,
                    x,
                    y,
                    width,
                    height,
                };
                record(lua, &pending, "spr", command)
            },
        )?;
        globals.raw_set("spr", spr)?;

        let pending = Rc::clone(&self.pending);
        let tostring = tostring.clone();
        let print = self.lua.create_function(
            move |lua, (text, x, y, color): (Value, Value, Value, Value)| {
                expect_draw(lua, &pending, "print")?;
                let command = DrawCommand::Text {
                    x: coordinate_arg(lua, "print", 2, &x)?,
                    y: coordinate_arg(lua, "print", 3, &y)?,
                    color: color_arg(lua, "print", 4, &color)?,
                    // Converted last: a `__tostring` metamethod runs script
                    // code, which must not see a half-validated call.
                    text: to_text(lua, &tostring, text, "print")?,
                };
                record(lua, &pending, "print", command)
            },
        )?;
        globals.raw_set("print", print)
    }

    /// Installs `btn` and `btnp`.
    fn install_input(&self) -> mlua::Result<()> {
        let globals = self.lua.globals();

        let buttons = Rc::clone(&self.buttons);
        let btn = self.lua.create_function(move |lua, button: Value| {
            let button = button_arg(lua, "btn", 1, &button)?;
            Ok(buttons.get().held.contains(button))
        })?;
        globals.raw_set("btn", btn)?;

        let buttons = Rc::clone(&self.buttons);
        let btnp = self.lua.create_function(move |lua, button: Value| {
            let button = button_arg(lua, "btnp", 1, &button)?;
            Ok(buttons.get().pressed.contains(button))
        })?;
        globals.raw_set("btnp", btnp)
    }

    /// Installs `frame` and `t`.
    fn install_time(&self) -> mlua::Result<()> {
        let globals = self.lua.globals();

        // Returned as doubles because Luau numbers are doubles; tick counts
        // stay exact far beyond any realistic run length.
        let ticks = Rc::clone(&self.ticks);
        let frame = self
            .lua
            .create_function(move |_, ()| Ok(ticks.get() as f64))?;
        globals.raw_set("frame", frame)?;

        let ticks = Rc::clone(&self.ticks);
        let t = self
            .lua
            .create_function(move |_, ()| Ok(ticks.get() as f64 / f64::from(TICK_RATE)))?;
        globals.raw_set("t", t)
    }

    /// Installs the `audio` table.
    fn install_audio(&self) -> mlua::Result<()> {
        let audio = self.lua.create_table()?;

        let state = Rc::clone(&self.audio);
        let tone = self.lua.create_function(
            move |lua, (channel, frequency, volume, waveform): (Value, Value, Value, Value)| {
                const NAME: &str = "audio.tone";
                let channel = channel_arg(lua, NAME, &channel)?;
                let tone = Tone {
                    frequency: bounded_arg(
                        lua,
                        NAME,
                        2,
                        &frequency,
                        "frequency",
                        MAX_FREQUENCY.into(),
                    )? as u16,
                    volume: bounded_arg(lua, NAME, 3, &volume, "volume", MAX_VOLUME.into())? as u8,
                    waveform: waveform_arg(lua, NAME, 4, &waveform)?,
                };
                let written = state.borrow_mut().set_tone(channel, tone);
                written.map_err(|err| located(lua, err.to_string()))
            },
        )?;
        audio.raw_set("tone", tone)?;

        let state = Rc::clone(&self.audio);
        let env = self.lua.create_function(
            move |lua,
                  (channel, attack, decay, sustain, release): (
                Value,
                Value,
                Value,
                Value,
                Value,
            )| {
                const NAME: &str = "audio.env";
                let channel = channel_arg(lua, NAME, &channel)?;
                let time = |position, value: &Value| {
                    bounded_arg(lua, NAME, position, value, "time in ms", MAX_ENVELOPE_MS)
                };
                let envelope = Envelope {
                    attack_ms: time(2, &attack)?,
                    decay_ms: time(3, &decay)?,
                    sustain: bounded_arg(lua, NAME, 4, &sustain, "sustain", MAX_SUSTAIN.into())?
                        as u8,
                    release_ms: time(5, &release)?,
                };
                let written = state.borrow_mut().set_envelope(channel, envelope);
                written.map_err(|err| located(lua, err.to_string()))
            },
        )?;
        audio.raw_set("env", env)?;

        let state = Rc::clone(&self.audio);
        let gate = self
            .lua
            .create_function(move |lua, (channel, enabled): (Value, Value)| {
                const NAME: &str = "audio.gate";
                let channel = channel_arg(lua, NAME, &channel)?;
                let Value::Boolean(open) = enabled else {
                    return Err(bad_argument(
                        lua,
                        NAME,
                        2,
                        &format!("boolean expected, got {}", script_type_name(&enabled)),
                    ));
                };
                let written = state.borrow_mut().set_gate(channel, open);
                written.map_err(|err| located(lua, err.to_string()))
            })?;
        audio.raw_set("gate", gate)?;

        self.lua.globals().raw_set("audio", audio)
    }

    /// Calls the global callback for `phase`, if the script defines one.
    /// Returns whether it did.
    fn callback(&self, phase: Phase) -> Result<bool, RuntimeError> {
        let Some(name) = phase.callback() else {
            return Ok(false);
        };
        self.call(phase, |lua| {
            // Looked up on every call so a script may replace its callbacks.
            match lua.globals().raw_get::<Value>(name)? {
                Value::Nil => Ok(false),
                Value::Function(callback) => callback.call::<()>(()).map(|()| true),
                other => Err(mlua::Error::runtime(format!(
                    "'{name}' must be a function, not a {}",
                    other.type_name()
                ))),
            }
        })
    }

    /// Runs `body` with a fresh budget, reporting exhaustion over whatever
    /// `body` returned: a script may have caught the limit error with `pcall`
    /// and finished normally.
    fn call<T>(
        &self,
        phase: Phase,
        body: impl FnOnce(&Lua) -> mlua::Result<T>,
    ) -> Result<T, RuntimeError> {
        self.meter.reset();
        let result = body(&self.lua);
        if self.meter.exhausted.get() {
            return Err(RuntimeError {
                phase,
                error: ScriptError::LimitExceeded,
            });
        }
        result.map_err(|err| RuntimeError {
            phase,
            error: script_error(&err),
        })
    }
}

/// Fails unless a `draw()` call is in progress.
fn expect_draw(lua: &Lua, pending: &PendingFrame, function: &str) -> mlua::Result<()> {
    if pending.borrow().is_some() {
        Ok(())
    } else {
        Err(outside_draw(lua, function))
    }
}

fn outside_draw(lua: &Lua, function: &str) -> mlua::Error {
    located(
        lua,
        format!("'{function}' can only be called during draw()"),
    )
}

/// Appends `command` to the frame in progress.
///
/// Callers check [`expect_draw`] before validating arguments, so a script
/// calling from the wrong phase hears about the phase first.
fn record(
    lua: &Lua,
    pending: &PendingFrame,
    function: &str,
    command: DrawCommand,
) -> mlua::Result<()> {
    let mut pending = pending.borrow_mut();
    let Some(recording) = pending.as_mut() else {
        return Err(outside_draw(lua, function));
    };
    let cost = match &command {
        DrawCommand::Text { text, .. } => {
            text.chars().count().max(1).saturating_mul(TEXT_CHAR_COST)
        }
        // Grid-sized blocks cannot overflow; if one did, its cost must still
        // exceed the limit rather than wrap below it.
        DrawCommand::Sprite { width, height, .. } => usize::from(*width)
            .checked_mul(usize::from(*height))
            .and_then(|tiles| tiles.checked_mul(SPRITE_TILE_COST))
            .unwrap_or(usize::MAX),
        DrawCommand::Clear(_) | DrawCommand::Pixel { .. } => 1,
    };
    recording.cost = recording.cost.saturating_add(cost);
    if recording.cost > MAX_DRAW_COMMANDS {
        return Err(located(
            lua,
            format!("too many draw commands in one frame (limit {MAX_DRAW_COMMANDS})"),
        ));
    }
    recording.commands.push(command);
    Ok(())
}

/// Converts `value` with the `tostring` captured before the script ran.
fn to_text(lua: &Lua, tostring: &Function, value: Value, function: &str) -> mlua::Result<String> {
    match tostring.call::<Value>(value)? {
        Value::String(text) => Ok(text.to_string_lossy()),
        _ => Err(located(
            lua,
            format!("'tostring' must return a string to '{function}'"),
        )),
    }
}

/// A numeric argument. Strings are not coerced.
fn number_arg(lua: &Lua, function: &str, position: usize, value: &Value) -> mlua::Result<f64> {
    match *value {
        Value::Integer(n) => Ok(n as f64),
        Value::Number(n) => Ok(n),
        ref other => Err(bad_argument(
            lua,
            function,
            position,
            &format!("number expected, got {}", other.type_name()),
        )),
    }
}

/// A screen coordinate: floored, and saturated to the `i32` range so that
/// huge values land off-screen instead of failing.
fn coordinate_arg(lua: &Lua, function: &str, position: usize, value: &Value) -> mlua::Result<i32> {
    let n = number_arg(lua, function, position, value)?;
    if n.is_nan() {
        return Err(bad_argument(lua, function, position, "number is NaN"));
    }
    // `as` saturates float-to-int conversions, including infinities.
    Ok(n.floor() as i32)
}

/// A palette index: an integer in `0..=255`, never wrapped or clamped.
fn color_arg(lua: &Lua, function: &str, position: usize, value: &Value) -> mlua::Result<u8> {
    let n = number_arg(lua, function, position, value)?;
    if n.fract() == 0.0 && (0.0..=255.0).contains(&n) {
        Ok(n as u8)
    } else {
        Err(bad_argument(
            lua,
            function,
            position,
            &format!("palette index 0-255 expected, got {n}"),
        ))
    }
}

/// An object-tile index: an integer in `0..=1023`.
fn tile_arg(lua: &Lua, function: &str, position: usize, value: &Value) -> mlua::Result<u16> {
    let n = number_arg(lua, function, position, value)?;
    if n.fract() == 0.0 && (0.0..=1023.0).contains(&n) {
        Ok(n as u16)
    } else {
        Err(bad_argument(
            lua,
            function,
            position,
            &format!("object tile index 0-1023 expected, got {n}"),
        ))
    }
}

/// A sprite block's `extent` (width or height) in tiles: `nil` means 1,
/// otherwise a positive integer that keeps the block inside the object-tile
/// grid when it starts at grid `axis` `start` of `limit`.
fn tile_span_arg(
    lua: &Lua,
    position: usize,
    value: &Value,
    extent: &str,
    start: usize,
    limit: usize,
    axis: &str,
) -> mlua::Result<u8> {
    if value.is_nil() {
        return Ok(1);
    }
    let n = number_arg(lua, "spr", position, value)?;
    if !(n.fract() == 0.0 && n >= 1.0) {
        return Err(bad_argument(
            lua,
            "spr",
            position,
            &format!("positive tile count expected, got {n}"),
        ));
    }
    let available = limit - start;
    if n > available as f64 {
        return Err(bad_argument(
            lua,
            "spr",
            position,
            &format!(
                "{extent} {n} from tile {axis} {start} exceeds the {limit}-{axis} object-tile grid"
            ),
        ));
    }
    // `available` is at most OBJ_GRID_ROWS, which fits in a byte.
    Ok(n as u8)
}

/// An integer argument in `0..=max`, never wrapped or clamped. `what` names
/// the value in the error.
fn bounded_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
    what: &str,
    max: u32,
) -> mlua::Result<u32> {
    let n = number_arg(lua, function, position, value)?;
    if n.fract() == 0.0 && (0.0..=f64::from(max)).contains(&n) {
        Ok(n as u32)
    } else {
        Err(bad_argument(
            lua,
            function,
            position,
            &format!("{what} 0-{max} expected, got {n}"),
        ))
    }
}

/// An audio channel: the first argument of every `audio` function.
fn channel_arg(lua: &Lua, function: &str, value: &Value) -> mlua::Result<usize> {
    let max = CHANNELS as u32 - 1;
    bounded_arg(lua, function, 1, value, "channel", max).map(|channel| channel as usize)
}

/// A waveform, named as scripts name it.
fn waveform_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<Waveform> {
    let Value::String(name) = value else {
        return Err(bad_argument(
            lua,
            function,
            position,
            &format!("string expected, got {}", script_type_name(value)),
        ));
    };
    let name = name.to_string_lossy();
    Waveform::from_name(&name).ok_or_else(|| {
        bad_argument(
            lua,
            function,
            position,
            &format!("unknown waveform '{name}'; expected square, triangle, saw or noise"),
        )
    })
}

/// The type name a script sees for `value`. mlua tells integers apart, but
/// Luau scripts only have `number`.
fn script_type_name(value: &Value) -> &'static str {
    match value {
        Value::Integer(_) => "number",
        other => other.type_name(),
    }
}

/// A game button, named as scripts name it.
fn button_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<GameButton> {
    let Value::String(name) = value else {
        return Err(bad_argument(
            lua,
            function,
            position,
            &format!("string expected, got {}", script_type_name(value)),
        ));
    };
    let name = name.to_string_lossy();
    let button = match name.as_str() {
        "up" => GameButton::Up,
        "down" => GameButton::Down,
        "left" => GameButton::Left,
        "right" => GameButton::Right,
        "a" => GameButton::A,
        "b" => GameButton::B,
        _ => {
            return Err(bad_argument(
                lua,
                function,
                position,
                &format!("unknown button '{name}'; expected up, down, left, right, a or b"),
            ));
        }
    };
    Ok(button)
}

fn bad_argument(lua: &Lua, function: &str, position: usize, detail: &str) -> mlua::Error {
    located(
        lua,
        format!("bad argument #{position} to '{function}' ({detail})"),
    )
}

/// A runtime error prefixed with the calling script line, as Luau's own
/// errors are, so that errors raised from Rust read like built-in ones.
fn located(lua: &Lua, message: String) -> mlua::Error {
    // Level 0 is the Rust function itself; level 1 is the script calling it.
    let place = lua
        .inspect_stack(1, |frame| {
            let line = frame.current_line()?;
            let source = frame.source().short_src?.into_owned();
            Some(format!("{source}:{line}: "))
        })
        .flatten()
        .unwrap_or_default();
    mlua::Error::runtime(format!("{place}{message}"))
}

/// The first line of the innermost Lua error message, without mlua's kind
/// prefix or the stack traceback that follows it.
fn script_error(err: &mlua::Error) -> ScriptError {
    let mut err = err;
    while let mlua::Error::CallbackError { cause, .. } = err {
        err = cause;
    }
    let first_line = |message: &str| message.lines().next().unwrap_or("").to_owned();
    match err {
        mlua::Error::SyntaxError { message, .. } => ScriptError::Syntax(first_line(message)),
        mlua::Error::RuntimeError(message) => ScriptError::Runtime(first_line(message)),
        other => ScriptError::Runtime(first_line(&other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaux_audio::{ChannelWrite, EnvelopeWrite, MAX_ENVELOPE_WRITES};
    use glaux_input::ButtonSet;
    use pretty_assertions::assert_eq;

    const SMALL: Budget = Budget { interrupts: 10_000 };
    const NO_BUTTONS: ButtonState = ButtonState {
        held: ButtonSet::EMPTY,
        pressed: ButtonSet::EMPTY,
    };

    fn start(source: &str) -> Result<Runtime, StartFailure> {
        Runtime::start("main.luau", source.as_bytes(), SMALL)
    }

    fn started(source: &str) -> Runtime {
        match start(source) {
            Ok(runtime) => runtime,
            Err(failure) => panic!("expected the script to start, got {failure:?}"),
        }
    }

    fn start_failure(source: &str) -> StartFailure {
        match start(source) {
            Ok(_) => panic!("expected the script to fail to start"),
            Err(failure) => failure,
        }
    }

    /// The text of each entry, dropping levels.
    fn texts(entries: Vec<LogEntry>) -> Vec<String> {
        entries.into_iter().map(|entry| entry.text).collect()
    }

    /// Starts `source` and returns what it logged while starting.
    fn start_output(source: &str) -> Vec<String> {
        texts(started(source).take_output())
    }

    fn runtime_message(source: &str) -> String {
        match start_failure(source).error.error {
            ScriptError::Runtime(message) => message,
            other => panic!("expected a runtime error, got {other:?}"),
        }
    }

    fn error(phase: Phase, error: ScriptError) -> RuntimeError {
        RuntimeError { phase, error }
    }

    // Starting.

    #[test]
    fn starts_a_script_without_output() {
        assert_eq!(start_output("local x = 1 + 2"), Vec::<String>::new());
    }

    #[test]
    fn entry_and_boot_run_once_each_at_start() {
        let mut runtime = started(
            r#"
            log.info("entry")
            function boot() log.info("boot") end
            "#,
        );
        assert_eq!(texts(runtime.take_output()), ["entry", "boot"]);
        for _ in 0..3 {
            runtime.tick(NO_BUTTONS).unwrap();
        }
        assert_eq!(texts(runtime.take_output()), Vec::<String>::new());
    }

    #[test]
    fn missing_callbacks_are_skipped() {
        let mut runtime = started("local x = 1");
        assert_eq!(runtime.tick(NO_BUTTONS), Ok(()));
        assert_eq!(runtime.tick(NO_BUTTONS), Ok(()));
    }

    #[test]
    fn syntax_errors_name_the_chunk_and_print_nothing() {
        let failure = start_failure("log.info(\"never\")\nlocal = 1");
        assert!(failure.output.is_empty());
        assert_eq!(failure.error.phase, Phase::Entry);
        match failure.error.error {
            ScriptError::Syntax(message) => {
                assert!(message.starts_with("main.luau:2:"), "{message}");
            }
            other => panic!("expected a syntax error, got {other:?}"),
        }
    }

    #[test]
    fn runtime_errors_keep_only_the_first_line() {
        assert_eq!(
            runtime_message("local x = 1\nerror(\"boom\")"),
            "main.luau:2: boom"
        );
        let message = runtime_message("local f = nil\nf()");
        assert!(message.starts_with("main.luau:2:"), "{message}");
        assert!(!message.contains('\n'), "{message}");
    }

    #[test]
    fn output_before_a_start_failure_is_kept() {
        let failure = start_failure(r#"log.info("before") error("boom")"#);
        assert_eq!(texts(failure.output.clone()), ["before"]);
        assert_eq!(failure.error.phase, Phase::Entry);

        let failure =
            start_failure(r#"log.info("entry") function boot() log.info("boot") error("x") end"#);
        assert_eq!(texts(failure.output.clone()), ["entry", "boot"]);
        assert_eq!(
            failure.error,
            error(Phase::Boot, ScriptError::Runtime("main.luau:1: x".into()))
        );
    }

    #[test]
    fn require_is_unavailable() {
        let message = runtime_message(r#"require("x")"#);
        assert!(message.starts_with("main.luau:1:"), "{message}");
    }

    // Logging.

    #[test]
    fn log_functions_record_their_level() {
        let source = r#"
            log.trace("t") log.debug("d") log.info("i") log.warn("w")
        "#;
        let entry = |level, text: &str| LogEntry {
            level,
            text: text.into(),
        };
        assert_eq!(
            started(source).take_output(),
            [
                entry(LogLevel::Trace, "t"),
                entry(LogLevel::Debug, "d"),
                entry(LogLevel::Info, "i"),
                entry(LogLevel::Warn, "w"),
            ]
        );
    }

    #[test]
    fn every_log_level_formats_arguments_alike() {
        let source = r#"
            local owl = setmetatable({}, { __tostring = function() return "OWL" end })
            tostring = function() return "x" end
            for _, f in { log.trace, log.debug, log.info, log.warn } do
                f(owl, 1, nil)
            end
        "#;
        assert_eq!(start_output(source), ["OWL\t1\tnil"; 4]);
    }

    #[test]
    fn error_and_the_debug_library_are_untouched() {
        assert_eq!(runtime_message(r#"error("x")"#), "main.luau:1: x");
        assert_eq!(
            start_output(r#"log.info(type(debug), type(debug.traceback))"#),
            ["table\tfunction"]
        );
    }

    #[test]
    fn print_is_not_a_log_function() {
        let message = runtime_message(r#"print("hi")"#);
        assert_eq!(
            message,
            "main.luau:1: 'print' can only be called during draw()"
        );
    }

    #[test]
    fn log_passes_text_out() {
        assert_eq!(start_output(r#"log.info("hi")"#), ["hi"]);
    }

    #[test]
    fn log_joins_arguments_with_tabs() {
        assert_eq!(
            start_output(r#"log.info("a", 1, true, nil) log.info()"#),
            ["a\t1\ttrue\tnil", ""]
        );
    }

    #[test]
    fn log_honors_tostring_metamethods() {
        let source = r#"
            local owl = setmetatable({}, { __tostring = function() return "OWL" end })
            log.info(owl, 2)
        "#;
        assert_eq!(start_output(source), ["OWL\t2"]);
    }

    #[test]
    fn log_uses_tostring_as_it_was_before_the_script() {
        let source = r#"tostring = function() return "x" end log.info(1)"#;
        assert_eq!(start_output(source), ["1"]);
    }

    #[test]
    fn log_rejects_a_non_string_tostring_result() {
        let source = r#"log.info(setmetatable({}, { __tostring = function() return {} end }))"#;
        let message = runtime_message(source);
        assert!(message.contains("must return a string"), "{message}");
    }

    #[test]
    fn take_output_drains_the_buffer() {
        let mut runtime = started(r#"function tick() log.info("t") end"#);
        runtime.tick(NO_BUTTONS).unwrap();
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(texts(runtime.take_output()), ["t", "t"]);
        assert_eq!(texts(runtime.take_output()), Vec::<String>::new());
    }

    // Ticking.

    #[test]
    fn tick_runs_once_per_call_and_draw_follows_it() {
        let mut runtime = started(
            r#"
            function tick() log.info("tick") end
            function draw() log.info("draw") end
            "#,
        );
        for _ in 0..3 {
            runtime.tick(NO_BUTTONS).unwrap();
        }
        assert_eq!(
            texts(runtime.take_output()),
            ["tick", "draw", "tick", "draw", "tick", "draw"]
        );
    }

    #[test]
    fn globals_persist_across_ticks() {
        let mut runtime = started(
            r#"
            count = 0
            local seen = {}
            function tick() count += 1 table.insert(seen, count) end
            function draw() log.info(count, #seen) end
            "#,
        );
        for _ in 0..3 {
            runtime.tick(NO_BUTTONS).unwrap();
        }
        assert_eq!(texts(runtime.take_output()), ["1\t1", "2\t2", "3\t3"]);
    }

    #[test]
    fn callbacks_are_looked_up_on_every_call() {
        let mut runtime = started(
            r#"
            function tick()
                log.info("first")
                tick = function() log.info("second") end
            end
            "#,
        );
        runtime.tick(NO_BUTTONS).unwrap();
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(texts(runtime.take_output()), ["first", "second"]);
    }

    #[test]
    fn a_failing_tick_skips_draw() {
        let mut runtime = started(
            r#"
            function tick() log.info("tick") error("boom") end
            function draw() log.info("draw") end
            "#,
        );
        assert_eq!(
            runtime.tick(NO_BUTTONS),
            Err(error(
                Phase::Tick,
                ScriptError::Runtime("main.luau:2: boom".into())
            ))
        );
        assert_eq!(texts(runtime.take_output()), ["tick"]);
    }

    #[test]
    fn draw_errors_name_the_draw_phase() {
        let mut runtime = started(r#"function draw() error("bad") end"#);
        assert_eq!(
            runtime.tick(NO_BUTTONS),
            Err(error(
                Phase::Draw,
                ScriptError::Runtime("main.luau:1: bad".into())
            ))
        );
    }

    #[test]
    fn non_function_callbacks_are_errors() {
        let failure = start_failure("boot = 3");
        assert_eq!(failure.error.phase, Phase::Boot);
        assert!(
            matches!(&failure.error.error, ScriptError::Runtime(m) if m.contains("'boot' must be a function")),
            "{failure:?}"
        );

        let mut runtime = started(r#"tick = "x""#);
        let err = runtime.tick(NO_BUTTONS).unwrap_err();
        assert_eq!(err.phase, Phase::Tick);
        assert!(
            matches!(&err.error, ScriptError::Runtime(m) if m.contains("'tick' must be a function")),
            "{err:?}"
        );
    }

    // Execution limits.

    const RUNAWAY: &str = "while true do end";
    const CAUGHT_RUNAWAY: &str = "pcall(function() while true do end end) log.info(\"after\")";

    fn limit(phase: Phase) -> RuntimeError {
        error(phase, ScriptError::LimitExceeded)
    }

    #[test]
    fn runaway_entry_and_boot_are_interrupted() {
        for body in [RUNAWAY, CAUGHT_RUNAWAY] {
            assert_eq!(start_failure(body).error, limit(Phase::Entry), "{body}");
            let boot = format!("function boot() {body} end");
            assert_eq!(start_failure(&boot).error, limit(Phase::Boot), "{body}");
        }
        assert_eq!(
            start_failure("local function f() for i = 1, math.huge do end end f()").error,
            limit(Phase::Entry)
        );
    }

    #[test]
    fn runaway_tick_and_draw_are_interrupted() {
        for body in [RUNAWAY, CAUGHT_RUNAWAY] {
            for (name, phase) in [("tick", Phase::Tick), ("draw", Phase::Draw)] {
                let mut runtime = started(&format!("function {name}() {body} end"));
                assert_eq!(
                    runtime.tick(NO_BUTTONS),
                    Err(limit(phase)),
                    "{name}: {body}"
                );
                assert!(
                    !texts(runtime.take_output()).contains(&"after".to_owned()),
                    "{name}: {body}"
                );
            }
        }
    }

    #[test]
    fn the_budget_applies_to_each_call() {
        // Each call uses most of the budget, which together would exceed it
        // many times over.
        let mut runtime = started(
            r#"
            local function spin() for i = 1, 7000 do end end
            spin()
            function boot() spin() end
            function tick() spin() end
            function draw() spin() end
            "#,
        );
        for _ in 0..20 {
            assert_eq!(runtime.tick(NO_BUTTONS), Ok(()));
        }
        // Two calls' worth of spinning in one call does not fit.
        assert_eq!(
            start_failure("for i = 1, 14000 do end").error,
            limit(Phase::Entry)
        );
    }

    #[test]
    fn bounded_loops_finish_within_the_budget() {
        assert!(start("for i = 1, 100 do end").is_ok());
    }

    #[test]
    fn errors_name_their_phase() {
        let shown = |phase, error| self::error(phase, error).to_string();
        assert_eq!(
            shown(Phase::Entry, ScriptError::Runtime("main.luau:1: x".into())),
            "RUNTIME ERROR: main.luau:1: x"
        );
        assert_eq!(
            shown(Phase::Tick, ScriptError::Runtime("main.luau:1: x".into())),
            "RUNTIME ERROR IN TICK: main.luau:1: x"
        );
        assert_eq!(
            shown(Phase::Draw, ScriptError::LimitExceeded),
            "EXECUTION LIMIT EXCEEDED IN DRAW"
        );
        assert_eq!(
            shown(Phase::Boot, ScriptError::LimitExceeded),
            "EXECUTION LIMIT EXCEEDED IN BOOT"
        );
    }

    // Drawing.

    fn pixel(x: i32, y: i32, color: u8) -> DrawCommand {
        DrawCommand::Pixel { x, y, color }
    }

    fn sprite(tile: u16, x: i32, y: i32) -> DrawCommand {
        sprite_block(tile, x, y, 1, 1)
    }

    fn sprite_block(tile: u16, x: i32, y: i32, width: u8, height: u8) -> DrawCommand {
        DrawCommand::Sprite {
            tile,
            x,
            y,
            width,
            height,
        }
    }

    /// The frame published by one tick of a script whose `draw()` body is
    /// `body`.
    fn drawn(body: &str) -> Vec<DrawCommand> {
        let mut runtime = started(&format!("function draw() {body} end"));
        runtime.tick(NO_BUTTONS).unwrap();
        runtime.frame().to_vec()
    }

    /// The runtime error raised by the first tick of `draw()` `body`.
    fn draw_error(body: &str) -> String {
        let mut runtime = started(&format!("function draw() {body} end"));
        match runtime.tick(NO_BUTTONS) {
            Err(RuntimeError {
                phase: Phase::Draw,
                error: ScriptError::Runtime(message),
            }) => message,
            other => panic!("expected a runtime error in draw, got {other:?}"),
        }
    }

    #[test]
    fn the_frame_starts_empty() {
        assert_eq!(started("").frame(), []);
    }

    #[test]
    fn draw_functions_record_commands_in_call_order() {
        assert_eq!(drawn("cls(3)"), [DrawCommand::Clear(3)]);
        assert_eq!(
            drawn(r#"cls(1) spr(3, 7, 8) pset(5, 5, 2) print("HI", 10, 20, 16) cls(0)"#),
            [
                DrawCommand::Clear(1),
                sprite(3, 7, 8),
                pixel(5, 5, 2),
                DrawCommand::Text {
                    text: "HI".into(),
                    x: 10,
                    y: 20,
                    color: 16,
                },
                DrawCommand::Clear(0),
            ]
        );
    }

    #[test]
    fn print_converts_text_with_the_original_tostring() {
        assert_eq!(
            drawn(r#"tostring = function() return "x" end print(42, 0, 0, 1)"#),
            [DrawCommand::Text {
                text: "42".into(),
                x: 0,
                y: 0,
                color: 1,
            }]
        );
    }

    #[test]
    fn each_successful_draw_replaces_the_frame() {
        let mut runtime = started(
            r#"
            local n = 0
            function draw() n += 1 for i = 1, n do pset(i, 0, n) end end
            "#,
        );
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(runtime.frame(), [pixel(1, 0, 1)]);
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(runtime.frame(), [pixel(1, 0, 2), pixel(2, 0, 2)]);
    }

    #[test]
    fn a_draw_that_records_nothing_publishes_an_empty_frame() {
        let mut runtime = started(
            "local first = true function draw() if first then cls(1) end first = false end",
        );
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(runtime.frame(), [DrawCommand::Clear(1)]);
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(runtime.frame(), []);
    }

    #[test]
    fn a_failing_draw_keeps_the_previous_frame() {
        for failure in [r#"error("boom")"#, "while true do end"] {
            let mut runtime = started(&format!(
                r#"
                local n = 0
                function draw()
                    n += 1
                    cls(n)
                    if n == 2 then pset(1, 1, 1) {failure} end
                end
                "#
            ));
            runtime.tick(NO_BUTTONS).unwrap();
            assert!(runtime.tick(NO_BUTTONS).is_err(), "{failure}");
            assert_eq!(runtime.frame(), [DrawCommand::Clear(1)], "{failure}");
        }
    }

    #[test]
    fn a_failing_draw_does_not_publish_sprite_commands() {
        let mut runtime = started(
            r#"
            local n = 0
            function draw()
                n += 1
                cls(n)
                if n == 2 then spr(7, 10, 20) error("boom") end
            end
            "#,
        );
        runtime.tick(NO_BUTTONS).unwrap();
        assert!(runtime.tick(NO_BUTTONS).is_err());
        assert_eq!(runtime.frame(), [DrawCommand::Clear(1)]);
    }

    #[test]
    fn a_failing_tick_keeps_the_previous_frame() {
        let mut runtime = started(
            r#"
            local n = 0
            function tick() n += 1 if n == 2 then error("x") end end
            function draw() cls(n) end
            "#,
        );
        runtime.tick(NO_BUTTONS).unwrap();
        assert!(runtime.tick(NO_BUTTONS).is_err());
        assert_eq!(runtime.frame(), [DrawCommand::Clear(1)]);
    }

    #[test]
    fn removing_draw_keeps_the_last_frame() {
        let mut runtime = started("function draw() cls(4) draw = nil end");
        runtime.tick(NO_BUTTONS).unwrap();
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(runtime.frame(), [DrawCommand::Clear(4)]);
    }

    #[test]
    fn draw_functions_fail_outside_draw() {
        for call in [
            "cls(1)",
            "pset(0, 0, 1)",
            "spr(0, 0, 0)",
            r#"print("x", 0, 0, 1)"#,
        ] {
            let name = &call[..call.find('(').unwrap()];
            let expected = format!("'{name}' can only be called during draw()");

            let message = runtime_message(call);
            assert_eq!(message, format!("main.luau:1: {expected}"));
            let failure = start_failure(&format!("function boot() {call} end"));
            assert_eq!(failure.error.phase, Phase::Boot);

            let mut runtime = started(&format!("function tick() {call} end"));
            let err = runtime.tick(NO_BUTTONS).unwrap_err();
            assert_eq!(err.phase, Phase::Tick);
            assert!(
                matches!(&err.error, ScriptError::Runtime(m) if m.ends_with(&expected)),
                "{err:?}"
            );
        }
    }

    #[test]
    fn draw_functions_fail_after_draw_returns() {
        let mut runtime = started(
            r#"
            local later
            function draw() later = function() cls(1) end end
            function tick() if later then later() end end
            "#,
        );
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(runtime.tick(NO_BUTTONS).unwrap_err().phase, Phase::Tick);
    }

    #[test]
    fn colors_must_be_palette_indices() {
        assert_eq!(
            drawn("cls(0) cls(255)"),
            [DrawCommand::Clear(0), DrawCommand::Clear(255)]
        );
        for (call, detail) in [
            (
                "cls(256)",
                "#1 to 'cls' (palette index 0-255 expected, got 256)",
            ),
            (
                "cls(-1)",
                "#1 to 'cls' (palette index 0-255 expected, got -1)",
            ),
            (
                "cls(1.5)",
                "#1 to 'cls' (palette index 0-255 expected, got 1.5)",
            ),
            (
                "cls(0/0)",
                "#1 to 'cls' (palette index 0-255 expected, got NaN)",
            ),
            (
                "cls(math.huge)",
                "#1 to 'cls' (palette index 0-255 expected, got inf)",
            ),
            (r#"cls("1")"#, "#1 to 'cls' (number expected, got string)"),
            ("cls()", "#1 to 'cls' (number expected, got nil)"),
            (
                "pset(0, 0, 300)",
                "#3 to 'pset' (palette index 0-255 expected, got 300)",
            ),
            (
                r#"print("x", 0, 0, true)"#,
                "#4 to 'print' (number expected, got boolean)",
            ),
        ] {
            assert_eq!(
                draw_error(call),
                format!("main.luau:1: bad argument {detail}"),
                "{call}"
            );
        }
    }

    #[test]
    fn coordinates_must_be_numbers() {
        for (call, detail) in [
            ("pset(0/0, 0, 1)", "#1 to 'pset' (number is NaN)"),
            (
                r#"pset("1", 0, 1)"#,
                "#1 to 'pset' (number expected, got string)",
            ),
            ("pset(0, nil, 1)", "#2 to 'pset' (number expected, got nil)"),
            (
                r#"print("x", 0, {}, 1)"#,
                "#3 to 'print' (number expected, got table)",
            ),
            (
                r#"spr(0, "1", 0)"#,
                "#2 to 'spr' (number expected, got string)",
            ),
        ] {
            assert_eq!(
                draw_error(call),
                format!("main.luau:1: bad argument {detail}"),
                "{call}"
            );
        }
    }

    #[test]
    fn fractional_coordinates_are_floored() {
        assert_eq!(
            drawn("pset(1.9, 2.5, 1) pset(-0.5, -1.5, 1) spr(2, 3.9, -0.5)"),
            [pixel(1, 2, 1), pixel(-1, -2, 1), sprite(2, 3, -1)]
        );
    }

    #[test]
    fn sprite_tile_indices_must_be_integers_in_range() {
        assert_eq!(
            drawn("spr(0, 1, 2) spr(1023, 3, 4)"),
            [sprite(0, 1, 2), sprite(1023, 3, 4)]
        );
        for (call, detail) in [
            (
                "spr(-1, 0, 0)",
                "#1 to 'spr' (object tile index 0-1023 expected, got -1)",
            ),
            (
                "spr(1024, 0, 0)",
                "#1 to 'spr' (object tile index 0-1023 expected, got 1024)",
            ),
            (
                "spr(1.5, 0, 0)",
                "#1 to 'spr' (object tile index 0-1023 expected, got 1.5)",
            ),
            (
                r#"spr("1", 0, 0)"#,
                "#1 to 'spr' (number expected, got string)",
            ),
            ("spr()", "#1 to 'spr' (number expected, got nil)"),
        ] {
            assert_eq!(
                draw_error(call),
                format!("main.luau:1: bad argument {detail}"),
                "{call}"
            );
        }
    }

    #[test]
    fn sprite_blocks_record_one_command_per_call() {
        assert_eq!(
            drawn("cls(1) spr(24, 7, 8, 2, 2) pset(0, 0, 2) spr(5, 0.5, -1.5, 3, 2) spr(6, 0, 0)"),
            [
                DrawCommand::Clear(1),
                sprite_block(24, 7, 8, 2, 2),
                pixel(0, 0, 2),
                sprite_block(5, 0, -2, 3, 2),
                sprite(6, 0, 0),
            ]
        );
    }

    #[test]
    fn a_missing_sprite_dimension_defaults_to_one_tile() {
        assert_eq!(
            drawn("spr(0, 0, 0, nil, nil) spr(0, 0, 0, 3) spr(0, 0, 0, nil, 4)"),
            [
                sprite(0, 0, 0),
                sprite_block(0, 0, 0, 3, 1),
                sprite_block(0, 0, 0, 1, 4),
            ]
        );
    }

    #[test]
    fn sprite_dimensions_must_be_positive_integers() {
        for (call, detail) in [
            (
                "spr(0, 0, 0, 0, 1)",
                "#4 to 'spr' (positive tile count expected, got 0)",
            ),
            (
                "spr(0, 0, 0, 1, -1)",
                "#5 to 'spr' (positive tile count expected, got -1)",
            ),
            (
                "spr(0, 0, 0, 1.5, 1)",
                "#4 to 'spr' (positive tile count expected, got 1.5)",
            ),
            (
                "spr(0, 0, 0, 1, 0/0)",
                "#5 to 'spr' (positive tile count expected, got NaN)",
            ),
            (
                r#"spr(0, 0, 0, "2", 1)"#,
                "#4 to 'spr' (number expected, got string)",
            ),
        ] {
            assert_eq!(
                draw_error(call),
                format!("main.luau:1: bad argument {detail}"),
                "{call}"
            );
        }
    }

    #[test]
    fn sprite_blocks_must_fit_the_object_tile_grid() {
        assert_eq!(
            drawn("spr(14, 0, 0, 2, 1) spr(1008, 0, 0, 16, 1)"),
            [
                sprite_block(14, 0, 0, 2, 1),
                sprite_block(1008, 0, 0, 16, 1),
            ]
        );
        assert_eq!(
            drawn("spr(0, 0, 0, 16, 64)"),
            [sprite_block(0, 0, 0, 16, 64)]
        );
        for (call, detail) in [
            (
                "spr(15, 0, 0, 2, 1)",
                "#4 to 'spr' (width 2 from tile column 15 exceeds the 16-column object-tile grid)",
            ),
            (
                "spr(1023, 0, 0, 2, 1)",
                "#4 to 'spr' (width 2 from tile column 15 exceeds the 16-column object-tile grid)",
            ),
            (
                "spr(1023, 0, 0, 1, 2)",
                "#5 to 'spr' (height 2 from tile row 63 exceeds the 64-row object-tile grid)",
            ),
            (
                "spr(0, 0, 0, 17, 1)",
                "#4 to 'spr' (width 17 from tile column 0 exceeds the 16-column object-tile grid)",
            ),
        ] {
            assert_eq!(
                draw_error(call),
                format!("main.luau:1: bad argument {detail}"),
                "{call}"
            );
        }
    }

    #[test]
    fn huge_coordinates_saturate() {
        assert_eq!(
            drawn("pset(1e300, -1e300, 1) pset(math.huge, -math.huge, 2) pset(400, -3, 3)"),
            [
                pixel(i32::MAX, i32::MIN, 1),
                pixel(i32::MAX, i32::MIN, 2),
                pixel(400, -3, 3),
            ]
        );
    }

    #[test]
    fn draw_commands_are_capped_per_frame() {
        let budget = Budget {
            interrupts: 4 * MAX_DRAW_COMMANDS as u64,
        };
        let source = format!(
            r#"
            local extra = 0
            function draw() for i = 1, {MAX_DRAW_COMMANDS} + extra do pset(0, 0, 1) end extra = 1 end
            "#
        );
        let mut runtime = Runtime::start("main.luau", source.as_bytes(), budget).unwrap();
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(runtime.frame().len(), MAX_DRAW_COMMANDS);

        let err = runtime.tick(NO_BUTTONS).unwrap_err();
        assert_eq!(err.phase, Phase::Draw);
        assert!(
            matches!(&err.error, ScriptError::Runtime(m) if m.contains("too many draw commands")),
            "{err:?}"
        );
        assert_eq!(runtime.frame().len(), MAX_DRAW_COMMANDS);
    }

    #[test]
    fn text_characters_count_toward_the_cap() {
        const CHARS: usize = MAX_DRAW_COMMANDS / TEXT_CHAR_COST;
        let fits = format!(
            r#"print(string.rep("x", {CHARS} - 1), 0, 0, 1) for i = 1, {TEXT_CHAR_COST} do pset(0, 0, 1) end"#
        );
        assert_eq!(drawn(&fits).len(), 1 + TEXT_CHAR_COST);
        for over in [
            format!(r#"print(string.rep("x", {CHARS}), 0, 0, 1) cls(0)"#),
            format!(r#"print(string.rep("x", {CHARS} + 1), 0, 0, 1)"#),
        ] {
            let message = draw_error(&over);
            assert!(message.contains("too many draw commands"), "{message}");
        }
        // Empty text still costs one character.
        let empty = format!(r#"for i = 1, {CHARS} + 1 do print("", 0, 0, 1) end"#);
        assert!(draw_error(&empty).contains("too many draw commands"));
    }

    #[test]
    fn sprites_cost_sixty_four_draw_units() {
        const SPRITES: usize = MAX_DRAW_COMMANDS / 64;
        let budget = Budget {
            interrupts: 4 * MAX_DRAW_COMMANDS as u64,
        };
        let fits = format!("function draw() for i = 1, {SPRITES} do spr(0, 0, 0) end end");
        let mut runtime = Runtime::start("main.luau", fits.as_bytes(), budget).unwrap();
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(runtime.frame().len(), SPRITES);

        let over = format!(
            "function draw() for i = 1, {} do spr(0, 0, 0) end end",
            SPRITES + 1
        );
        let mut runtime = Runtime::start("main.luau", over.as_bytes(), budget).unwrap();
        let err = runtime.tick(NO_BUTTONS).unwrap_err();
        assert!(matches!(
            err.error,
            ScriptError::Runtime(message) if message.contains("too many draw commands")
        ));
    }

    #[test]
    fn sprite_blocks_cost_sixty_four_units_per_tile() {
        const BLOCKS: usize = MAX_DRAW_COMMANDS / (4 * SPRITE_TILE_COST);
        let fits = format!("for i = 1, {BLOCKS} do spr(0, 0, 0, 2, 2) end");
        assert_eq!(drawn(&fits).len(), BLOCKS);
        let over = format!("for i = 1, {BLOCKS} do spr(0, 0, 0, 2, 2) end pset(0, 0, 1)");
        assert!(draw_error(&over).contains("too many draw commands"));

        assert_eq!(drawn("spr(0, 0, 0, 16, 64)").len(), 1);
        assert!(draw_error("spr(0, 0, 0, 16, 64) cls(0)").contains("too many draw commands"));
    }

    #[test]
    fn a_failing_draw_does_not_publish_sprite_blocks() {
        let mut runtime = started(
            r#"
            local n = 0
            function draw()
                n += 1
                spr(0, 0, 0, 2, 2)
                if n == 2 then spr(0, 0, 0, 16, 64) end
            end
            "#,
        );
        runtime.tick(NO_BUTTONS).unwrap();
        assert!(runtime.tick(NO_BUTTONS).is_err());
        assert_eq!(runtime.frame(), [sprite_block(0, 0, 0, 2, 2)]);
    }

    #[test]
    fn tile_viewer_example_draws_and_navigates_its_visible_range() {
        let source = include_str!("../../../examples/tile-viewer/code/main.luau");
        let mut runtime = started(source);
        let sprite_tiles = |runtime: &Runtime| {
            runtime
                .frame()
                .iter()
                .filter_map(|command| match command {
                    DrawCommand::Sprite { tile, .. } => Some(*tile),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };

        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(sprite_tiles(&runtime), (0..40).collect::<Vec<_>>());

        runtime.tick(buttons(&[], &[GameButton::Right])).unwrap();
        assert_eq!(sprite_tiles(&runtime), (1..41).collect::<Vec<_>>());

        runtime.tick(buttons(&[], &[GameButton::Down])).unwrap();
        assert_eq!(sprite_tiles(&runtime), (17..57).collect::<Vec<_>>());

        for _ in 0..130 {
            runtime.tick(buttons(&[], &[GameButton::Down])).unwrap();
        }
        assert_eq!(sprite_tiles(&runtime), vec![1023]);
        assert!(runtime.frame().iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text == "SELECTED TILE 1023"
        )));
        assert!(
            runtime
                .frame()
                .iter()
                .any(|command| matches!(command, DrawCommand::Pixel { .. }))
        );
    }

    // Input.

    fn buttons(held: &[GameButton], pressed: &[GameButton]) -> ButtonState {
        ButtonState {
            held: held.iter().copied().collect(),
            pressed: pressed.iter().copied().collect(),
        }
    }

    #[test]
    fn btn_and_btnp_report_the_tick_state_in_tick_and_draw() {
        let mut runtime = started(
            r#"
            function tick() log.info("tick", btn("up"), btnp("up")) end
            function draw() log.info("draw", btn("up"), btnp("up")) end
            "#,
        );
        use GameButton::Up;
        for state in [
            buttons(&[Up], &[Up]),
            buttons(&[Up], &[]),
            buttons(&[], &[]),
            buttons(&[], &[Up]),
        ] {
            runtime.tick(state).unwrap();
        }
        assert_eq!(
            texts(runtime.take_output()),
            [
                "tick\ttrue\ttrue",
                "draw\ttrue\ttrue",
                "tick\ttrue\tfalse",
                "draw\ttrue\tfalse",
                "tick\tfalse\tfalse",
                "draw\tfalse\tfalse",
                "tick\tfalse\ttrue",
                "draw\tfalse\ttrue",
            ]
        );
    }

    #[test]
    fn every_button_name_reads_its_own_button() {
        let mut runtime = started(
            r#"
            function tick()
                for _, name in { "up", "down", "left", "right", "a", "b" } do
                    log.info(name, btn(name), btnp(name))
                end
            end
            "#,
        );
        runtime
            .tick(buttons(&[GameButton::Left], &[GameButton::B]))
            .unwrap();
        assert_eq!(
            texts(runtime.take_output()),
            [
                "up\tfalse\tfalse",
                "down\tfalse\tfalse",
                "left\ttrue\tfalse",
                "right\tfalse\tfalse",
                "a\tfalse\tfalse",
                "b\tfalse\ttrue",
            ]
        );
    }

    #[test]
    fn buttons_are_released_before_the_first_tick() {
        assert_eq!(
            start_output(
                r#"log.info(btn("a"), btnp("a")) function boot() log.info(btn("b"), btnp("b")) end"#
            ),
            ["false\tfalse", "false\tfalse"]
        );
    }

    #[test]
    fn bad_button_names_are_runtime_errors() {
        for (call, detail) in [
            (
                r#"btn("start")"#,
                "#1 to 'btn' (unknown button 'start'; expected up, down, left, right, a or b)",
            ),
            (
                r#"btnp("esc")"#,
                "#1 to 'btnp' (unknown button 'esc'; expected up, down, left, right, a or b)",
            ),
            (
                r#"btn("UP")"#,
                "#1 to 'btn' (unknown button 'UP'; expected up, down, left, right, a or b)",
            ),
            ("btn(1)", "#1 to 'btn' (string expected, got number)"),
            ("btnp()", "#1 to 'btnp' (string expected, got nil)"),
        ] {
            let mut runtime = started(&format!("function tick() {call} end"));
            assert_eq!(
                runtime.tick(NO_BUTTONS),
                Err(error(
                    Phase::Tick,
                    ScriptError::Runtime(format!("main.luau:1: bad argument {detail}"))
                )),
                "{call}"
            );
        }
    }

    // Time.

    #[test]
    fn time_is_zero_during_entry_and_boot() {
        assert_eq!(
            start_output(r#"log.info(frame(), t()) function boot() log.info(frame(), t()) end"#),
            ["0\t0", "0\t0"]
        );
    }

    #[test]
    fn tick_and_draw_see_the_same_frame() {
        let mut runtime = started(
            r#"
            function tick() log.info(frame()) end
            function draw() log.info(frame()) end
            "#,
        );
        for _ in 0..3 {
            runtime.tick(NO_BUTTONS).unwrap();
        }
        assert_eq!(texts(runtime.take_output()), ["0", "0", "1", "1", "2", "2"]);
    }

    #[test]
    fn t_is_frame_over_the_tick_rate() {
        let mut runtime = started(&format!(
            r#"
            function tick()
                log.info(t() == frame() / {TICK_RATE}, frame() == {TICK_RATE} and t() == 1)
            end
            "#
        ));
        // A callback sees the ticks completed before it, so the first tick
        // to see one second is the one after `TICK_RATE` completed ticks.
        for _ in 0..=TICK_RATE {
            runtime.tick(NO_BUTTONS).unwrap();
        }
        let mut expected = vec!["true\tfalse"; TICK_RATE as usize];
        expected.push("true\ttrue");
        assert_eq!(texts(runtime.take_output()), expected);
    }

    #[test]
    fn completed_ticks_advance_the_counter() {
        let mut runtime = started("function tick() end function draw() end");
        assert_eq!(runtime.ticks.get(), 0);
        for _ in 0..TICK_RATE {
            runtime.tick(NO_BUTTONS).unwrap();
        }
        assert_eq!(runtime.ticks.get(), u64::from(TICK_RATE));
    }

    #[test]
    fn failed_tick_does_not_advance_time() {
        let mut runtime = started(r#"function tick() if frame() == 1 then error("x") end end"#);
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(runtime.ticks.get(), 1);
        assert_eq!(runtime.tick(NO_BUTTONS).unwrap_err().phase, Phase::Tick);
        assert_eq!(runtime.ticks.get(), 1);
    }

    #[test]
    fn failed_draw_does_not_advance_time() {
        let mut runtime = started(
            r#"
            function tick() end
            function draw() if frame() == 1 then error("x") end end
            "#,
        );
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(runtime.ticks.get(), 1);
        assert_eq!(runtime.tick(NO_BUTTONS).unwrap_err().phase, Phase::Draw);
        assert_eq!(runtime.ticks.get(), 1);
    }

    // Audio.

    fn gate_write(channel: usize, open: bool) -> ChannelWrite {
        ChannelWrite {
            channel,
            write: EnvelopeWrite::Gate(open),
        }
    }

    #[test]
    fn audio_configures_each_channel() {
        let mut runtime = started(
            r#"
            audio.tone(0, 440, 15, "square")
            audio.tone(1, 660, 6, "triangle")
            audio.tone(2, 220, 5, "saw")
            audio.tone(3, 1200, 4, "noise")
            audio.env(2, 10, 80, 8, 200)
            audio.gate(1, true)
            "#,
        );
        let audio = runtime.audio_mut();
        let channels = audio.channels();
        assert_eq!(
            channels.map(|channel| channel.tone),
            [
                Tone {
                    waveform: Waveform::Square,
                    frequency: 440,
                    volume: 15
                },
                Tone {
                    waveform: Waveform::Triangle,
                    frequency: 660,
                    volume: 6
                },
                Tone {
                    waveform: Waveform::Saw,
                    frequency: 220,
                    volume: 5
                },
                Tone {
                    waveform: Waveform::Noise,
                    frequency: 1200,
                    volume: 4
                },
            ]
        );
        assert_eq!(
            channels[2].envelope,
            Envelope {
                attack_ms: 10,
                decay_ms: 80,
                sustain: 8,
                release_ms: 200,
            }
        );
        assert_eq!(
            channels.map(|channel| channel.gate),
            [false, true, false, false]
        );
    }

    #[test]
    fn audio_accepts_the_range_limits() {
        let mut runtime = started(
            r#"
            audio.tone(3, 24000, 15, "noise")
            audio.tone(0, 0, 0, "square")
            audio.env(3, 10000, 10000, 0, 10000)
            audio.env(0, 0, 0, 15, 0)
            "#,
        );
        let audio = runtime.audio_mut();
        let channels = audio.channels();
        assert_eq!(channels[3].tone.frequency, MAX_FREQUENCY);
        assert_eq!(channels[3].tone.volume, MAX_VOLUME);
        assert_eq!(
            channels[3].envelope,
            Envelope {
                attack_ms: MAX_ENVELOPE_MS,
                decay_ms: MAX_ENVELOPE_MS,
                sustain: 0,
                release_ms: MAX_ENVELOPE_MS,
            }
        );
        assert_eq!(channels[0].envelope.sustain, MAX_SUSTAIN);
    }

    #[test]
    fn audio_writes_in_every_phase_are_kept_in_order() {
        let mut runtime = started(
            r#"
            audio.gate(0, true)
            function boot() audio.gate(1, true) end
            function tick() audio.gate(0, false) end
            function draw() audio.gate(1, false) end
            "#,
        );
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(
            runtime.audio_mut().writes(),
            [
                gate_write(0, true),
                gate_write(1, true),
                gate_write(0, false),
                gate_write(1, false),
            ]
        );
    }

    #[test]
    fn audio_state_persists_across_ticks() {
        let mut runtime = started(
            r#"
            function boot()
                audio.tone(2, 330, 9, "saw")
                audio.gate(2, true)
            end
            "#,
        );
        runtime.audio_mut().commit();
        for _ in 0..5 {
            runtime.tick(NO_BUTTONS).unwrap();
        }
        let audio = runtime.audio_mut();
        assert!(audio.writes().is_empty());
        assert_eq!(
            audio.channels()[2].tone,
            Tone {
                waveform: Waveform::Saw,
                frequency: 330,
                volume: 9
            }
        );
        assert!(audio.channels()[2].gate);
    }

    #[test]
    fn bad_audio_arguments_are_runtime_errors() {
        for (call, detail) in [
            (
                r#"audio.tone(4, 440, 8, "square")"#,
                "#1 to 'audio.tone' (channel 0-3 expected, got 4)",
            ),
            (
                r#"audio.tone(-1, 440, 8, "square")"#,
                "#1 to 'audio.tone' (channel 0-3 expected, got -1)",
            ),
            (
                r#"audio.tone(0.5, 440, 8, "square")"#,
                "#1 to 'audio.tone' (channel 0-3 expected, got 0.5)",
            ),
            (
                r#"audio.tone("0", 440, 8, "square")"#,
                "#1 to 'audio.tone' (number expected, got string)",
            ),
            (
                r#"audio.tone(0, 24001, 8, "square")"#,
                "#2 to 'audio.tone' (frequency 0-24000 expected, got 24001)",
            ),
            (
                r#"audio.tone(0, 440.5, 8, "square")"#,
                "#2 to 'audio.tone' (frequency 0-24000 expected, got 440.5)",
            ),
            (
                r#"audio.tone(0, 0/0, 8, "square")"#,
                "#2 to 'audio.tone' (frequency 0-24000 expected, got NaN)",
            ),
            (
                r#"audio.tone(0, 440, 16, "square")"#,
                "#3 to 'audio.tone' (volume 0-15 expected, got 16)",
            ),
            (
                r#"audio.tone(0, 440, 8, "sine")"#,
                "#4 to 'audio.tone' (unknown waveform 'sine'; expected square, triangle, saw or noise)",
            ),
            (
                r#"audio.tone(0, 440, 8, "SQUARE")"#,
                "#4 to 'audio.tone' (unknown waveform 'SQUARE'; expected square, triangle, saw or noise)",
            ),
            (
                "audio.tone(0, 440, 8)",
                "#4 to 'audio.tone' (string expected, got nil)",
            ),
            (
                "audio.tone(0, 440, 8, 1)",
                "#4 to 'audio.tone' (string expected, got number)",
            ),
            (
                "audio.env(0, 10001, 0, 15, 0)",
                "#2 to 'audio.env' (time in ms 0-10000 expected, got 10001)",
            ),
            (
                "audio.env(0, 0, -1, 15, 0)",
                "#3 to 'audio.env' (time in ms 0-10000 expected, got -1)",
            ),
            (
                "audio.env(0, 0, 0, 16, 0)",
                "#4 to 'audio.env' (sustain 0-15 expected, got 16)",
            ),
            (
                "audio.env(0, 0, 0, 15, 2.5)",
                "#5 to 'audio.env' (time in ms 0-10000 expected, got 2.5)",
            ),
            (
                "audio.env(0, 0, 0, 15)",
                "#5 to 'audio.env' (number expected, got nil)",
            ),
            (
                "audio.gate(4, true)",
                "#1 to 'audio.gate' (channel 0-3 expected, got 4)",
            ),
            (
                "audio.gate(0, 1)",
                "#2 to 'audio.gate' (boolean expected, got number)",
            ),
            (
                "audio.gate(0)",
                "#2 to 'audio.gate' (boolean expected, got nil)",
            ),
        ] {
            assert_eq!(
                runtime_message(call),
                format!("main.luau:1: bad argument {detail}"),
                "{call}"
            );
        }
    }

    #[test]
    fn rejected_audio_calls_change_nothing() {
        let mut runtime = started(
            r#"
            audio.tone(0, 440, 8, "saw")
            pcall(audio.tone, 0, 99999, 8, "square")
            pcall(audio.env, 0, 0, 0, 99, 0)
            pcall(audio.gate, 0, "yes")
            "#,
        );
        let audio = runtime.audio_mut();
        assert_eq!(
            audio.channels()[0].tone,
            Tone {
                waveform: Waveform::Saw,
                frequency: 440,
                volume: 8
            }
        );
        assert_eq!(audio.channels()[0].envelope, Envelope::default());
        assert!(audio.writes().is_empty());
    }

    #[test]
    fn too_many_envelope_writes_in_one_tick_fail() {
        let mut runtime = started(&format!(
            "function tick() for i = 1, {} do audio.gate(0, i % 2 == 1) end end",
            MAX_ENVELOPE_WRITES + 1
        ));
        assert_eq!(
            runtime.tick(NO_BUTTONS),
            Err(error(
                Phase::Tick,
                ScriptError::Runtime(format!(
                    "main.luau:1: too many envelope and gate writes in one tick (limit {MAX_ENVELOPE_WRITES})"
                ))
            ))
        );
    }

    #[test]
    fn committed_writes_free_the_budget() {
        let mut runtime = started(&format!(
            "function tick() for i = 1, {MAX_ENVELOPE_WRITES} do audio.gate(0, i % 2 == 1) end end",
        ));
        for _ in 0..3 {
            runtime.tick(NO_BUTTONS).unwrap();
            assert_eq!(runtime.audio_mut().writes().len(), MAX_ENVELOPE_WRITES);
            runtime.audio_mut().commit();
        }
    }
}
