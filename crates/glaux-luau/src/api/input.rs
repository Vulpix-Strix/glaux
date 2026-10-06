//! `btn` and `btnp`.

use std::cell::Cell;
use std::rc::Rc;

use glaux_input::ButtonState;
use mlua::{Lua, Value};

use crate::args::button_arg;

/// Installs `btn` and `btnp`.
pub(crate) fn install(lua: &Lua, state: &Rc<Cell<ButtonState>>) -> mlua::Result<()> {
    let globals = lua.globals();

    let buttons = Rc::clone(state);
    let btn = lua.create_function(move |lua, button: Value| {
        let button = button_arg(lua, "btn", 1, &button)?;
        Ok(buttons.get().held.contains(button))
    })?;
    globals.raw_set("btn", btn)?;

    let buttons = Rc::clone(state);
    let btnp = lua.create_function(move |lua, button: Value| {
        let button = button_arg(lua, "btnp", 1, &button)?;
        Ok(buttons.get().pressed.contains(button))
    })?;
    globals.raw_set("btnp", btnp)
}
