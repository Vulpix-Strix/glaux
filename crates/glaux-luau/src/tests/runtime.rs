//! Starting a script and ticking it.

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn entry_and_boot_run_once_each_at_start() {
    let mut runtime = started(
        r#"
        log.info("entry")
        function boot() log.info("boot") end
        "#,
    );
    assert_eq!(texts(runtime.take_output()), ["entry", "boot"]);
    for _ in 0..3 {
        runtime.tick(NO_INPUT).unwrap();
    }
    assert_eq!(texts(runtime.take_output()), Vec::<String>::new());
}

#[test]
fn missing_callbacks_are_skipped() {
    let mut runtime = started("local x = 1");
    assert_eq!(runtime.tick(NO_INPUT), Ok(()));
    assert_eq!(runtime.tick(NO_INPUT), Ok(()));
}

#[test]
fn syntax_errors_name_the_chunk_and_print_nothing() {
    let failure = start_failure("log.info(\"never\")\nlocal = 1");
    assert!(failure.output.is_empty());
    assert_eq!(failure.error.phase, Phase::Entry);
    match failure.error.error {
        ScriptError::Syntax(message) => {
            assert!(message.starts_with("main.luau:2:"), "{message}");
        }
        other => panic!("expected a syntax error, got {other:?}"),
    }
}

#[test]
fn runtime_errors_keep_only_the_first_line() {
    assert_eq!(
        runtime_message("local x = 1\nerror(\"boom\")"),
        "main.luau:2: boom"
    );
    let message = runtime_message("local f = nil\nf()");
    assert!(message.starts_with("main.luau:2:"), "{message}");
    assert!(!message.contains('\n'), "{message}");
}

#[test]
fn output_before_a_start_failure_is_kept() {
    let failure = start_failure(r#"log.info("before") error("boom")"#);
    assert_eq!(texts(failure.output.clone()), ["before"]);
    assert_eq!(failure.error.phase, Phase::Entry);

    let failure =
        start_failure(r#"log.info("entry") function boot() log.info("boot") error("x") end"#);
    assert_eq!(texts(failure.output.clone()), ["entry", "boot"]);
    assert_eq!(
        failure.error,
        error(Phase::Boot, ScriptError::Runtime("main.luau:1: x".into()))
    );
}

#[test]
fn bytecode_entries_are_rejected() {
    let bytecode = mlua::chunk::Compiler::new()
        .compile(r#"log.info("compiled")"#)
        .unwrap();
    let failure = match Runtime::start("main.luau", &bytecode, Files::default(), SMALL) {
        Ok(_) => panic!("bytecode entry started"),
        Err(failure) => failure,
    };
    assert!(failure.output.is_empty());
    assert!(matches!(failure.error.error, ScriptError::Syntax(_)));
}

#[test]
fn tick_runs_once_per_call_and_draw_follows_it() {
    let mut runtime = started(
        r#"
        function tick() log.info("tick") end
        function draw() log.info("draw") end
        "#,
    );
    for _ in 0..3 {
        runtime.tick(NO_INPUT).unwrap();
    }
    assert_eq!(
        texts(runtime.take_output()),
        ["tick", "draw", "tick", "draw", "tick", "draw"]
    );
}

#[test]
fn globals_persist_across_ticks() {
    let mut runtime = started(
        r#"
        count = 0
        local seen = {}
        function tick() count += 1 table.insert(seen, count) end
        function draw() log.info(count, #seen) end
        "#,
    );
    for _ in 0..3 {
        runtime.tick(NO_INPUT).unwrap();
    }
    assert_eq!(texts(runtime.take_output()), ["1\t1", "2\t2", "3\t3"]);
}

#[test]
fn callbacks_are_looked_up_on_every_call() {
    let mut runtime = started(
        r#"
        function tick()
            log.info("first")
            tick = function() log.info("second") end
        end
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["first", "second"]);
}

#[test]
fn a_failing_tick_skips_draw() {
    let mut runtime = started(
        r#"
        function tick() log.info("tick") error("boom") end
        function draw() log.info("draw") end
        "#,
    );
    assert_eq!(
        runtime.tick(NO_INPUT),
        Err(error(
            Phase::Tick,
            ScriptError::Runtime("main.luau:2: boom".into())
        ))
    );
    assert_eq!(texts(runtime.take_output()), ["tick"]);
}

#[test]
fn draw_errors_name_the_draw_phase() {
    let mut runtime = started(r#"function draw() error("bad") end"#);
    assert_eq!(
        runtime.tick(NO_INPUT),
        Err(error(
            Phase::Draw,
            ScriptError::Runtime("main.luau:1: bad".into())
        ))
    );
}

#[test]
fn non_function_callbacks_are_errors() {
    let failure = start_failure("boot = 3");
    assert_eq!(failure.error.phase, Phase::Boot);
    assert!(
        matches!(&failure.error.error, ScriptError::Runtime(m) if m.contains("'boot' must be a function")),
        "{failure:?}"
    );

    let mut runtime = started(r#"tick = "x""#);
    let err = runtime.tick(NO_INPUT).unwrap_err();
    assert_eq!(err.phase, Phase::Tick);
    assert!(
        matches!(&err.error, ScriptError::Runtime(m) if m.contains("'tick' must be a function")),
        "{err:?}"
    );
}
