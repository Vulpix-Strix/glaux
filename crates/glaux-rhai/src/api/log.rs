//! The `log` module: `log::trace`, `log::info` and `log::warn`. Each takes
//! one value and records it as a [`LogEntry`]; several values are combined
//! with string interpolation, as in ``log::info(`x = ${x}`)``.
//!
//! There is no `log::debug`: `debug` is a Rhai keyword, which no function
//! name may be, even inside a module. Rhai's own `debug(value)` records at
//! debug level instead.

use std::cell::RefCell;
use std::rc::Rc;

use glaux_script::{LogEntry, LogLevel};
use rhai::{Dynamic, Engine, FuncRegistration, Module, Shared};

use crate::args::to_text;

/// Registers the `log` module.
pub(crate) fn install(engine: &mut Engine, output: &Rc<RefCell<Vec<LogEntry>>>) {
    let mut log = Module::new();
    for level in [LogLevel::Trace, LogLevel::Info, LogLevel::Warn] {
        let output = Rc::clone(output);
        FuncRegistration::new(level.name()).set_into_module(&mut log, move |value: Dynamic| {
            output.borrow_mut().push(LogEntry {
                level,
                text: to_text(&value),
            });
        });
    }
    engine.register_static_module("log", Shared::new(log));
}
