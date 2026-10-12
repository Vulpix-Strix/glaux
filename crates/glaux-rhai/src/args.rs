//! Conversion of the values scripts pass to Glaux functions, and the
//! located errors raised when they are wrong.
//!
//! Glaux functions take every argument as a [`Dynamic`] and check it here,
//! so a wrong argument is reported as `bad argument #n to 'f' (...)` rather
//! than as Rhai failing to find a function for those argument types.

use glaux_audio::Waveform;
use glaux_input::{Button, Key};
use glaux_machine::obj_palette::ObjBank;
use glaux_script::validate;
use rhai::{Dynamic, EvalAltResult, NativeCallContext};

use crate::api::input::{ButtonValue, KeyValue};

pub(crate) type Result<T> = std::result::Result<T, Box<EvalAltResult>>;

/// A runtime error prefixed with the calling script line, as Rhai's own
/// errors are located.
pub(crate) fn located(ctx: &NativeCallContext, message: String) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(message.into(), ctx.call_position()).into()
}

pub(crate) fn bad_argument(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    detail: &str,
) -> Box<EvalAltResult> {
    located(
        ctx,
        format!("bad argument #{position} to '{function}' ({detail})"),
    )
}

/// The type name a script sees for `value`, as `type_of` reports it.
pub(crate) fn script_type_name(ctx: &NativeCallContext, value: &Dynamic) -> String {
    ctx.engine().map_type_name(value.type_name()).to_owned()
}

/// Text for `value` as Glaux shows it: strings as they are, `()` as `()`,
/// and anything else as Rhai's `to_string` gives it.
pub(crate) fn to_text(value: &Dynamic) -> String {
    if value.is_unit() {
        return "()".to_owned();
    }
    if let Some(button) = value.read_lock::<ButtonValue>() {
        return button.to_string();
    }
    if let Some(key) = value.read_lock::<KeyValue>() {
        return key.to_string();
    }
    value.to_string()
}

/// A numeric argument: an integer or a float. Strings are not coerced.
pub(crate) fn number_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
) -> Result<f64> {
    if let Ok(n) = value.as_int() {
        return Ok(n as f64);
    }
    if let Ok(n) = value.as_float() {
        return Ok(n);
    }
    Err(bad_argument(
        ctx,
        function,
        position,
        &format!("number expected, got {}", script_type_name(ctx, value)),
    ))
}

/// A numeric argument checked by `check`, whose error detail becomes a
/// bad-argument error.
fn checked<T>(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
    check: impl FnOnce(f64) -> std::result::Result<T, String>,
) -> Result<T> {
    let n = number_arg(ctx, function, position, value)?;
    check(n).map_err(|detail| bad_argument(ctx, function, position, &detail))
}

/// A screen coordinate, as [`validate::coordinate`] takes it.
pub(crate) fn coordinate_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
) -> Result<i32> {
    checked(ctx, function, position, value, validate::coordinate)
}

/// A size in pixels, as [`validate::size`] takes it.
pub(crate) fn size_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
    what: &str,
) -> Result<i32> {
    checked(ctx, function, position, value, |n| validate::size(n, what))
}

/// A palette index, as [`validate::color`] takes it.
pub(crate) fn color_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
) -> Result<u8> {
    checked(ctx, function, position, value, validate::color)
}

/// An object-tile index, as [`validate::tile`] takes it.
pub(crate) fn tile_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
) -> Result<u16> {
    checked(ctx, function, position, value, validate::tile)
}

/// A sprite block's `extent` in tiles: `()` means 1, otherwise as
/// [`validate::tile_span`] takes it.
pub(crate) fn tile_span_arg(
    ctx: &NativeCallContext,
    position: usize,
    value: &Dynamic,
    extent: &str,
    start: usize,
    limit: usize,
    axis: &str,
) -> Result<u8> {
    if value.is_unit() {
        return Ok(1);
    }
    checked(ctx, "spr", position, value, |n| {
        validate::tile_span(n, extent, start, limit, axis)
    })
}

/// An optional boolean flag: `()` means `false`. Other types, including
/// numbers, are rejected.
pub(crate) fn flag_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
) -> Result<bool> {
    if value.is_unit() {
        return Ok(false);
    }
    value.as_bool().map_err(|_| {
        bad_argument(
            ctx,
            function,
            position,
            &format!("bool expected, got {}", script_type_name(ctx, value)),
        )
    })
}

/// A boolean argument.
pub(crate) fn bool_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
) -> Result<bool> {
    value.as_bool().map_err(|_| {
        bad_argument(
            ctx,
            function,
            position,
            &format!("bool expected, got {}", script_type_name(ctx, value)),
        )
    })
}

/// An integer argument in `0..=max`, as [`validate::bounded`] takes it.
pub(crate) fn bounded_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
    what: &str,
    max: u32,
) -> Result<u32> {
    checked(ctx, function, position, value, |n| {
        validate::bounded(n, what, max)
    })
}

/// An optional object palette bank: `()` means each tile's default bank.
pub(crate) fn obj_bank_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
) -> Result<Option<ObjBank>> {
    if value.is_unit() {
        return Ok(None);
    }
    checked(ctx, function, position, value, validate::obj_bank).map(Some)
}

/// An audio channel: the first argument of every `audio` function.
pub(crate) fn channel_arg(
    ctx: &NativeCallContext,
    function: &str,
    value: &Dynamic,
) -> Result<usize> {
    checked(ctx, function, 1, value, validate::channel)
}

/// A waveform, named as scripts name it.
pub(crate) fn waveform_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
) -> Result<Waveform> {
    let Some(name) = value.read_lock::<rhai::ImmutableString>() else {
        return Err(bad_argument(
            ctx,
            function,
            position,
            &format!("string expected, got {}", script_type_name(ctx, value)),
        ));
    };
    validate::waveform(&name).map_err(|detail| bad_argument(ctx, function, position, &detail))
}

/// A `Button` constant. Anything else, including a button name as a
/// string, is rejected.
pub(crate) fn button_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
) -> Result<Button> {
    if let Some(button) = value.read_lock::<ButtonValue>() {
        return Ok(button.0);
    }
    Err(bad_argument(
        ctx,
        function,
        position,
        &format!(
            "Button expected, got {}; use Button::A etc.",
            script_type_name(ctx, value)
        ),
    ))
}

/// A `Key` constant. Anything else, including a key name as a string, is
/// rejected.
pub(crate) fn key_arg(
    ctx: &NativeCallContext,
    function: &str,
    position: usize,
    value: &Dynamic,
) -> Result<Key> {
    if let Some(key) = value.read_lock::<KeyValue>() {
        return Ok(key.0);
    }
    Err(bad_argument(
        ctx,
        function,
        position,
        &format!(
            "Key expected, got {}; use Key::A etc.",
            script_type_name(ctx, value)
        ),
    ))
}
