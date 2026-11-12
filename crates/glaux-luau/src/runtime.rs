//! The persistent VM, its lifecycle and the budget each call runs under.

use std::cell::{Cell, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use glaux_audio::AudioState;
use glaux_input::InputState;
use glaux_modules::ModuleSource;
use glaux_script::debug::{Breakpoint, DebugPort};
use mlua::chunk::ChunkMode;
use mlua::prelude::{LuaGcIncParams as GcIncParams, LuaGcMode as GcMode};
use mlua::{Function, Lua, Value, VmState};

use glaux_machine::font::BitmapFont;
use glaux_machine::hardware::CartridgeHardware;
use glaux_script::{
    DrawBudget, HardwareSlot, Loan, LogEntry, LogSink, Phase, RuntimeError, ScriptError,
    StartFailure,
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
/// Checks are not time, so this is not a real-time guarantee: the collector
/// and the builtins a script calls run between checks and are not counted.
pub const DEFAULT_INTERRUPT_BUDGET: u64 = 5_000_000;

/// Bytes of heap a script may use, counting garbage the collector has not
/// freed yet. An allocation that would pass it fails with an ordinary Luau
/// memory error, which `pcall` and coroutines can catch.
pub const MEMORY_LIMIT: usize = 4 << 20;

/// Heap size above which the interrupt stops the call with
/// [`ScriptError::MemoryLimitExceeded`], which a script cannot catch.
///
/// It is below [`MEMORY_LIMIT`] so that a script growing its heap is stopped
/// before an allocation fails inside a `pcall`. The gap is what a single
/// builtin such as `string.rep` can allocate between two interrupt checks.
/// This is an internal policy, not part of the machine contract.
const MEMORY_SOFT_LIMIT: usize = MEMORY_LIMIT / 8 * 7;

/// Heap size, as a percentage of live data, the collector aims for.
/// Luau's default of 200 lets garbage double the heap, which makes a script
/// with little live data reach the limit.
const GC_GOAL: i32 = 130;

/// How much collector work runs per allocated byte, as a percentage. A small
/// goal needs a large multiplier for the collector to keep up.
const GC_STEP_MULTIPLIER: i32 = 1000;

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
    /// Whether the heap went above [`MEMORY_SOFT_LIMIT`].
    memory_exhausted: Cell<bool>,
    /// Path of the innermost module whose top-level code was running when
    /// the budget ran out, if any.
    pub(crate) module: RefCell<Option<String>>,
}

impl Meter {
    fn reset(&self) {
        self.checks.set(0);
        self.exhausted.set(false);
        self.memory_exhausted.set(false);
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
    /// The draw budget of the `draw()` call in progress, if one is.
    pending: PendingFrame,
    /// The System ROM font that `print` draws with.
    font: BitmapFont,
    /// The buttons and keys of the tick in progress.
    input: Rc<Cell<InputState>>,
    /// Logical ticks completed so far, as read by `frame()` and `t()`.
    ticks: Rc<Cell<u64>>,
    audio: Rc<RefCell<AudioState>>,
    meter: Rc<Meter>,
    modules: Rc<RefCell<Modules>>,
    source: Rc<dyn ModuleSource>,
    /// Holds the cartridge hardware while a phase runs.
    hardware: HardwareSlot,
    /// The hardware that [`tick`](Self::tick) lends, for tests.
    #[cfg(test)]
    scratch: Option<Box<CartridgeHardware>>,
    /// Present when the VM is being debugged.
    debug: Option<Attachment>,
}

impl fmt::Debug for Runtime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runtime")
            .field("meter", &self.meter)
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
    /// rejected. `font` is what `print` draws text with.
    ///
    /// The hardware in `home` is lent to the entry script and to `boot()`,
    /// so the script can set up memory before the first `tick()`. It is back
    /// in `home` when this returns, whether the start succeeded or not.
    /// Without hardware the start fails before running any script.
    pub fn start(
        chunk_name: &str,
        source: &[u8],
        modules: impl ModuleSource + 'static,
        budget: Budget,
        font: BitmapFont,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<Self, StartFailure> {
        Self::start_with(chunk_name, source, modules, budget, font, home, None)
    }

    /// Like [`start`](Self::start), for a VM that stops at `breakpoints` and
    /// wherever the debugger at the other end of `port` steps.
    ///
    /// The scripts are compiled without optimizations and with the debug
    /// information needed to name their variables, so the VM runs slower
    /// than one started with [`start`](Self::start). Breakpoints are
    /// cartridge-relative paths, as in `code/main.luau`.
    ///
    /// A start that stops at a breakpoint does not return until it is resumed
    /// and finishes, so the hardware stays lent meanwhile. See
    /// [`start`](Self::start) for `home`.
    #[allow(clippy::too_many_arguments)]
    pub fn start_debug(
        chunk_name: &str,
        source: &[u8],
        modules: impl ModuleSource + 'static,
        budget: Budget,
        font: BitmapFont,
        home: &mut Option<Box<CartridgeHardware>>,
        port: DebugPort,
        breakpoints: Vec<Breakpoint>,
    ) -> Result<Self, StartFailure> {
        let debug = Some((port, breakpoints));
        Self::start_with(chunk_name, source, modules, budget, font, home, debug)
    }

    fn start_with(
        chunk_name: &str,
        source: &[u8],
        modules: impl ModuleSource + 'static,
        budget: Budget,
        font: BitmapFont,
        home: &mut Option<Box<CartridgeHardware>>,
        debug: Option<(DebugPort, Vec<Breakpoint>)>,
    ) -> Result<Self, StartFailure> {
        let meter = Rc::new(Meter {
            budget: budget.interrupts,
            checks: Cell::new(0),
            exhausted: Cell::new(false),
            memory_exhausted: Cell::new(false),
            module: RefCell::default(),
        });
        let lua = Lua::new();
        lua.gc_set_mode(GcMode::Incremental(
            GcIncParams::default()
                .goal(GC_GOAL)
                .step_multiplier(GC_STEP_MULTIPLIER),
        ));
        if let Err(err) = lua.set_memory_limit(MEMORY_LIMIT) {
            return Err(StartFailure {
                error: RuntimeError {
                    phase: Phase::Entry,
                    error: script_error(&err),
                },
                output: Vec::new(),
                dropped: 0,
            });
        }
        let output: Rc<LogSink> = Rc::default();
        let debug = match debug
            .map(|(port, breakpoints)| Attachment::new(&lua, port, Rc::clone(&output), breakpoints))
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
            output,
            pending: Rc::default(),
            font,
            input: Rc::default(),
            ticks: Rc::default(),
            audio: Rc::default(),
            meter,
            modules: Rc::default(),
            source: Rc::new(modules),
            hardware: HardwareSlot::new(),
            #[cfg(test)]
            scratch: None,
            debug,
        };

        // The loan ends before the result is used, so the hardware is back in
        // `home` on every path.
        let slot = runtime.hardware.clone();
        let started = match Loan::new(&slot, home) {
            Ok(_loan) => runtime
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
                .and_then(|()| runtime.callback(Phase::Boot).map(drop)),
            Err(error) => Err(error.in_phase(Phase::Entry)),
        };
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

    /// Calls `tick()` with the hardware lent from `home`. A callback the
    /// script does not define is skipped.
    ///
    /// `btn`, `btnp`, `key` and `keyp` report `input` during both
    /// callbacks of a tick, and keep reporting it until the next tick.
    ///
    /// `home` must hold the hardware, and holds it again when this returns,
    /// whether the call succeeded or not. Without it the call fails before
    /// running any script.
    ///
    /// A tick is complete only after [`run_draw`](Self::run_draw) succeeds,
    /// so this does not advance the logical time.
    #[hotpath::measure(label = "luau::tick", impl_type = "Runtime")]
    pub fn run_tick(
        &mut self,
        input: InputState,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<(), RuntimeError> {
        let slot = self.hardware.clone();
        let _loan = Loan::new(&slot, home).map_err(|error| error.in_phase(Phase::Tick))?;
        self.tick_callback(input)
    }

    /// Calls `draw()` with the hardware lent from `home`, after a
    /// successful [`run_tick`](Self::run_tick). See that method for `home`.
    ///
    /// The drawing functions draw on the framebuffer of the lent hardware
    /// as they are called. Whoever owns the hardware decides what to do with
    /// the result: a `draw()` that fails leaves it half drawn, and nothing
    /// here reverts it.
    ///
    /// A successful call advances the logical time read by `frame()` and
    /// `t()`. A failed one does not.
    #[hotpath::measure(label = "luau::draw", impl_type = "Runtime")]
    pub fn run_draw(
        &mut self,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<(), RuntimeError> {
        let slot = self.hardware.clone();
        let _loan = Loan::new(&slot, home).map_err(|error| error.in_phase(Phase::Draw))?;
        self.draw_callback()
    }

    /// A whole tick on a hardware of the runtime's own, for tests of the
    /// scripting layer. The hardware persists across calls, and like the
    /// system it clears the framebuffer to the backdrop color between
    /// `tick()` and `draw()`.
    #[cfg(test)]
    pub fn tick(&mut self, input: InputState) -> Result<(), RuntimeError> {
        let mut home = self.scratch.take();
        let result = self.run_tick(input, &mut home).and_then(|()| {
            if let Some(hardware) = home.as_deref_mut() {
                hardware
                    .framebuffer
                    .clear(hardware.registers.backdrop_color());
            }
            self.run_draw(&mut home)
        });
        self.scratch = home;
        result
    }

    /// Starts a runtime on the hardware of [`hardware`](crate::tests::hardware),
    /// which [`tick`](Self::tick) keeps lending after the start.
    #[cfg(test)]
    pub(crate) fn start_on_scratch(
        chunk_name: &str,
        source: &[u8],
        modules: impl ModuleSource + 'static,
        budget: Budget,
        font: BitmapFont,
    ) -> Result<Self, StartFailure> {
        let mut home = Some(crate::tests::hardware());
        let runtime = Self::start(chunk_name, source, modules, budget, font, &mut home)?;
        Ok(runtime.with_scratch(home))
    }

    /// Makes `home` the hardware that [`tick`](Self::tick) lends.
    #[cfg(test)]
    pub(crate) fn with_scratch(mut self, home: Option<Box<CartridgeHardware>>) -> Self {
        self.scratch = home;
        self
    }

    /// The hardware that [`tick`](Self::tick) lends.
    #[cfg(test)]
    pub(crate) fn scratch(&self) -> &CartridgeHardware {
        self.scratch.as_deref().expect("the hardware is back")
    }

    fn tick_callback(&mut self, input: InputState) -> Result<(), RuntimeError> {
        self.input.set(input);
        self.callback(Phase::Tick)?;
        Ok(())
    }

    fn draw_callback(&mut self) -> Result<(), RuntimeError> {
        *self.pending.borrow_mut() = Some(DrawBudget::default());
        let drawn = self.callback(Phase::Draw);
        self.pending.borrow_mut().take();
        drawn?;
        self.ticks.set(self.ticks.get() + 1);
        Ok(())
    }

    /// The slot the runtime lends the hardware through, for tests.
    #[cfg(test)]
    pub(crate) fn hardware_slot(&self) -> HardwareSlot {
        self.hardware.clone()
    }

    /// The VM, for tests that inspect what the runtime installed.
    #[cfg(test)]
    pub(crate) fn lua(&self) -> &Lua {
        &self.lua
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
        // The failures are sticky for the rest of the call so a `pcall` around
        // a runaway loop or a growing table cannot resume it.
        self.lua.set_interrupt(move |lua| {
            if stopped.as_ref().is_some_and(|stopped| stopped()) {
                return Err(mlua::Error::runtime("stopped by the debugger"));
            }
            if meter.memory_exhausted.get() || lua.used_memory() > MEMORY_SOFT_LIMIT {
                meter.memory_exhausted.set(true);
                return Err(mlua::Error::runtime("memory limit exceeded"));
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
        api::draw::install(
            &self.lua,
            &tostring,
            &self.pending,
            &self.hardware,
            &self.font,
        )?;
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
        if self.meter.memory_exhausted.get() {
            return Err(RuntimeError {
                phase,
                error: ScriptError::MemoryLimitExceeded,
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
