//! Exact comparison operators for floats.
//!
//! Rhai's built-in `==`, `!=`, `<`, `<=`, `>` and `>=` compare floats, and
//! floats with integers, within a relative epsilon: `29.999999999999996 <
//! 30` is `false` there. Glaux numbers compare exactly, as Luau's do, so a
//! clamp or a bounds check gives the same answer in either language.

use rhai::{Engine, FLOAT, INT};

/// Replaces Rhai's float comparisons with exact ones.
pub(crate) fn install(engine: &mut Engine) {
    // Rhai's fast operators run built-in implementations for standard types
    // without looking for registered ones, so they must be off for these to
    // take effect.
    engine.set_fast_operators(false);
    register(engine, "==", |a, b| a == b);
    register(engine, "!=", |a, b| a != b);
    register(engine, "<", |a, b| a < b);
    register(engine, "<=", |a, b| a <= b);
    register(engine, ">", |a, b| a > b);
    register(engine, ">=", |a, b| a >= b);
}

/// Registers `op` for float/float, float/integer and integer/float operands,
/// the integer converted to a float first.
fn register(engine: &mut Engine, op: &str, compare: fn(FLOAT, FLOAT) -> bool) {
    engine
        .register_fn(op, move |a: FLOAT, b: FLOAT| compare(a, b))
        .register_fn(op, move |a: FLOAT, b: INT| compare(a, b as FLOAT))
        .register_fn(op, move |a: INT, b: FLOAT| compare(a as FLOAT, b));
}
