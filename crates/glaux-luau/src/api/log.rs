//! `log.trace`, `log.debug`, `log.info` and `log.warn`. Each records a
//! [`LogEntry`] whose text is its arguments converted with `tostring` and
//! joined with tabs.

use std::cell::RefCell;
use std::rc::Rc;

use glaux_script::{LogEntry, LogLevel};
use mlua::{Function, Lua, MultiValue};

use crate::args::to_text;

/// Installs the `log` table.
pub(crate) fn install(
    lua: &Lua,
    tostring: &Function,
    output: &Rc<RefCell<Vec<LogEntry>>>,
) -> mlua::Result<()> {
    let log = lua.create_table()?;
    for level in LogLevel::ALL {
        let name = level.name();
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
