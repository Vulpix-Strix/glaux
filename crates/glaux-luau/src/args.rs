//! Validation of the arguments scripts pass to Glaux functions, and the
//! located errors raised when they are wrong.

use glaux_audio::{CHANNELS, Waveform};
use glaux_input::{Button, Key};
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

/// A screen coordinate: floored, and saturated to the `i32` range so that
/// huge values land off-screen instead of failing.
pub(crate) fn coordinate_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<i32> {
    let n = number_arg(lua, function, position, value)?;
    if n.is_nan() {
        return Err(bad_argument(lua, function, position, "number is NaN"));
    }
    // `as` saturates float-to-int conversions, including infinities.
    Ok(n.floor() as i32)
}

/// A size in pixels: a positive integer. `what` names the size in the
/// error. Values past `i32::MAX` saturate, as coordinates do; the result is
/// always at least 1.
pub(crate) fn size_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
    what: &str,
) -> mlua::Result<i32> {
    let n = number_arg(lua, function, position, value)?;
    if n.fract() == 0.0 && n >= 1.0 {
        Ok(n as i32)
    } else {
        Err(bad_argument(
            lua,
            function,
            position,
            &format!("positive {what} expected, got {n}"),
        ))
    }
}

/// A palette index: an integer in `0..=255`, never wrapped or clamped.
pub(crate) fn color_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<u8> {
    let n = number_arg(lua, function, position, value)?;
    if n.fract() == 0.0 && (0.0..=255.0).contains(&n) {
        Ok(n as u8)
    } else {
        Err(bad_argument(
            lua,
            function,
            position,
            &format!("palette index 0-255 expected, got {n}"),
        ))
    }
}

/// An object-tile index: an integer in `0..=1023`.
pub(crate) fn tile_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
) -> mlua::Result<u16> {
    let n = number_arg(lua, function, position, value)?;
    if n.fract() == 0.0 && (0.0..=1023.0).contains(&n) {
        Ok(n as u16)
    } else {
        Err(bad_argument(
            lua,
            function,
            position,
            &format!("object tile index 0-1023 expected, got {n}"),
        ))
    }
}

/// A sprite block's `extent` (width or height) in tiles: `nil` means 1,
/// otherwise a positive integer that keeps the block inside the object-tile
/// grid when it starts at grid `axis` `start` of `limit`.
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
    let n = number_arg(lua, "spr", position, value)?;
    if !(n.fract() == 0.0 && n >= 1.0) {
        return Err(bad_argument(
            lua,
            "spr",
            position,
            &format!("positive tile count expected, got {n}"),
        ));
    }
    let available = limit - start;
    if n > available as f64 {
        return Err(bad_argument(
            lua,
            "spr",
            position,
            &format!(
                "{extent} {n} from tile {axis} {start} exceeds the {limit}-{axis} object-tile grid"
            ),
        ));
    }
    // `available` is at most OBJ_GRID_ROWS, which fits in a byte.
    Ok(n as u8)
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

/// An integer argument in `0..=max`, never wrapped or clamped. `what` names
/// the value in the error.
pub(crate) fn bounded_arg(
    lua: &Lua,
    function: &str,
    position: usize,
    value: &Value,
    what: &str,
    max: u32,
) -> mlua::Result<u32> {
    let n = number_arg(lua, function, position, value)?;
    if n.fract() == 0.0 && (0.0..=f64::from(max)).contains(&n) {
        Ok(n as u32)
    } else {
        Err(bad_argument(
            lua,
            function,
            position,
            &format!("{what} 0-{max} expected, got {n}"),
        ))
    }
}

/// An audio channel: the first argument of every `audio` function.
pub(crate) fn channel_arg(lua: &Lua, function: &str, value: &Value) -> mlua::Result<usize> {
    let max = CHANNELS as u32 - 1;
    bounded_arg(lua, function, 1, value, "channel", max).map(|channel| channel as usize)
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
    let name = name.to_string_lossy();
    Waveform::from_name(&name).ok_or_else(|| {
        bad_argument(
            lua,
            function,
            position,
            &format!("unknown waveform '{name}'; expected square, triangle, saw or noise"),
        )
    })
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
