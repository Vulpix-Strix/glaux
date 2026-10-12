//! The persistent engine, its lifecycle and the budget each call runs under.

use std::cell::{Cell, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use glaux_audio::AudioState;
use glaux_input::InputState;
use glaux_modules::ModuleSource;
use glaux_script::{
    DrawCommand, LogEntry, LogLevel, Phase, Recording, RuntimeError, ScriptError, StartFailure,
};
use rhai::{AST, CallFnOptions, Dynamic, Engine, EvalAltResult, FnAccess, Map, Module, Scope};

use crate::api;
use crate::api::draw::PendingFrame;
use crate::error::{script_error, syntax_error};
use crate::modules::Resolver;

/// Rhai operations one call may perform with [`Budget::default`].
///
/// Measured in a debug build, `let x = 0; loop { x += 1; }` performs about
/// 4.8 million operations per second, so a runaway call stops in roughly a
/// third of a second.
pub const DEFAULT_OPERATION_BUDGET: u64 = 1_500_000;

/// Deepest nesting of script function calls, including closures and calls
/// made through native functions such as `map`.
///
/// Rhai's frames are large in debug builds: this depth was measured to end
/// in a clean `Stack overflow` error on a 2 MiB thread, where 80 levels
/// overflow the native stack and abort the process.
pub(crate) const MAX_CALL_LEVELS: usize = 64;

/// Longest string a script may build, in bytes.
pub(crate) const MAX_STRING_SIZE: usize = 16 * 1024;

/// Most elements one array may hold, counting nested arrays' elements.
pub(crate) const MAX_ARRAY_SIZE: usize = 16 * 1024;

/// Most properties one object map may hold.
pub(crate) const MAX_MAP_SIZE: usize = 4 * 1024;

/// Most modules one call may import.
pub(crate) const MAX_MODULES: usize = 256;

/// Deepest expression nesting, at the top level and inside functions.
const MAX_EXPR_DEPTHS: (usize, usize) = (64, 32);

/// How long each call into a script may execute before it is stopped.
///
/// The budget applies separately to the entry script and to every callback
/// call. It is counted in Rhai operations: every statement, expression
/// evaluation, loop iteration and function call costs at least one, so
/// unlike Luau's budget, straight-line code is not free. Code Rhai's
/// optimizer removes because it has no effect, such as an empty loop over a
/// range, costs nothing because it never runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub operations: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            operations: DEFAULT_OPERATION_BUDGET,
        }
    }
}

/// Counts operations against the budget of the current call. Modules are
/// evaluated with their own Rhai state, so the count lives here, shared by
/// every evaluation within a call, rather than in Rhai.
#[derive(Debug)]
pub(crate) struct Meter {
    pub(crate) budget: u64,
    pub(crate) operations: Cell<u64>,
    pub(crate) exhausted: Cell<bool>,
    /// Path of the innermost module whose top-level code was running when
    /// the budget ran out, if any.
    pub(crate) module: RefCell<Option<String>>,
}

impl Meter {
    fn reset(&self) {
        self.operations.set(0);
        self.exhausted.set(false);
        self.module.borrow_mut().take();
    }
}

/// Callbacks are called through these script functions, merged into the
/// entry script's functions: Rhai restores a function's imports and global
/// constants when a script calls it, but not when the host calls it
/// directly. Each calls its callback as a method, so `this` stays bound.
/// Names starting with `__glaux_` are reserved for them.
const TRAMPOLINES: &str = "
fn __glaux_boot() { this.boot() }
fn __glaux_tick() { this.tick() }
fn __glaux_draw() { this.draw() }
";

/// How the script defines the callback of one phase.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Callback {
    Missing,
    Defined,
    /// Defined, but not callable as a callback, for this reason.
    Invalid(String),
}

