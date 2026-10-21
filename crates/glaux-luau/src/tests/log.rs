//! The `log` functions.

use super::*;
use glaux_script::MAX_ORDINARY_ENTRIES;
use pretty_assertions::assert_eq;

#[test]
fn log_functions_record_their_level() {
    let source = r#"
        log.trace("t") log.debug("d") log.info("i") log.warn("w")
    "#;
    let entry = |level, text: &str| LogEntry {
        level,
        text: text.into(),
    };
    assert_eq!(
        started(source).take_output(),
        [
            entry(LogLevel::Trace, "t"),
            entry(LogLevel::Debug, "d"),
            entry(LogLevel::Info, "i"),
            entry(LogLevel::Warn, "w"),
        ]
    );
}

#[test]
fn every_log_level_formats_arguments_alike() {
    let source = r#"
        local owl = setmetatable({}, { __tostring = function() return "OWL" end })
        tostring = function() return "x" end
        for _, f in { log.trace, log.debug, log.info, log.warn } do
            f(owl, 1, nil)
        end
    "#;
    assert_eq!(start_output(source), ["OWL\t1\tnil"; 4]);
}

#[test]
fn error_and_the_debug_library_are_untouched() {
    assert_eq!(runtime_message(r#"error("x")"#), "main.luau:1: x");
    assert_eq!(
        start_output(r#"log.info(type(debug), type(debug.traceback))"#),
        ["table\tfunction"]
    );
}

#[test]
fn print_is_not_a_log_function() {
    let message = runtime_message(r#"print("hi")"#);
    assert_eq!(
        message,
        "main.luau:1: 'print' can only be called during draw()"
    );
}

#[test]
fn log_joins_arguments_with_tabs() {
    assert_eq!(
        start_output(r#"log.info("a", 1, true, nil) log.info()"#),
        ["a\t1\ttrue\tnil", ""]
    );
}

#[test]
fn log_rejects_a_non_string_tostring_result() {
    let source = r#"log.info(setmetatable({}, { __tostring = function() return {} end }))"#;
    let message = runtime_message(source);
    assert!(message.contains("must return a string"), "{message}");
}

#[test]
fn take_output_drains_the_buffer() {
    let mut runtime = started(r#"function tick() log.info("t") end"#);
    runtime.tick(NO_INPUT).unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["t", "t"]);
    assert_eq!(texts(runtime.take_output()), Vec::<String>::new());
}

#[test]
fn entries_past_the_allowance_are_refused_before_their_arguments_are_converted() {
    let source = format!(
        r#"
        local converted = 0
        local counted = setmetatable({{}}, {{ __tostring = function()
            converted = converted + 1
            return "x"
        end }})
        function tick()
            for _ = 1, {} do log.info(counted) end
            log.warn(converted)
        end
    "#,
        MAX_ORDINARY_ENTRIES + 50
    );
    let mut runtime = started(&source);
    runtime.tick(NO_INPUT).unwrap();
    let output = runtime.take_output();
    assert_eq!(output.len() as u32, MAX_ORDINARY_ENTRIES + 1);
    // Only the entries kept were converted, and the warn still fit.
    let warn = output.last().unwrap();
    assert_eq!(warn.level, LogLevel::Warn);
    assert_eq!(warn.text, MAX_ORDINARY_ENTRIES.to_string());
    assert_eq!(runtime.take_dropped(), 50);
}
