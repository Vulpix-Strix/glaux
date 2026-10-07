//! `Button`, `Key`, `btn`, `btnp`, `key` and `keyp`.

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn btn_and_btnp_report_the_tick_state_in_tick_and_draw() {
    let mut runtime = started(
        r#"
        function tick() log.info("tick", btn(Button.Up), btnp(Button.Up)) end
        function draw() log.info("draw", btn(Button.Up), btnp(Button.Up)) end
        "#,
    );
    use Button::Up;
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
fn every_button_reads_its_own_state() {
    let mut runtime = started(
        r#"
        function tick()
            for _, name in { "Up", "Down", "Left", "Right", "A", "B" } do
                log.info(name, btn(Button[name]), btnp(Button[name]))
            end
        end
        "#,
    );
    runtime
        .tick(buttons(&[Button::Left, Button::A], &[Button::B, Button::A]))
        .unwrap();
    assert_eq!(
        texts(runtime.take_output()),
        [
            "Up\tfalse\tfalse",
            "Down\tfalse\tfalse",
            "Left\ttrue\tfalse",
            "Right\tfalse\tfalse",
            "A\ttrue\ttrue",
            "B\tfalse\ttrue",
        ]
    );
}

#[test]
fn key_and_keyp_report_the_tick_state() {
    let mut runtime = started(r#"function tick() log.info(key(Key.Insert), keyp(Key.Insert)) end"#);
    use Key::Insert;
    for state in [
        keys(&[Insert], &[Insert]),
        keys(&[Insert], &[]),
        keys(&[], &[]),
        keys(&[], &[Insert]),
    ] {
        runtime.tick(state).unwrap();
    }
    assert_eq!(
        texts(runtime.take_output()),
        ["true\ttrue", "true\tfalse", "false\tfalse", "false\ttrue",]
    );
}

#[test]
fn representative_keys_read_their_own_state() {
    let mut runtime = started(
        r#"
        function tick()
            for _, name in { "A", "Z", "Num0", "Num9", "F1", "F12", "Space", "PageUp", "LeftShift", "Up" } do
                log.info(name, key(Key[name]), keyp(Key[name]))
            end
        end
        "#,
    );
    runtime
        .tick(keys(
            &[Key::Z, Key::F12, Key::LeftShift],
            &[Key::F12, Key::Num0],
        ))
        .unwrap();
    assert_eq!(
        texts(runtime.take_output()),
        [
            "A\tfalse\tfalse",
            "Z\ttrue\tfalse",
            "Num0\tfalse\ttrue",
            "Num9\tfalse\tfalse",
            "F1\tfalse\tfalse",
            "F12\ttrue\ttrue",
            "Space\tfalse\tfalse",
            "PageUp\tfalse\tfalse",
            "LeftShift\ttrue\tfalse",
            "Up\tfalse\tfalse",
        ]
    );
}

#[test]
fn buttons_and_keys_with_the_same_name_are_separate() {
    let mut runtime = started(
        r#"function tick() log.info(btn(Button.A), key(Key.A), btn(Button.Up), key(Key.Up)) end"#,
    );
    let mut state = buttons(&[Button::A], &[Button::A]);
    state.keys = keys(&[Key::Up], &[Key::Up]).keys;
    runtime.tick(state).unwrap();
    assert_eq!(texts(runtime.take_output()), ["true\tfalse\tfalse\ttrue"]);
}

#[test]
fn input_is_released_before_the_first_tick() {
    assert_eq!(
        start_output(
            r#"
            log.info(btn(Button.A), btnp(Button.A), key(Key.Z), keyp(Key.Z))
            function boot() log.info(btn(Button.B), btnp(Button.B), key(Key.X), keyp(Key.X)) end
            "#
        ),
        ["false\tfalse\tfalse\tfalse", "false\tfalse\tfalse\tfalse"]
    );
}

#[test]
fn constants_print_and_type_as_glaux_values() {
    assert_eq!(
        start_output(
            r#"
            log.info(tostring(Button.A), tostring(Key.F2), tostring(Key.Num0))
            log.info(typeof(Button.Up), typeof(Key.Up))
            log.info(Button.A == Button.A, Button.A == Button.B, Button.Up == Key.Up)
            "#
        ),
        [
            "Button.A\tKey.F2\tKey.Num0",
            "Button\tKey",
            "true\tfalse\tfalse"
        ]
    );
}

#[test]
fn constant_tables_cover_every_value_and_nothing_else() {
    assert_eq!(
        start_output(
            r#"
            local buttons, keys = 0, 0
            for _ in Button do buttons += 1 end
            for _ in Key do keys += 1 end
            log.info(buttons, keys, Button.Start, Key.Escape)
            "#
        ),
        [format!("6\t{}\tnil\tnil", Key::ALL.len())]
    );
}

#[test]
fn constant_tables_are_read_only() {
    for (script, detail) in [
        ("Button.A = Button.B", "attempt to modify a readonly table"),
        (
            "Key.Start = Key.Enter",
            "attempt to modify a readonly table",
        ),
    ] {
        let message = runtime_message(script);
        assert!(message.contains(detail), "{script}: {message}");
    }
}

#[test]
fn bad_input_arguments_are_runtime_errors() {
    for (call, detail) in [
        (
            r#"btn("a")"#,
            "#1 to 'btn' (Button expected, got string; use Button.A etc.)",
        ),
        (
            "btnp(Key.A)",
            "#1 to 'btnp' (Button expected, got Key; use Button.A etc.)",
        ),
        (
            "btn(1)",
            "#1 to 'btn' (Button expected, got number; use Button.A etc.)",
        ),
        (
            "btn(Button.Start)",
            "#1 to 'btn' (Button expected, got nil; use Button.A etc.)",
        ),
        (
            r#"key("f2")"#,
            "#1 to 'key' (Key expected, got string; use Key.A etc.)",
        ),
        (
            "keyp(Button.A)",
            "#1 to 'keyp' (Key expected, got Button; use Key.A etc.)",
        ),
        (
            "keyp()",
            "#1 to 'keyp' (Key expected, got nil; use Key.A etc.)",
        ),
    ] {
        let mut runtime = started(&format!("function tick() {call} end"));
        assert_eq!(
            runtime.tick(NO_INPUT),
            Err(error(
                Phase::Tick,
                ScriptError::Runtime(format!("main.luau:1: bad argument {detail}"))
            )),
            "{call}"
        );
    }
}
