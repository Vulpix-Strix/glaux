//! `log.trace`, `log.debug`, `log.info` and `log.warn`.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Function, Lua, MultiValue};

use crate::args::to_text;

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

/// Installs the `log` table.
pub(crate) fn install(
    lua: &Lua,
    tostring: &Function,
    output: &Rc<RefCell<Vec<LogEntry>>>,
) -> mlua::Result<()> {
    let log = lua.create_table()?;
    for level in LogLevel::ALL {
        let name = level.function_name();
        let qualified = format!("log.{name}");
        let tostring = tostring.clone();
        let output = Rc::clone(output);
        let function = lua.create_function(move |lua, args: MultiValue| {
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
    lua.globals().raw_set("log", log)
}
