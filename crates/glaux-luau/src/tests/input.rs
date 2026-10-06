//! `btn` and `btnp`.

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn btn_and_btnp_report_the_tick_state_in_tick_and_draw() {
    let mut runtime = started(
        r#"
        function tick() log.info("tick", btn("up"), btnp("up")) end
        function draw() log.info("draw", btn("up"), btnp("up")) end
        "#,
    );
    use GameButton::Up;
    for state in [
        buttons(&[Up], &[Up]),
        buttons(&[Up], &[]),
        buttons(&[], &[]),
        buttons(&[], &[Up]),
    ] {
        runtime.tick(state).unwrap();
    }
    assert_eq!(
        texts(runtime.take_output()),
        [
            "tick\ttrue\ttrue",
            "draw\ttrue\ttrue",
            "tick\ttrue\tfalse",
            "draw\ttrue\tfalse",
            "tick\tfalse\tfalse",
            "draw\tfalse\tfalse",
            "tick\tfalse\ttrue",
            "draw\tfalse\ttrue",
        ]
    );
}

#[test]
fn every_button_name_reads_its_own_button() {
    let mut runtime = started(
        r#"
        function tick()
            for _, name in { "up", "down", "left", "right", "a", "b" } do
                log.info(name, btn(name), btnp(name))
            end
        end
        "#,
    );
    runtime
        .tick(buttons(&[GameButton::Left], &[GameButton::B]))
        .unwrap();
    assert_eq!(
        texts(runtime.take_output()),
        [
            "up\tfalse\tfalse",
            "down\tfalse\tfalse",
            "left\ttrue\tfalse",
            "right\tfalse\tfalse",
            "a\tfalse\tfalse",
            "b\tfalse\ttrue",
        ]
    );
}

#[test]
fn buttons_are_released_before_the_first_tick() {
    assert_eq!(
        start_output(
            r#"log.info(btn("a"), btnp("a")) function boot() log.info(btn("b"), btnp("b")) end"#
        ),
        ["false\tfalse", "false\tfalse"]
    );
}

#[test]
fn bad_button_names_are_runtime_errors() {
    for (call, detail) in [
        (
            r#"btn("start")"#,
            "#1 to 'btn' (unknown button 'start'; expected up, down, left, right, a or b)",
        ),
        (
            r#"btnp("esc")"#,
            "#1 to 'btnp' (unknown button 'esc'; expected up, down, left, right, a or b)",
        ),
        (
            r#"btn("UP")"#,
            "#1 to 'btn' (unknown button 'UP'; expected up, down, left, right, a or b)",
        ),
        ("btn(1)", "#1 to 'btn' (string expected, got number)"),
        ("btnp()", "#1 to 'btnp' (string expected, got nil)"),
    ] {
        let mut runtime = started(&format!("function tick() {call} end"));
        assert_eq!(
            runtime.tick(NO_BUTTONS),
            Err(error(
                Phase::Tick,
                ScriptError::Runtime(format!("main.luau:1: bad argument {detail}"))
            )),
            "{call}"
        );
    }
}
