//! Persistent Luau runtime for cartridge scripts.
//!
//! A [`Runtime`] owns one VM for the whole time a cartridge runs. Starting it
//! executes the entry chunk once and calls the script's optional `boot()`;
//! each [`Runtime::tick`] calls the optional `tick()` and then `draw()`.
//! Scripts see Luau's safe standard library, a Glaux-owned `print`, and no
//! `require`.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use mlua::{Function, Lua, MultiValue, Value, VmState};

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

/// Why [`Runtime::start`] failed, with the text printed before the failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartFailure {
    pub error: RuntimeError,
    pub output: Vec<String>,
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

/// A live Luau VM running one cartridge script.
///
/// After any call returns an error the runtime must be dropped: the script
/// may have been interrupted halfway through updating its own state.
pub struct Runtime {
    lua: Lua,
    output: Rc<RefCell<Vec<String>>>,
    meter: Rc<Meter>,
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
    /// defines it.
    ///
    /// `chunk_name` is shown verbatim in error locations, as in
    /// `main.luau:3: boom`.
    pub fn start(chunk_name: &str, source: &[u8], budget: Budget) -> Result<Self, StartFailure> {
        let output = Rc::new(RefCell::new(Vec::new()));
        let meter = Rc::new(Meter {
            budget: budget.interrupts,
            checks: Cell::new(0),
            exhausted: Cell::new(false),
        });
        let lua = Lua::new();
        let runtime = Self { lua, output, meter };

        let started = runtime
            .call(Phase::Entry, |lua| {
                runtime.install_globals()?;
                lua.load(source).set_name(format!("={chunk_name}")).exec()
            })
            .and_then(|()| runtime.callback(Phase::Boot));
        match started {
            Ok(()) => Ok(runtime),
            Err(error) => {
                tracing::debug!(chunk = chunk_name, %error, "script failed to start");
                let mut runtime = runtime;
                Err(StartFailure {
                    error,
                    output: runtime.take_output(),
                })
            }
        }
    }

    /// Calls `tick()` and then, if it succeeded, `draw()`. A callback the
    /// script does not define is skipped.
    pub fn tick(&mut self) -> Result<(), RuntimeError> {
        self.callback(Phase::Tick)?;
        self.callback(Phase::Draw)
    }

    /// Text printed since the last call, one item per `print` call.
    pub fn take_output(&mut self) -> Vec<String> {
        std::mem::take(&mut *self.output.borrow_mut())
    }

