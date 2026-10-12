//! `frame()` and `t()`.

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn time_is_zero_until_a_tick_completes_and_then_counts_ticks() {
    let mut runtime = started(
        r#"
        print(`${frame()} ${t()}`);
        fn boot() { print(`${frame()}`); }
        fn tick() { print(`${frame()} ${t()}`); }
        "#,
    );
    for _ in 0..3 {
        runtime.tick(NO_INPUT).unwrap();
    }
    // 60 ticks are one second.
    assert_eq!(
        texts(runtime.take_output()),
        [
            "0 0.0",
            "0",
            "0 0.0",
            "1 0.016666666666666666",
            "2 0.03333333333333333"
        ]
    );
}

#[test]
fn frame_is_an_integer_and_t_a_float() {
    assert_eq!(
        start_output("print(type_of(frame())); print(type_of(t()));"),
        ["i64", "f64"]
    );
}

#[test]
fn a_failed_tick_does_not_advance_time() {
    let mut runtime = started(
        r#"
        fn boot() { this.fail = false; }
        fn tick() { print(frame()); if this.fail { throw "x"; } }
        fn draw() { this.fail = !this.fail; }
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.tick(NO_INPUT).is_err());
    assert_eq!(texts(runtime.take_output()), ["0", "1"]);
}
