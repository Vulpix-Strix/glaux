//! Lifecycle: the top level, callbacks, `this` and error reporting.

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn the_top_level_and_boot_run_once_then_each_tick_calls_tick_then_draw() {
    let mut runtime = started(
        r#"
        print("top");
        fn boot() { print("boot"); }
        fn tick() { print("tick"); }
        fn draw() { print("draw"); }
        "#,
    );
    assert_eq!(texts(runtime.take_output()), ["top", "boot"]);
    runtime.tick(NO_INPUT).unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(
        texts(runtime.take_output()),
        ["tick", "draw", "tick", "draw"]
    );
}

#[test]
fn missing_callbacks_are_skipped() {
    let mut runtime = started("fn draw() { cls(1); }");
    assert_eq!(runtime.tick(NO_INPUT), Ok(()));
    assert_eq!(runtime.frame(), [glaux_script::DrawCommand::Clear(1)]);

    let mut runtime = started("");
    assert_eq!(runtime.tick(NO_INPUT), Ok(()));
    assert!(runtime.frame().is_empty());
}

#[test]
fn this_is_one_map_kept_for_the_whole_run() {
    let mut runtime = started(
        r#"
        fn boot() { this.count = 10; }
        fn tick() { this.count += 1; }
        fn draw() { log::info(this.count); }
        "#,
    );
    for _ in 0..3 {
        runtime.tick(NO_INPUT).unwrap();
    }
    assert_eq!(texts(runtime.take_output()), ["11", "12", "13"]);
}

#[test]
fn this_starts_as_an_empty_map() {
    assert_eq!(start_output("fn boot() { log::info(this); }"), ["#{}"]);
}

#[test]
fn callbacks_may_call_script_functions_as_methods_of_this() {
    let mut runtime = started(
        r#"
        fn step() { this.x += 2; }
        fn boot() { this.x = 0; }
        fn tick() { this.step(); }
        fn draw() { log::info(this.x); }
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["2", "4"]);
}

#[test]
fn top_level_variables_are_not_visible_in_callbacks() {
    let failure = start_failure("let speed = 2;\nfn boot() {\n log::info(speed); }");
    assert_eq!(
        failure.error,
        runtime_error(Phase::Boot, "main.rhai:3: Variable not found: speed")
    );
}

#[test]
fn top_level_constants_are_visible_through_global_in_callbacks_and_helpers() {
    let mut runtime = started(
        r#"
        const SPEED = 3;
        fn doubled() { global::SPEED * 2 }
        fn tick() { log::info(global::SPEED + doubled()); }
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["9"]);
}

/// Rhai 1.26 marks a value read through `global::` or a module namespace
/// as constant, and a field given it directly keeps the mark. Passing it
/// through a parameter, as Signal Lost's `util::writable` does, clears it.
#[test]
fn a_field_given_a_constant_directly_stays_constant() {
    let mut runtime = started(
        r#"
        const LOCK = 60;
        fn writable(value) { value }
        fn tick() {
            this.copied = writable(global::LOCK);
            this.copied -= 1;
            log::info(this.copied);
            this.direct = global::LOCK;
            this.direct -= 1;
        }
        "#,
    );
    assert_eq!(
        runtime.tick(NO_INPUT),
        Err(runtime_error(
            Phase::Tick,
            "main.rhai:9: Cannot modify constant "
        ))
    );
    assert_eq!(texts(runtime.take_output()), ["59"]);
}

#[test]
fn private_callbacks_are_rejected_when_called() {
    let mut runtime = started("private fn tick() {}");
    assert_eq!(
        runtime.tick(NO_INPUT),
        Err(runtime_error(Phase::Tick, "'tick' must not be private"))
    );
}

#[test]
fn reserved_trampoline_names_cannot_hijack_callbacks() {
    let mut runtime =
        started("fn __glaux_tick() { print(\"hijacked\"); }\nfn tick() { print(\"tick\"); }");
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["tick"]);
}

