//! `Button`, `Key`, `btn`, `btnp`, `key` and `keyp`.

use super::*;
use pretty_assertions::assert_eq;

const READ_INPUT: &str = r#"
    fn tick() { log::info(`${btn(Button::A)} ${btnp(Button::A)} ${key(Key::F1)} ${keyp(Key::F1)}`); }
"#;

#[test]
fn input_reads_the_current_tick() {
    let mut runtime = started(READ_INPUT);
    runtime.tick(buttons(&[Button::A], &[Button::A])).unwrap();
    runtime.tick(buttons(&[Button::A], &[])).unwrap();
    runtime.tick(keys(&[Key::F1], &[Key::F1])).unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(
        texts(runtime.take_output()),
        [
            "true true false false",
            "true false false false",
            "false false true true",
            "false false false false"
        ]
    );
}

#[test]
fn input_reads_false_before_the_first_tick() {
    assert_eq!(
        start_output("fn boot() { log::info(`${btn(Button::A)} ${key(Key::Space)}`); }"),
        ["false false"]
    );
}

#[test]
fn constants_compare_by_value_and_show_their_names() {
    assert_eq!(
        start_output(
            r#"
            log::info(Button::A == Button::A);
            log::info(Button::A != Button::B);
            log::info(Key::Num0.to_string());
            log::info(type_of(Button::Up));
            log::info(type_of(Key::PageUp));
            "#
        ),
        ["true", "true", "Key::Num0", "Button", "Key"]
    );
}

#[test]
fn every_button_and_key_has_a_constant() {
    let mut names = Vec::new();
    for button in Button::ALL {
        names.push(format!("Button::{}", button.name()));
    }
    for key in Key::ALL {
        names.push(format!("Key::{}", key.name()));
    }
    let source: String = names
        .iter()
        .map(|name| format!("log::info({name});\n"))
        .collect();
    assert_eq!(start_output(&source), names);
}

#[test]
fn anything_but_a_constant_of_the_right_kind_is_rejected() {
    assert_eq!(
        start_message("btn(\"A\");"),
        "main.rhai:1: bad argument #1 to 'btn' (Button expected, got string; use Button::A etc.)"
    );
    assert_eq!(
        start_message("btnp(Key::A);"),
        "main.rhai:1: bad argument #1 to 'btnp' (Button expected, got Key; use Button::A etc.)"
    );
    assert_eq!(
        start_message("keyp(Button::A);"),
        "main.rhai:1: bad argument #1 to 'keyp' (Key expected, got Button; use Key::A etc.)"
    );
    assert_eq!(
        start_message("key(0);"),
        "main.rhai:1: bad argument #1 to 'key' (Key expected, got i64; use Key::A etc.)"
    );
}

#[test]
fn constants_cannot_be_reassigned() {
    assert!(start("Button::A = 1;").is_err());
}