    /// Removes `require`, replaces `print` and installs the interrupt that
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
        let tostring: Function = globals.get("tostring")?;
        let output = Rc::clone(&self.output);
        let print = self.lua.create_function(move |_, args: MultiValue| {
            let mut parts = Vec::with_capacity(args.len());
            for arg in args {
                match tostring.call::<Value>(arg)? {
                    Value::String(text) => parts.push(text.to_string_lossy()),
                    _ => {
                        return Err(mlua::Error::runtime(
                            "'tostring' must return a string to 'print'",
                        ));
                    }
                }
            }
            output.borrow_mut().push(parts.join("\t"));
            Ok(())
        })?;
        globals.raw_set("print", print)
    }

    /// Calls the global callback for `phase`, if the script defines one.
    fn callback(&self, phase: Phase) -> Result<(), RuntimeError> {
        let Some(name) = phase.callback() else {
            return Ok(());
        };
        self.call(phase, |lua| {
            // Looked up on every call so a script may replace its callbacks.
            match lua.globals().raw_get::<Value>(name)? {
                Value::Nil => Ok(()),
                Value::Function(callback) => callback.call::<()>(()),
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
    fn call(
        &self,
        phase: Phase,
        body: impl FnOnce(&Lua) -> mlua::Result<()>,
    ) -> Result<(), RuntimeError> {
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
    use pretty_assertions::assert_eq;

    const SMALL: Budget = Budget { interrupts: 10_000 };

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

    /// Starts `source` and returns what it printed while starting.
    fn start_output(source: &str) -> Vec<String> {
        started(source).take_output()
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
            print("entry")
            function boot() print("boot") end
            "#,
        );
        assert_eq!(runtime.take_output(), ["entry", "boot"]);
        for _ in 0..3 {
            runtime.tick().unwrap();
        }
        assert_eq!(runtime.take_output(), Vec::<String>::new());
    }

    #[test]
    fn missing_callbacks_are_skipped() {
        let mut runtime = started("local x = 1");
        assert_eq!(runtime.tick(), Ok(()));
        assert_eq!(runtime.tick(), Ok(()));
    }

    #[test]
    fn syntax_errors_name_the_chunk_and_print_nothing() {
        let failure = start_failure("print(\"never\")\nlocal = 1");
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
        let failure = start_failure(r#"print("before") error("boom")"#);
        assert_eq!(failure.output, ["before"]);
        assert_eq!(failure.error.phase, Phase::Entry);

        let failure =
            start_failure(r#"print("entry") function boot() print("boot") error("x") end"#);
        assert_eq!(failure.output, ["entry", "boot"]);
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

    // Printing.

    #[test]
    fn print_passes_text_out() {
        assert_eq!(start_output(r#"print("hi")"#), ["hi"]);
    }

    #[test]
    fn print_joins_arguments_with_tabs() {
        assert_eq!(
            start_output(r#"print("a", 1, true, nil) print()"#),
            ["a\t1\ttrue\tnil", ""]
        );
    }

    #[test]
    fn print_honors_tostring_metamethods() {
        let source = r#"
            local owl = setmetatable({}, { __tostring = function() return "OWL" end })
            print(owl, 2)
        "#;
        assert_eq!(start_output(source), ["OWL\t2"]);
    }

    #[test]
    fn print_uses_tostring_as_it_was_before_the_script() {
        let source = r#"tostring = function() return "x" end print(1)"#;
        assert_eq!(start_output(source), ["1"]);
    }

    #[test]
    fn print_rejects_a_non_string_tostring_result() {
        let source = r#"print(setmetatable({}, { __tostring = function() return {} end }))"#;
        let message = runtime_message(source);
        assert!(message.contains("must return a string"), "{message}");
    }

    #[test]
    fn take_output_drains_the_buffer() {
        let mut runtime = started(r#"function tick() print("t") end"#);
        runtime.tick().unwrap();
        runtime.tick().unwrap();
        assert_eq!(runtime.take_output(), ["t", "t"]);
        assert_eq!(runtime.take_output(), Vec::<String>::new());
    }

    // Ticking.

    #[test]
    fn tick_runs_once_per_call_and_draw_follows_it() {
        let mut runtime = started(
            r#"
            function tick() print("tick") end
            function draw() print("draw") end
            "#,
        );
        for _ in 0..3 {
            runtime.tick().unwrap();
        }
        assert_eq!(
            runtime.take_output(),
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
            function draw() print(count, #seen) end
            "#,
        );
        for _ in 0..3 {
            runtime.tick().unwrap();
        }
        assert_eq!(runtime.take_output(), ["1\t1", "2\t2", "3\t3"]);
    }

    #[test]
    fn callbacks_are_looked_up_on_every_call() {
        let mut runtime = started(
            r#"
            function tick()
                print("first")
                tick = function() print("second") end
            end
            "#,
        );
        runtime.tick().unwrap();
        runtime.tick().unwrap();
        assert_eq!(runtime.take_output(), ["first", "second"]);
    }

    #[test]
    fn a_failing_tick_skips_draw() {
        let mut runtime = started(
            r#"
            function tick() print("tick") error("boom") end
            function draw() print("draw") end
            "#,
        );
        assert_eq!(
            runtime.tick(),
            Err(error(
                Phase::Tick,
                ScriptError::Runtime("main.luau:2: boom".into())
            ))
        );
        assert_eq!(runtime.take_output(), ["tick"]);
    }

    #[test]
    fn draw_errors_name_the_draw_phase() {
        let mut runtime = started(r#"function draw() error("bad") end"#);
        assert_eq!(
            runtime.tick(),
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
        let err = runtime.tick().unwrap_err();
        assert_eq!(err.phase, Phase::Tick);
        assert!(
            matches!(&err.error, ScriptError::Runtime(m) if m.contains("'tick' must be a function")),
            "{err:?}"
        );
    }

    // Execution limits.

    const RUNAWAY: &str = "while true do end";
    const CAUGHT_RUNAWAY: &str = "pcall(function() while true do end end) print(\"after\")";

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
                assert_eq!(runtime.tick(), Err(limit(phase)), "{name}: {body}");
                assert!(
                    !runtime.take_output().contains(&"after".to_owned()),
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
            assert_eq!(runtime.tick(), Ok(()));
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
}