#[test]
fn callbacks_with_parameters_are_rejected_when_called() {
    let failure = start_failure("fn boot(x) {}");
    assert_eq!(
        failure.error,
        runtime_error(Phase::Boot, "'boot' must take no parameters")
    );
    let mut runtime = started("fn draw(a, b) {}");
    assert_eq!(
        runtime.tick(NO_INPUT),
        Err(runtime_error(Phase::Draw, "'draw' must take no parameters"))
    );
}

#[test]
fn an_overload_without_parameters_is_the_callback() {
    let mut runtime = started("fn tick(x) { print(\"one\"); }\nfn tick() { print(\"none\"); }");
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["none"]);
}

#[test]
fn a_failed_tick_skips_draw() {
    let mut runtime = started("fn tick() { throw \"no\"; }\nfn draw() { print(\"drawn\"); }");
    assert_eq!(
        runtime.tick(NO_INPUT),
        Err(runtime_error(Phase::Tick, "main.rhai:1: no"))
    );
    assert!(runtime.take_output().is_empty());
}

#[test]
fn a_failed_draw_keeps_the_previous_frame() {
    let mut runtime = started(
        r#"
        fn boot() { this.fail = false; }
        fn draw() {
            cls(3);
            if this.fail { throw "late"; }
            this.fail = true;
        }
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(
        runtime.tick(NO_INPUT),
        Err(runtime_error(Phase::Draw, "main.rhai:5: late"))
    );
    assert_eq!(runtime.frame(), [glaux_script::DrawCommand::Clear(3)]);
}

#[test]
fn errors_are_located_and_name_their_phase() {
    let failure = start_failure("let x = 1;\nlet y = x +;");
    assert_eq!(
        failure.error,
        error(
            Phase::Entry,
            ScriptError::Syntax("main.rhai:2: Unexpected ';'".into())
        )
    );
    assert_eq!(
        failure.error.to_string(),
        "Syntax error: main.rhai:2: Unexpected ';'"
    );

    let mut runtime = started("fn tick() {\n  missing(1);\n}");
    let err = runtime.tick(NO_INPUT).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Runtime error in tick: main.rhai:2: Function not found: missing (i64)"
    );
}

#[test]
fn errors_inside_nested_calls_name_the_innermost_line() {
    assert_eq!(
        start_message("fn inner() {\n  throw \"deep\";\n}\nfn outer() { inner() }\nouter();"),
        "main.rhai:2: deep"
    );
}

#[test]
fn arithmetic_errors_name_the_script_line_and_operation() {
    assert_eq!(
        start_message("let zero = 0;\nlet x = 1 / zero;"),
        "main.rhai:2: Division by zero: 1 / 0"
    );
    assert_eq!(
        start_message("let big = 9223372036854775807;\n\nlet x = big + 1;"),
        "main.rhai:3: Addition overflow: 9223372036854775807 + 1"
    );
}

#[test]
fn a_thrown_non_string_is_shown_as_rhai_prints_it() {
    assert_eq!(start_message("throw 42;"), "main.rhai:1: 42");
    assert_eq!(start_message("throw;"), "main.rhai:1: ()");
}

#[test]
fn caught_errors_do_not_stop_the_script() {
    assert_eq!(
        start_output(r#"try { pset(1, 1, 1); } catch (e) { print("caught"); } print("after");"#),
        ["caught", "after"]
    );
}

#[test]
fn invalid_utf8_is_a_syntax_error() {
    let failure = Runtime::start("main.rhai", b"print(\"\xff\");", Files::default(), SMALL)
        .err()
        .unwrap();
    assert_eq!(
        failure.error,
        error(
            Phase::Entry,
            ScriptError::Syntax("main.rhai: script is not valid UTF-8".into())
        )
    );
}

#[test]
fn output_logged_before_a_failed_start_is_kept() {
    let failure = start_failure("print(\"one\");\nfn boot() { print(\"two\"); throw \"x\"; }");
    assert_eq!(texts(failure.output), ["one", "two"]);
    assert_eq!(failure.error.phase, Phase::Boot);
}
