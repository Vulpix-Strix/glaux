//! Validation of the arguments scripts pass to Glaux functions, and the
//! located errors raised when they are wrong.

use glaux_audio::Waveform;
use glaux_input::{Button, Key};
use glaux_machine::obj_palette::ObjBank;
use glaux_script::validate;
use mlua::{Function, Lua, Value};

use crate::api::input::{ButtonValue, KeyValue};

/// Converts `value` with the `tostring` captured before the script ran.
pub(crate) fn to_text(
    lua: &Lua,
    tostring: &Function,
    value: Value,
    function: &str,
) -> mlua::Result<String> {
    match tostring.call::<Value>(value)? {
        Value::String(text) => Ok(text.to_string_lossy()),
        _ => Err(located(
            lua,
            format!("'tostring' must return a string to '{function}'"),
        )),
    }
}

/// A numeric argument. Strings are not coerced.
pub(crate) fn number_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<f64> {
    match *value {
        Value::Integer(n) => Ok(n as f64),
        Value::Number(n) => Ok(n),
        ref other => Err(bad_argument(
            lua,
            function,
            position,
            &format!("number expected, got {}", other.type_name()),
        )),
    }
}

/// A numeric argument checked by `check`, whose error detail becomes a
/// bad-argument error.
fn checked<T>(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
    check: impl FnOnce(f64) -> Result<T, String>,
) -> mlua::Result<T> {
    let n = number_arg(lua, function, position, value)?;
    check(n).map_err(|detail| bad_argument(lua, function, position, &detail))
}

/// A screen coordinate, as [`validate::coordinate`] takes it.
pub(crate) fn coordinate_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<i32> {
    checked(lua, function, position, value, validate::coordinate)
}

/// A size in pixels, as [`validate::size`] takes it.
pub(crate) fn size_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
    what: &str,
) -> mlua::Result<i32> {
    checked(lua, function, position, value, |n| validate::size(n, what))
}

/// A palette index, as [`validate::color`] takes it.
pub(crate) fn color_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<u8> {
    checked(lua, function, position, value, validate::color)
}

/// An object-tile index, as [`validate::tile`] takes it.
pub(crate) fn tile_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<u16> {
    checked(lua, function, position, value, validate::tile)
}

/// A sprite block's `extent` in tiles: `nil` means 1, otherwise as
/// [`validate::tile_span`] takes it.
pub(crate) fn tile_span_arg(
    lua: &Lua,
    position: usize,
    value: &Value,
    extent: &str,
    start: usize,
    limit: usize,
    axis: &str,
) -> mlua::Result<u8> {
    if value.is_nil() {
        return Ok(1);
    }
    checked(lua, "spr", position, value, |n| {
        validate::tile_span(n, extent, start, limit, axis)
    })
}

/// An optional boolean flag: `nil` means `false`. Other types, including
/// numbers, are rejected rather than tested for truthiness.
pub(crate) fn flag_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<bool> {
    match *value {
        Value::Nil => Ok(false),
        Value::Boolean(flag) => Ok(flag),
        ref other => Err(bad_argument(
            lua,
            function,
            position,
            &format!("boolean expected, got {}", script_type_name(other)),
        )),
    }
}

/// An integer argument in `0..=max`, as [`validate::bounded`] takes it.
pub(crate) fn bounded_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
    what: &str,
    max: u32,
) -> mlua::Result<u32> {
    checked(lua, function, position, value, |n| {
        validate::bounded(n, what, max)
    })
}

/// An optional object palette bank: `nil` means each tile's default bank.
pub(crate) fn obj_bank_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<Option<ObjBank>> {
    if value.is_nil() {
        return Ok(None);
    }
    checked(lua, function, position, value, validate::obj_bank).map(Some)
}

/// An audio channel: the first argument of every `audio` function.
pub(crate) fn channel_arg(lua: &Lua, function: &str, value: &Value) -> mlua::Result<usize> {
    checked(lua, function, 1, value, validate::channel)
}

/// A waveform, named as scripts name it.
pub(crate) fn waveform_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<Waveform> {
    let Value::String(name) = value else {
        return Err(bad_argument(
            lua,
            function,
            position,
            &format!("string expected, got {}", script_type_name(value)),
        ));
    };
    validate::waveform(&name.to_string_lossy())
        .map_err(|detail| bad_argument(lua, function, position, &detail))
}

/// The type name a script sees for `value`, as `typeof` reports it. mlua
/// tells integers apart, but Luau scripts only have `number`.
pub(crate) fn script_type_name(value: &Value) -> &'static str {
    match value {
        Value::Integer(_) => "number",
        Value::UserData(data) if data.is::<ButtonValue>() => "Button",
        Value::UserData(data) if data.is::<KeyValue>() => "Key",
        other => other.type_name(),
    }
}

/// A `Button` constant. Anything else, including a button name as a
/// string, is rejected.
pub(crate) fn button_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<Button> {
    if let Value::UserData(data) = value
        && let Ok(button) = data.borrow::<ButtonValue>()
    {
        return Ok(button.0);
    }
    Err(bad_argument(
        lua,
        function,
        position,
        &format!(
            "Button expected, got {}; use Button.A etc.",
            script_type_name(value)
        ),
    ))
}

/// A `Key` constant. Anything else, including a key name as a string, is
/// rejected.
pub(crate) fn key_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<Key> {
    if let Value::UserData(data) = value
        && let Ok(key) = data.borrow::<KeyValue>()
    {
        return Ok(key.0);
    }
    Err(bad_argument(
        lua,
        function,
        position,
        &format!(
            "Key expected, got {}; use Key.A etc.",
            script_type_name(value)
        ),
    ))
}

pub(crate) fn bad_argument(
    lua: &Lua,
    function: &str,
    position: usize,
    detail: &str,
) -> mlua::Error {
    located(
        lua,
        format!("bad argument #{position} to '{function}' ({detail})"),
    )
}

/// A runtime error prefixed with the calling script line, as Luau's own
/// errors are, so that errors raised from Rust read like built-in ones.
pub(crate) fn located(lua: &Lua, message: String) -> mlua::Error {
    let place = call_site(lua)
        .map(|place| format!("{place}: "))
        .unwrap_or_default();
    mlua::Error::runtime(format!("{place}{message}"))
}

/// `file:line` of the script calling the running Rust function, if its
/// caller is script code rather than, say, `pcall`.
pub(crate) fn call_site(lua: &Lua) -> Option<String> {
    // Level 0 is the Rust function itself; level 1 is its caller.
    lua.inspect_stack(1, |frame| {
        let line = frame.current_line()?;
        let source = frame.source().short_src?.into_owned();
        Some(format!("{source}:{line}"))
    })
    .flatten()
}
