//! `Button`, `Key`, `btn`, `btnp`, `key` and `keyp`.

use std::cell::Cell;
use std::rc::Rc;

use glaux_input::{Button, InputState, Key};
use mlua::{Lua, MetaMethod, UserData, UserDataFields, UserDataMethods, Value};

use crate::args::{button_arg, key_arg};

/// A `Button` constant as scripts hold it. Each button has exactly one
/// instance, stored in the frozen `Button` table, so constants compare equal
/// by identity and cannot be built from numbers or strings.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ButtonValue(pub(crate) Button);

impl UserData for ButtonValue {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "Button");
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Button.{}", this.0.name()))
        });
    }
}

/// A `Key` constant as scripts hold it, with the same guarantees as
/// [`ButtonValue`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct KeyValue(pub(crate) Key);

impl UserData for KeyValue {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "Key");
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Key.{}", this.0.name()))
        });
    }
}

/// Installs the read-only `Button` and `Key` tables and the functions that
/// read `state`.
pub(crate) fn install(lua: &Lua, state: &Rc<Cell<InputState>>) -> mlua::Result<()> {
    let globals = lua.globals();

    let buttons = lua.create_table_with_capacity(0, Button::ALL.len())?;
    for button in Button::ALL {
        buttons.raw_set(button.name(), ButtonValue(button))?;
    }
    buttons.set_readonly(true);
    globals.raw_set("Button", buttons)?;

    let keys = lua.create_table_with_capacity(0, Key::ALL.len())?;
    for &key in Key::ALL {
        keys.raw_set(key.name(), KeyValue(key))?;
    }
    keys.set_readonly(true);
    globals.raw_set("Key", keys)?;

    let input = Rc::clone(state);
    let btn = lua.create_function(move |lua, button: Value| {
        let button = button_arg(lua, "btn", 1, &button)?;
        Ok(input.get().buttons.held.contains(button))
    })?;
    globals.raw_set("btn", btn)?;

    let input = Rc::clone(state);
    let btnp = lua.create_function(move |lua, button: Value| {
        let button = button_arg(lua, "btnp", 1, &button)?;
        Ok(input.get().buttons.pressed.contains(button))
    })?;
    globals.raw_set("btnp", btnp)?;

    let input = Rc::clone(state);
    let key = lua.create_function(move |lua, key: Value| {
        let key = key_arg(lua, "key", 1, &key)?;
        Ok(input.get().keys.held.contains(key))
    })?;
    globals.raw_set("key", key)?;

    let input = Rc::clone(state);
    let keyp = lua.create_function(move |lua, key: Value| {
        let key = key_arg(lua, "keyp", 1, &key)?;
        Ok(input.get().keys.pressed.contains(key))
    })?;
    globals.raw_set("keyp", keyp)
}
