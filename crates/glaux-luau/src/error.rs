//! How mlua errors become Glaux [`ScriptError`]s.

use glaux_script::{RequireFailure, ScriptError};

/// The first line of the innermost Lua error message, without mlua's kind
/// prefix or the stack traceback that follows it.
pub(crate) fn script_error(err: &mlua::Error) -> ScriptError {
    let mut err = err;
    while let mlua::Error::CallbackError { cause, .. } = err {
        err = cause;
    }
    let first_line = |message: &str| message.lines().next().unwrap_or("").to_owned();
    match err {
        mlua::Error::SyntaxError { message, .. } => ScriptError::Syntax(first_line(message)),
        mlua::Error::RuntimeError(message) => ScriptError::Runtime(first_line(message)),
        mlua::Error::ExternalError(external) => match external.downcast_ref::<RequireFailure>() {
            Some(failure) => ScriptError::Require(Box::new(failure.clone())),
            None => ScriptError::Runtime(first_line(&external.to_string())),
        },
        other => ScriptError::Runtime(first_line(&other.to_string())),
    }
}
