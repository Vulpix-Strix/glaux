//! Script failures and how they are reported.

use std::fmt;

use glaux_modules::ModuleNameError;

use crate::log::LogEntry;

/// The part of a script's lifecycle that was executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The entry chunk, executed once when the runtime starts.
    Entry,
    Boot,
    Tick,
    Draw,
}

impl Phase {
    /// The name of the script callback run in this phase, if it has one.
    pub fn callback(self) -> Option<&'static str> {
        match self {
            Self::Entry => None,
            Self::Boot => Some("boot"),
            Self::Tick => Some("tick"),
            Self::Draw => Some("draw"),
        }
    }
}

/// A script that failed to compile or run. Messages name locations only by
/// the chunk name the runtime was started with or by cartridge-relative
/// module paths, as in `code/game/enemies.luau:3: boom`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptError {
    #[error("Syntax error: {0}")]
    Syntax(String),
    #[error("Runtime error: {0}")]
    Runtime(String),
    #[error("Execution limit exceeded")]
    LimitExceeded,
    /// The budget ran out while the top-level code of the module at this
    /// path was running.
    #[error("Execution limit exceeded in {0}")]
    ModuleLimitExceeded(String),
    #[error("{0}")]
    Require(Box<RequireFailure>),
}

impl ScriptError {
    /// The `file` and 1-based `line` the error names, when its message
    /// starts with a `file:line:` position as the runtimes write it.
    /// Errors without a position, such as an exhausted budget, give `None`.
    ///
    /// The file is whatever the message names. The caller decides whether
    /// it is a script it knows.
    pub fn location(&self) -> Option<(&str, u32)> {
        match self {
            Self::Syntax(message) | Self::Runtime(message) => leading_position(message),
            Self::Require(failure) => failure.at.as_deref().and_then(trailing_position),
            Self::LimitExceeded | Self::ModuleLimitExceeded(_) => None,
        }
    }
}

/// `file` and `line` of a message starting with `file:line:`.
fn leading_position(message: &str) -> Option<(&str, u32)> {
    let (file, rest) = message.split_once(':')?;
    let (line, _) = rest.split_once(':')?;
    position(file, line)
}

/// `file` and `line` of a text that is exactly `file:line`.
fn trailing_position(at: &str) -> Option<(&str, u32)> {
    let (file, line) = at.rsplit_once(':')?;
    position(file, line)
}

fn position<'a>(file: &'a str, line: &str) -> Option<(&'a str, u32)> {
    let line: u32 = line.parse().ok()?;
    (line > 0 && !file.is_empty() && !file.contains(char::is_whitespace)).then_some((file, line))
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
            Self::InvalidName { .. } => "Invalid module name",
            Self::NotFound(_) => "Module not found",
            Self::Unreadable(_) => "Cannot read module",
            Self::Cycle(_) => "Cyclic require",
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
    /// The message, naming `callback` when it failed in one, as in
    /// `Module not found in tick: code/x.luau (code/main.luau:3)`.
    fn message(&self, callback: Option<&str>) -> String {
        let mut text = self.error.headline().to_owned();
        if let Some(callback) = callback {
            text.push_str(" in ");
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
/// `Runtime error in tick: main.luau:3: boom`.
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
        match &self.error {
            ScriptError::Syntax(message) => write!(f, "Syntax error in {callback}: {message}"),
            ScriptError::Runtime(message) => write!(f, "Runtime error in {callback}: {message}"),
            ScriptError::LimitExceeded => write!(f, "Execution limit exceeded in {callback}"),
            ScriptError::ModuleLimitExceeded(path) => {
                write!(f, "Execution limit exceeded in {callback}: {path}")
            }
            ScriptError::Require(failure) => f.write_str(&failure.message(Some(callback))),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Why a runtime failed to start, with the text logged before the failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartFailure {
    pub error: RuntimeError,
    pub output: Vec<LogEntry>,
    /// Entries refused because the log allowance was spent.
    pub dropped: u32,
}
