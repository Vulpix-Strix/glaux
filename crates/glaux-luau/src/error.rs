//! Script failures and how they are reported.

use std::fmt;

use glaux_modules::ModuleNameError;

use crate::api::log::LogEntry;
use crate::runtime::Phase;

/// A script that failed to compile or run. Messages name locations only by
/// the chunk name given to [`Runtime::start`](crate::Runtime::start) or by cartridge-relative module
/// paths, as in `code/game/enemies.luau:3: boom`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptError {
    #[error("SYNTAX ERROR: {0}")]
    Syntax(String),
    #[error("RUNTIME ERROR: {0}")]
    Runtime(String),
    #[error("EXECUTION LIMIT EXCEEDED")]
    LimitExceeded,
    /// The budget ran out while the top-level code of the module at this
    /// path was running.
    #[error("EXECUTION LIMIT EXCEEDED IN {0}")]
    ModuleLimitExceeded(String),
    #[error("{0}")]
    Require(Box<RequireFailure>),
}

/// Why `require` could not provide a module. Paths are cartridge-relative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequireError {
    InvalidName {
        name: String,
        reason: ModuleNameError,
    },
    NotFound(String),
    Unreadable(String),
    /// The modules being loaded, from the first one in the cycle to the one
    /// required again.
    Cycle(Vec<String>),
}

impl RequireError {
    fn headline(&self) -> &'static str {
        match self {
            Self::InvalidName { .. } => "INVALID MODULE NAME",
            Self::NotFound(_) => "MODULE NOT FOUND",
            Self::Unreadable(_) => "CANNOT READ MODULE",
            Self::Cycle(_) => "CYCLIC REQUIRE",
        }
    }

    fn detail(&self) -> String {
        match self {
            Self::InvalidName { name, reason } => format!("{name:?} - {reason}"),
            Self::NotFound(path) | Self::Unreadable(path) => path.clone(),
            Self::Cycle(chain) => chain.join(" -> "),
        }
    }
}

/// A failed `require` and the `file:line` that called it, when the caller
/// was script code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequireFailure {
    pub at: Option<String>,
    pub error: RequireError,
}

impl RequireFailure {
    /// The message, naming `callback` (uppercase) when it failed in one, as
    /// in `MODULE NOT FOUND IN TICK: code/x.luau (code/main.luau:3)`.
    fn message(&self, callback: Option<&str>) -> String {
        let mut text = self.error.headline().to_owned();
        if let Some(callback) = callback {
            text.push_str(" IN ");
            text.push_str(callback);
        }
        text.push_str(": ");
        text.push_str(&self.error.detail());
        if let Some(at) = &self.at {
            text.push_str(&format!(" ({at})"));
        }
        text
    }
}

impl fmt::Display for RequireFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message(None))
    }
}

impl std::error::Error for RequireFailure {}

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
            ScriptError::ModuleLimitExceeded(path) => {
                write!(f, "EXECUTION LIMIT EXCEEDED IN {callback}: {path}")
            }
            ScriptError::Require(failure) => f.write_str(&failure.message(Some(&callback))),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Why [`Runtime::start`](crate::Runtime::start) failed, with the text logged before the failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartFailure {
    pub error: RuntimeError,
    pub output: Vec<LogEntry>,
}

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
