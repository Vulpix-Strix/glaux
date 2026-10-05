//! One-shot Luau execution for cartridge scripts.
//!
//! Each [`run`] creates a fresh VM, executes one chunk and drops the VM.
//! Scripts see Luau's safe standard library, a Glaux-owned `print`, and no
//! `require`.

use std::cell::Cell;
use std::rc::Rc;

use mlua::{Function, Lua, MultiValue, Value, VmState};

/// Interrupt checks a run may pass with [`Budget::default`].
///
/// Measured in a debug build, `while true do end` passes about 15 million
/// checks per second, so a runaway loop stops in roughly a third of a second.
pub const DEFAULT_INTERRUPT_BUDGET: u64 = 5_000_000;

/// How long a script may execute before it is stopped.
///
/// Counted in Luau interrupt checks, not VM instructions: Luau checks at loop
/// back-edges and function calls, so straight-line code costs nothing and
/// each loop iteration or call costs one check.
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
/// the chunk name given to [`run`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptError {
    #[error("SYNTAX ERROR: {0}")]
    Syntax(String),
    #[error("RUNTIME ERROR: {0}")]
    Runtime(String),
    #[error("EXECUTION LIMIT EXCEEDED")]
    LimitExceeded,
}

/// Runs `source` once in a fresh Luau VM, passing the text of each `print`
/// call to `print`.
///
/// `chunk_name` is shown verbatim in error locations, as in
/// `main.luau:3: boom`. Output printed before a failure has already been
/// passed to `print` when the error is returned.
pub fn run(
    chunk_name: &str,
    source: &[u8],
    budget: Budget,
    print: &mut dyn FnMut(String),
) -> Result<(), ScriptError> {
    let lua = Lua::new();
    let exhausted = Rc::new(Cell::new(false));
    install_budget(&lua, budget, Rc::clone(&exhausted));

    let result = lua.scope(|scope| {
        let globals = lua.globals();
        // The default Luau `require` resolves modules on the host filesystem.
        globals.raw_set("require", Value::Nil)?;
        let tostring: Function = globals.get("tostring")?;
        let print_fn = scope.create_function_mut(move |_, args: MultiValue| {
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
            print(parts.join("\t"));
            Ok(())
        })?;
        globals.raw_set("print", print_fn)?;
        lua.load(source).set_name(format!("={chunk_name}")).exec()
    });

    // Exhaustion wins over whatever the chunk returned: a script may have
    // caught the limit error with `pcall` and finished normally.
    if exhausted.get() {
        return Err(ScriptError::LimitExceeded);
    }
    result.map_err(|err| {
        tracing::debug!(chunk = chunk_name, error = %err, "script failed");
        script_error(&err)
    })
}

/// Fails every interrupt check once `budget` checks have passed. The failure
/// is sticky so a `pcall` around a runaway loop cannot reset it.
fn install_budget(lua: &Lua, budget: Budget, exhausted: Rc<Cell<bool>>) {
    let checks = Cell::new(0u64);
    lua.set_interrupt(move |_| {
        if !exhausted.get() {
            let passed = checks.get() + 1;
            checks.set(passed);
            if passed <= budget.interrupts {
                return Ok(VmState::Continue);
            }
            exhausted.set(true);
        }
        Err(mlua::Error::runtime("execution limit exceeded"))
    });
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

    /// Runs `source` as `main.luau` and returns what it printed and how it
    /// ended.
    fn exec(source: &str) -> (Vec<String>, Result<(), ScriptError>) {
        let mut printed = Vec::new();
        let result = run("main.luau", source.as_bytes(), SMALL, &mut |text| {
            printed.push(text)
        });
        (printed, result)
    }

    fn runtime_message(source: &str) -> String {
        match exec(source).1 {
            Err(ScriptError::Runtime(message)) => message,
            other => panic!("expected a runtime error, got {other:?}"),
        }
    }

    #[test]
    fn runs_a_script_without_output() {
        assert_eq!(exec("local x = 1 + 2"), (vec![], Ok(())));
    }

    #[test]
    fn print_passes_text_out() {
        assert_eq!(exec(r#"print("hi")"#), (vec!["hi".to_owned()], Ok(())));
    }

    #[test]
    fn print_joins_arguments_with_tabs() {
        assert_eq!(
            exec(r#"print("a", 1, true, nil) print()"#).0,
            ["a\t1\ttrue\tnil", ""]
        );
    }

    #[test]
    fn print_honors_tostring_metamethods() {
        let source = r#"
            local owl = setmetatable({}, { __tostring = function() return "OWL" end })
            print(owl, 2)
        "#;
        assert_eq!(exec(source), (vec!["OWL\t2".to_owned()], Ok(())));
    }

    #[test]
    fn print_uses_tostring_as_it_was_before_the_script() {
        let source = r#"tostring = function() return "x" end print(1)"#;
        assert_eq!(exec(source).0, ["1"]);
    }

    #[test]
    fn print_rejects_a_non_string_tostring_result() {
        let source = r#"print(setmetatable({}, { __tostring = function() return {} end }))"#;
        let message = runtime_message(source);
        assert!(message.contains("must return a string"), "{message}");
    }

    #[test]
    fn syntax_errors_name_the_chunk() {
        let (printed, result) = exec("print(\"never\")\nlocal = 1");
        assert!(printed.is_empty());
        match result {
            Err(ScriptError::Syntax(message)) => {
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
    fn output_before_a_runtime_error_is_kept() {
        let (printed, result) = exec(r#"print("before") error("boom")"#);
        assert_eq!(printed, ["before"]);
        assert!(matches!(result, Err(ScriptError::Runtime(_))));
    }

    #[test]
    fn runaway_loops_exhaust_the_budget() {
        assert_eq!(exec("while true do end").1, Err(ScriptError::LimitExceeded));
        assert_eq!(
            exec("local function f() for i = 1, math.huge do end end f()").1,
            Err(ScriptError::LimitExceeded)
        );
    }

    #[test]
    fn pcall_cannot_swallow_the_limit() {
        let source = r#"
            pcall(function() while true do end end)
            print("after")
        "#;
        assert_eq!(exec(source).1, Err(ScriptError::LimitExceeded));
    }

    #[test]
    fn bounded_loops_finish_within_the_budget() {
        assert_eq!(exec("for i = 1, 100 do end").1, Ok(()));
    }

    #[test]
    fn require_is_unavailable() {
        let message = runtime_message(r#"require("x")"#);
        assert!(message.starts_with("main.luau:1:"), "{message}");
    }
}
