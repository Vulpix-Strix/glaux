//! `log::*`, `print` and `debug`.

use super::*;
use glaux_script::LogLevel;
use pretty_assertions::assert_eq;

#[test]
fn each_log_function_records_its_level() {
    let mut runtime = started(
        r#"
        log::trace("t"); log::info("i"); log::warn("w");
        print("p"); debug("g");
        "#,
    );
    let entries: Vec<_> = runtime
        .take_output()
        .into_iter()
        .map(|entry| (entry.level, entry.text))
        .collect();
    assert_eq!(
        entries,
        [
            (LogLevel::Trace, "t".into()),
            (LogLevel::Info, "i".into()),
            (LogLevel::Warn, "w".into()),
            (LogLevel::Info, "p".into()),
            // `debug` uses Rhai's debug formatting.
            (LogLevel::Debug, "\"g\"".into()),
        ]
    );
}

#[test]
fn log_converts_any_value() {
    assert_eq!(
        start_output(
            "log::info(1); log::info(2.5); log::info(()); log::info([1, \"a\"]); log::info(#{x: 1}); log::info(Key::F2);"
        ),
        ["1", "2.5", "()", "[1, \"a\"]", "#{\"x\": 1}", "Key::F2"]
    );
}

#[test]
fn output_is_taken_once() {
    let mut runtime = started("fn tick() { print(\"x\"); }");
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["x"]);
    assert!(runtime.take_output().is_empty());
}

#[test]
fn log_debug_is_not_available_because_debug_is_a_keyword() {
    assert_eq!(
        start_failure("log::debug(1);").error,
        error(
            Phase::Entry,
            ScriptError::Syntax("main.rhai:1: 'debug' is a reserved keyword".into())
        )
    );
}