/// A live Rhai engine running one cartridge script.
///
/// After any call returns an error the runtime must be dropped: the script
/// may have been stopped halfway through updating its own state.
pub struct Runtime {
    engine: Engine,
    chunk_name: String,
    /// The entry script's functions, each carrying the imports and global
    /// constants its top level left, plus the [`TRAMPOLINES`].
    functions: AST,
    /// `boot`, `tick` and `draw`, in that order.
    callbacks: [Callback; 3],
    /// The object bound as `this` in every callback.
    state: Dynamic,
    output: Rc<RefCell<Vec<LogEntry>>>,
    pending: PendingFrame,
    frame: Vec<DrawCommand>,
    input: Rc<Cell<InputState>>,
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
    /// Compiles `source`, runs its top level once and calls `boot()` if the
    /// script defines it. `import` loads modules from `modules`, which the
    /// runtime keeps for its whole life.
    ///
    /// `chunk_name` is shown verbatim in error locations, as in
    /// `main.rhai:3: boom`. `source` must be UTF-8 Rhai text.
    pub fn start(
        chunk_name: &str,
        source: &[u8],
        modules: impl ModuleSource + 'static,
        budget: Budget,
    ) -> Result<Self, StartFailure> {
        let meter = Rc::new(Meter {
            budget: budget.operations,
            operations: Cell::new(0),
            exhausted: Cell::new(false),
            module: RefCell::default(),
        });
        let output: Rc<RefCell<Vec<LogEntry>>> = Rc::default();
        let pending: PendingFrame = Rc::default();
        let input: Rc<Cell<InputState>> = Rc::default();
        let ticks: Rc<Cell<u64>> = Rc::default();
        let audio: Rc<RefCell<AudioState>> = Rc::default();

        let mut engine = Engine::new();
        configure(&mut engine, &meter, &output);
        api::compare::install(&mut engine);
        api::draw::install(&mut engine, &pending);
        api::log::install(&mut engine, &output);
        api::input::install(&mut engine, &input);
        api::time::install(&mut engine, &ticks);
        api::audio::install(&mut engine, &audio);
        engine.set_module_resolver(Resolver::new(Rc::new(modules), Rc::clone(&meter)));

        let (functions, callbacks) = match run_entry(&engine, &meter, chunk_name, source) {
            Ok(entry) => entry,
            Err(error) => {
                let error = RuntimeError {
                    phase: Phase::Entry,
                    error,
                };
                tracing::debug!(chunk = chunk_name, %error, "script failed to start");
                return Err(StartFailure {
                    error,
                    output: std::mem::take(&mut *output.borrow_mut()),
                });
            }
        };

        let mut runtime = Self {
            engine,
            chunk_name: chunk_name.to_owned(),
            functions,
            callbacks,
            state: Map::new().into(),
            output,
            pending,
            frame: Vec::new(),
            input,
            ticks,
            audio,
            meter,
        };
        match runtime.callback(Phase::Boot) {
            Ok(_) => Ok(runtime),
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
    /// `btn`, `btnp`, `key` and `keyp` report `input` during both
    /// callbacks, and keep reporting it until the next tick.
    ///
    /// A `draw()` that returns normally replaces [`frame`](Self::frame) with
    /// the commands it recorded; one that fails leaves the previous frame.
    ///
    /// A successful call advances the logical time read by `frame()` and
    /// `t()`; a failed one does not.
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
    /// engine; whoever applies it commits the writes it has applied.
    pub fn audio_mut(&mut self) -> RefMut<'_, AudioState> {
        self.audio.borrow_mut()
    }

    /// Calls the callback for `phase` with the state bound as `this`, if the
    /// script defines it. Returns whether it did.
    fn callback(&mut self, phase: Phase) -> Result<bool, RuntimeError> {
        let (Some(name), Some(index)) = (phase.callback(), callback_index(phase)) else {
            return Ok(false);
        };
        match &self.callbacks[index] {
            Callback::Missing => return Ok(false),
            Callback::Invalid(reason) => {
                return Err(RuntimeError {
                    phase,
                    error: ScriptError::Runtime(reason.clone()),
                });
            }
            Callback::Defined => {}
        }

        self.meter.reset();
        let options = CallFnOptions::new()
            .eval_ast(false)
            .bind_this_ptr(&mut self.state);
        let result = self.engine.call_fn_with_options::<Dynamic>(
            options,
            &mut Scope::new(),
            &self.functions,
            &format!("__glaux_{name}"),
            (),
        );
        // Rhai checks a map's size when it is passed or returned, not when
        // an index assignment grows it, so state kept in `this` is checked
        // after every call: otherwise it could grow without bound, a few
        // entries per tick.
        let result = result.and_then(|_| self.engine.ensure_data_size_within_limits(&self.state));
        finish(&self.meter, &self.chunk_name, phase, result).map(|()| true)
    }
}

/// Position of `phase`'s callback in [`Runtime::callbacks`].
fn callback_index(phase: Phase) -> Option<usize> {
    match phase {
        Phase::Entry => None,
        Phase::Boot => Some(0),
        Phase::Tick => Some(1),
        Phase::Draw => Some(2),
    }
}

/// Compiles and runs the entry script's top level, returning its functions
/// ready to call and how it defines each callback.
fn run_entry(
    engine: &Engine,
    meter: &Meter,
    chunk_name: &str,
    source: &[u8],
) -> Result<(AST, [Callback; 3]), ScriptError> {
    let text = std::str::from_utf8(source)
        .map_err(|_| ScriptError::Syntax(format!("{chunk_name}: script is not valid UTF-8")))?;
    let mut ast = engine
        .compile(text)
        .map_err(|err| syntax_error(&err, chunk_name))?;
    ast.set_source(chunk_name);

    let callbacks = [Phase::Boot, Phase::Tick, Phase::Draw].map(|phase| {
        let name = phase.callback().unwrap_or_default();
        let mut found = Callback::Missing;
        for function in ast.iter_functions().filter(|f| f.name == name) {
            if function.params.is_empty() {
                if function.access == FnAccess::Private {
                    found = Callback::Invalid(format!("'{name}' must not be private"));
                } else {
                    return Callback::Defined;
                }
            } else if found == Callback::Missing {
                found = Callback::Invalid(format!("'{name}' must take no parameters"));
            }
        }
        found
    });

    meter.reset();
    let module = Module::eval_ast_as_new(Scope::new(), &ast, engine);
    let module = finish(meter, chunk_name, Phase::Entry, module).map_err(|err| err.error)?;

    let trampolines = engine
        .compile(TRAMPOLINES)
        .map_err(|err| syntax_error(&err, "trampolines"))?;
    let mut functions = AST::new_from_module(module).merge(&trampolines);
    functions.set_source(chunk_name);
    Ok((functions, callbacks))
}

/// Reports exhaustion over whatever the call returned, then maps the call's
/// own error.
fn finish<T>(
    meter: &Meter,
    chunk_name: &str,
    phase: Phase,
    result: Result<T, Box<EvalAltResult>>,
) -> Result<T, RuntimeError> {
    if meter.exhausted.get() {
        let error = match meter.module.borrow_mut().take() {
            Some(path) => ScriptError::ModuleLimitExceeded(path),
            None => ScriptError::LimitExceeded,
        };
        return Err(RuntimeError { phase, error });
    }
    result.map_err(|err| RuntimeError {
        phase,
        error: script_error(&err, chunk_name),
    })
}

/// Applies the limits, the execution budget and the logging hooks, and
/// removes what would let a script reach outside its cartridge or around
/// its source locations.
fn configure(engine: &mut Engine, meter: &Rc<Meter>, output: &Rc<RefCell<Vec<LogEntry>>>) {
    let progress = Rc::clone(meter);
    // Once exhausted the call keeps failing: Rhai reports termination as a
    // system error that `try`/`catch` cannot catch, and every further
    // operation terminates again.
    engine.on_progress(move |_| {
        if !progress.exhausted.get() {
            let passed = progress.operations.get() + 1;
            progress.operations.set(passed);
            if passed <= progress.budget {
                return None;
            }
            progress.exhausted.set(true);
        }
        Some(Dynamic::UNIT)
    });

    engine
        .set_max_call_levels(MAX_CALL_LEVELS)
        .set_max_expr_depths(MAX_EXPR_DEPTHS.0, MAX_EXPR_DEPTHS.1)
        .set_max_string_size(MAX_STRING_SIZE)
        .set_max_array_size(MAX_ARRAY_SIZE)
        .set_max_map_size(MAX_MAP_SIZE)
        .set_max_modules(MAX_MODULES);

    // `eval` would run code that has no cartridge file to locate errors in.
    engine.disable_symbol("eval");

    let log = Rc::clone(output);
    engine.on_print(move |text| {
        log.borrow_mut().push(LogEntry {
            level: LogLevel::Info,
            text: text.to_owned(),
        });
    });
    let log = Rc::clone(output);
    engine.on_debug(move |text, _, _| {
        log.borrow_mut().push(LogEntry {
            level: LogLevel::Debug,
            text: text.to_owned(),
        });
    });
}
