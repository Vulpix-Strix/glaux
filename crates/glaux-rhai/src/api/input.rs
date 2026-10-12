//! `Button`, `Key`, `btn`, `btnp`, `key` and `keyp`.

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

use glaux_input::{Button, InputState, Key};
use rhai::{Dynamic, Engine, Module, NativeCallContext, Shared};

use crate::args::{Result, button_arg, key_arg};

/// A `Button` constant as scripts hold it. Scripts get them only from the
/// `Button` module, as in `Button::A`, so they cannot be built from numbers
/// or strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ButtonValue(pub(crate) Button);

impl fmt::Display for ButtonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Button::{}", self.0.name())
    }
}

/// A `Key` constant as scripts hold it, with the same guarantees as
/// [`ButtonValue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyValue(pub(crate) Key);

impl fmt::Display for KeyValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Key::{}", self.0.name())
    }
}

/// Registers the `Button` and `Key` types and modules and the functions
/// that read `state`.
pub(crate) fn install(engine: &mut Engine, state: &Rc<Cell<InputState>>) {
    engine
        .register_type_with_name::<ButtonValue>("Button")
        .register_fn("to_string", |b: &mut ButtonValue| b.to_string())
        .register_fn("to_debug", |b: &mut ButtonValue| b.to_string())
        .register_fn("==", |a: ButtonValue, b: ButtonValue| a == b)
        .register_fn("!=", |a: ButtonValue, b: ButtonValue| a != b);
    engine
        .register_type_with_name::<KeyValue>("Key")
        .register_fn("to_string", |k: &mut KeyValue| k.to_string())
        .register_fn("to_debug", |k: &mut KeyValue| k.to_string())
        .register_fn("==", |a: KeyValue, b: KeyValue| a == b)
        .register_fn("!=", |a: KeyValue, b: KeyValue| a != b);

    let mut buttons = Module::new();
    for button in Button::ALL {
        buttons.set_var(button.name(), ButtonValue(button));
    }
    engine.register_static_module("Button", Shared::new(buttons));

    let mut keys = Module::new();
    for &key in Key::ALL {
        keys.set_var(key.name(), KeyValue(key));
    }
    engine.register_static_module("Key", Shared::new(keys));

    let input = Rc::clone(state);
    engine.register_fn(
        "btn",
        move |ctx: NativeCallContext, button: Dynamic| -> Result<bool> {
            let button = button_arg(&ctx, "btn", 1, &button)?;
            Ok(input.get().buttons.held.contains(button))
        },
    );
    let input = Rc::clone(state);
    engine.register_fn(
        "btnp",
        move |ctx: NativeCallContext, button: Dynamic| -> Result<bool> {
            let button = button_arg(&ctx, "btnp", 1, &button)?;
            Ok(input.get().buttons.pressed.contains(button))
        },
    );
    let input = Rc::clone(state);
    engine.register_fn(
        "key",
        move |ctx: NativeCallContext, key: Dynamic| -> Result<bool> {
            let key = key_arg(&ctx, "key", 1, &key)?;
            Ok(input.get().keys.held.contains(key))
        },
    );
    let input = Rc::clone(state);
    engine.register_fn(
        "keyp",
        move |ctx: NativeCallContext, key: Dynamic| -> Result<bool> {
            let key = key_arg(&ctx, "keyp", 1, &key)?;
            Ok(input.get().keys.pressed.contains(key))
        },
    );
}
