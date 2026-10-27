//! The persistent VM, its lifecycle and the budget each call runs under.

use std::cell::{Cell, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use glaux_audio::AudioState;
use glaux_input::InputState;
use glaux_modules::ModuleSource;
use glaux_script::debug::{Breakpoint, DebugPort};
use mlua::chunk::ChunkMode;
use mlua::{Function, Lua, Value, VmState};

use glaux_script::{
    DrawCommand, LogEntry, LogSink, Phase, Recording, RuntimeError, ScriptError, StartFailure,
};

use crate::api;
use crate::api::draw::PendingFrame;
use crate::debug::{self, Attachment};
use crate::error::script_error;
use crate::modules::{self, Modules};

/// Interrupt checks one call may pass with [`Budget::default`].
///
/// Measured in a debug build, `while true do end` passes about 15 million
/// checks per second, so a runaway call stops in roughly a third of a second.
pub const DEFAULT_INTERRUPT_BUDGET: u64 = 5_000_000;

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

/// Counts interrupt checks against the budget of the current call.
#[derive(Debug)]
pub(crate) struct Meter {
    pub(crate) budget: u64,
    pub(crate) checks: Cell<u64>,
    pub(crate) exhausted: Cell<bool>,
    /// Path of the innermost module whose top-level code was running when
    /// the budget ran out, if any.
    pub(crate) module: RefCell<Option<String>>,
}

impl Meter {
    fn reset(&self) {
        self.checks.set(0);
        self.exhausted.set(false);
        self.module.borrow_mut().take();
    }
}

/// A live Luau VM running one cartridge script.
///
/// After any call returns an error the runtime must be dropped, since the
/// script may have been interrupted halfway through updating its own state.
pub struct Runtime {
    lua: Lua,
    output: Rc<LogSink>,
    pending: PendingFrame,
    frame: Vec<DrawCommand>,
    /// The buttons and keys of the tick in progress.
    input: Rc<Cell<InputState>>,
    /// Logical ticks completed so far, as read by `frame()` and `t()`.
    ticks: Rc<Cell<u64>>,
    audio: Rc<RefCell<AudioState>>,
    meter: Rc<Meter>,
    modules: Rc<RefCell<Modules>>,
    source: Rc<dyn ModuleSource>,
    /// Present when the VM is being debugged.
    debug: Option<Attachment>,
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
    /// defines it. `require` loads modules from `modules`, which the runtime
    /// keeps for its whole life.
    ///
    /// `chunk_name` is shown verbatim in error locations, as in
    /// `main.luau:3: boom`. `source` must be Luau text, and bytecode is
    /// rejected.
    pub fn start(
        chunk_name: &str,
        source: &[u8],
        modules: impl ModuleSource + 'static,
        budget: Budget,
    ) -> Result<Self, StartFailure> {
        Self::start_with(chunk_name, source, modules, budget, None)
    }

    /// Like [`start`](Self::start), for a VM that stops at `breakpoints` and
    /// wherever the debugger at the other end of `port` steps.
    ///
    /// The scripts are compiled without optimizations and with the debug
    /// information needed to name their variables, so the VM runs slower
    /// than one started with [`start`](Self::start). Breakpoints are
    /// cartridge-relative paths, as in `code/main.luau`.
    pub fn start_debug(
        chunk_name: &str,
        source: &[u8],
        modules: impl ModuleSource + 'static,
        budget: Budget,
        port: DebugPort,
        breakpoints: Vec<Breakpoint>,
    ) -> Result<Self, StartFailure> {
        let debug = Some((port, breakpoints));
        Self::start_with(chunk_name, source, modules, budget, debug)
    }

    fn start_with(
        chunk_name: &str,
        source: &[u8],
        modules: impl ModuleSource + 'static,
        budget: Budget,
        debug: Option<(DebugPort, Vec<Breakpoint>)>,
    ) -> Result<Self, StartFailure> {
        let meter = Rc::new(Meter {
            budget: budget.interrupts,
            checks: Cell::new(0),
            exhausted: Cell::new(false),
            module: RefCell::default(),
        });
        let lua = Lua::new();
        let debug = match debug
            .map(|(port, breakpoints)| Attachment::new(&lua, port, breakpoints))
            .transpose()
        {
            Ok(debug) => debug,
            Err(err) => {
                return Err(StartFailure {
                    error: RuntimeError {
                        phase: Phase::Entry,
                        error: script_error(&err),
                    },
                    output: Vec::new(),
                    dropped: 0,
                });
            }
        };
        let mut runtime = Self {
            lua,
            output: Rc::default(),
            pending: Rc::default(),
            frame: Vec::new(),
            input: Rc::default(),
            ticks: Rc::default(),
            audio: Rc::default(),
            meter,
            modules: Rc::default(),
            source: Rc::new(modules),
            debug,
        };

        let started = runtime
            .call(Phase::Entry, |lua| {
                runtime.install_globals()?;
                let entry = lua
                    .load(source)
                    .set_name(format!("={chunk_name}"))
                    .set_mode(ChunkMode::Text)
                    .into_function()?;
                debug::chunk_loaded(lua, chunk_name, source, &entry);
                entry.call::<()>(())
            })
            .and_then(|()| runtime.callback(Phase::Boot).map(drop));
        match started {
            Ok(()) => Ok(runtime),
            Err(error) => {
                tracing::debug!(chunk = chunk_name, %error, "script failed to start");
                Err(StartFailure {
                    error,
                    output: runtime.take_output(),
                    dropped: runtime.take_dropped(),
                })
            }
        }
    }

    /// Calls `tick()` and then, if it succeeded, `draw()`. A callback the
    /// script does not define is skipped.
    ///
    /// `btn`, `btnp`, `key` and `keyp` report `input` during both
    /// callbacks, and keep reporting it until the next tick.
    ///
    /// A `draw()` that returns normally replaces [`frame`](Self::frame) with
    /// the commands it recorded. One that fails leaves the previous frame.
    ///
    /// A successful call advances the logical time read by `frame()` and
    /// `t()`. A failed one does not.
    pub fn tick(&mut self, input: InputState) -> Result<(), RuntimeError> {
        self.input.set(input);
        self.callback(Phase::Tick)?;

        *self.pending.borrow_mut() = Some(Recording::default());
        let drawn = self.callback(Phase::Draw);
        let recording = self.pending.borrow_mut().take();
        if drawn? && let Some(recording) = recording {
            self.frame = recording.into_commands();
        }
        self.ticks.set(self.ticks.get() + 1);
        Ok(())
    }

    /// The VM, for tests that inspect what the runtime installed.
    #[cfg(test)]
    pub(crate) fn lua(&self) -> &Lua {
        &self.lua
    }

    /// The commands recorded by the last `draw()` that completed, in call
    /// order. Empty until one does.
    pub fn frame(&self) -> &[DrawCommand] {
        &self.frame
    }

    /// Entries logged since the last call, oldest first.
    pub fn take_output(&mut self) -> Vec<LogEntry> {
        self.output.take()
    }

    /// Takes how many log entries were refused since the last call because
    /// the script logged more than the output keeps per take.
    pub fn take_dropped(&mut self) -> u32 {
        self.output.take_dropped()
    }

    /// The audio the script has written. It persists for the life of the
    /// VM. Whoever applies it commits the writes it has applied.
    pub fn audio_mut(&mut self) -> RefMut<'_, AudioState> {
        self.audio.borrow_mut()
    }

    /// Installs the Glaux API, including the cartridge `require`, and the
    /// interrupt that enforces the budget.
    fn install_globals(&self) -> mlua::Result<()> {
        let meter = Rc::clone(&self.meter);
        let stopped = self.debug.as_ref().map(Attachment::stopper);
        // The failure is sticky for the rest of the call so a `pcall` around
        // a runaway loop cannot resume it.
        self.lua.set_interrupt(move |_| {
            if stopped.as_ref().is_some_and(|stopped| stopped()) {
                return Err(mlua::Error::runtime("stopped by the debugger"));
            }
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
        // Captured before the script runs so that redefining the global
        // cannot change how Glaux converts values to text.
        let tostring: Function = globals.get("tostring")?;
        api::log::install(&self.lua, &tostring, &self.output)?;
        api::draw::install(&self.lua, &tostring, &self.pending)?;
        api::input::install(&self.lua, &self.input)?;
        api::time::install(&self.lua, &self.ticks)?;
        api::audio::install(&self.lua, &self.audio)?;
        modules::install(&self.lua, &self.modules, &self.source, &self.meter)
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
    /// `body` returned, because a script may have caught the limit error with
    /// `pcall` and finished normally.
    fn call<T>(
        &self,
        phase: Phase,
        body: impl FnOnce(&Lua) -> mlua::Result<T>,
    ) -> Result<T, RuntimeError> {
        self.meter.reset();
        if let Some(debug) = &self.debug {
            debug.sync(&self.lua);
        }
        let result = body(&self.lua);
        if self.debug.as_ref().is_some_and(Attachment::stop_requested) {
            return Err(RuntimeError {
                phase,
                error: ScriptError::Stopped,
            });
        }
        if self.meter.exhausted.get() {
            let error = match self.meter.module.borrow_mut().take() {
                Some(path) => ScriptError::ModuleLimitExceeded(path),
                None => ScriptError::LimitExceeded,
            };
            return Err(RuntimeError { phase, error });
        }
        result.map_err(|err| RuntimeError {
            phase,
            error: script_error(&err),
        })
    }

    /// The modules this runtime has tried to load.
    #[cfg(test)]
    pub(crate) fn modules(&self) -> std::cell::Ref<'_, Modules> {
        self.modules.borrow()
    }
}
