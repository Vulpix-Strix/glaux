//! `frame` and `t`.

use std::cell::Cell;
use std::rc::Rc;

use glaux_machine::machine::TICK_RATE;
use mlua::Lua;

/// Installs `frame` and `t`.
pub(crate) fn install(lua: &Lua, counter: &Rc<Cell<u64>>) -> mlua::Result<()> {
    let globals = lua.globals();

    // Returned as doubles because Luau numbers are doubles; tick counts
    // stay exact far beyond any realistic run length.
    let ticks = Rc::clone(counter);
    let frame = lua.create_function(move |_, ()| Ok(ticks.get() as f64))?;
    globals.raw_set("frame", frame)?;

    let ticks = Rc::clone(counter);
    let t = lua.create_function(move |_, ()| Ok(ticks.get() as f64 / f64::from(TICK_RATE)))?;
    globals.raw_set("t", t)
}
