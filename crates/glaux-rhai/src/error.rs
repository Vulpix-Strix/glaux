//! How Rhai errors become Glaux [`ScriptError`]s.

use glaux_script::{RequireFailure, ScriptError};
use rhai::{Dynamic, EvalAltResult, ParseError, Position};

/// A Glaux error carried through Rhai as the value of a runtime error, so
/// that it reaches [`script_error`] unchanged. Raised by `import` for
/// modules that cannot be provided or that failed.
#[derive(Debug, Clone)]
pub(crate) struct Failure(pub(crate) ScriptError);

/// A runtime error carrying `failure`, raised at `position`.
pub(crate) fn failure_error(failure: ScriptError, position: Position) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(Dynamic::from(Failure(failure)), position).into()
}

/// A runtime error carrying a failed `import`.
pub(crate) fn require_error(failure: RequireFailure, position: Position) -> Box<EvalAltResult> {
    failure_error(ScriptError::Require(Box::new(failure)), position)
}

/// `source:line`, or just `source` when `position` has no line.
pub(crate) fn place(source: &str, position: Position) -> String {
    match position.line() {
        Some(line) => format!("{source}:{line}"),
        None => source.to_owned(),
    }
}

/// `message` prefixed with the script line it belongs to, as Luau's errors
/// are: `main.rhai:3: boom`.
fn located(source: &str, position: Position, message: &str) -> String {
    format!("{}: {message}", place(source, position))
}

/// A compilation failure of the script named `source`.
pub(crate) fn syntax_error(err: &ParseError, source: &str) -> ScriptError {
    ScriptError::Syntax(located(source, err.position(), &err.err_type().to_string()))
}

/// The innermost error in `err`, located in the script it happened in.
/// `source` names the script `err` was raised from, used until a function
/// call frame names another.
pub(crate) fn script_error(err: &EvalAltResult, source: &str) -> ScriptError {
    let mut err = err;
    let mut source = source;
    loop {
        match err {
            EvalAltResult::ErrorInFunctionCall(_, fn_source, inner, _) => {
                if !fn_source.is_empty() {
                    source = fn_source;
                }
                err = inner;
            }
            EvalAltResult::ErrorInModule(_, inner, _) => err = inner,
            _ => break,
        }
    }

    match err {
        EvalAltResult::ErrorParsing(kind, position) => {
            ScriptError::Syntax(located(source, *position, &kind.to_string()))
        }
        EvalAltResult::ErrorRuntime(value, position) => {
            if let Some(Failure(failure)) = value.read_lock::<Failure>().as_deref() {
                return failure.clone();
            }
            ScriptError::Runtime(located(source, *position, &value_text(value)))
        }
        other => {
            let position = other.position();
            // Only the first line: the rest is a call chain already
            // reduced to its innermost frame above.
            let text = other.to_string();
            let first = text.lines().next().unwrap_or_default();
            // Rhai ends the line with `(line N, position M)`; the location
            // goes in front instead, as `source:N:`.
            let message = if position.is_none() {
                first
            } else {
                first
                    .strip_suffix(&format!(" ({position})"))
                    .unwrap_or(first)
            };
            ScriptError::Runtime(located(source, position, message))
        }
    }
}

/// The text of a thrown value: strings as they are, anything else as Rhai
/// prints it.
fn value_text(value: &Dynamic) -> String {
    if value.is_unit() {
        "()".to_owned()
    } else {
        value.to_string()
    }
}
