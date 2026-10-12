//! The operation budget of each call, and Rhai's size and depth limits.

use super::*;
use pretty_assertions::assert_eq;

const RUNAWAYS: [&str; 4] = [
    "loop {}",
    "while true {}",
    "let x = 0; loop { x += 1; }",
    "try { loop {} } catch { } print(\"after\");",
];

#[test]
fn runaway_top_level_and_boot_are_stopped() {
    for body in RUNAWAYS {
        assert_eq!(start_failure(body).error, limit(Phase::Entry), "{body}");
        let boot = format!("fn boot() {{ {body} }}");
        assert_eq!(start_failure(&boot).error, limit(Phase::Boot), "{body}");
    }
}

#[test]
fn runaway_tick_and_draw_are_stopped_and_catch_cannot_resume_them() {
    for body in RUNAWAYS {
        for (name, phase) in [("tick", Phase::Tick), ("draw", Phase::Draw)] {
            let mut runtime = started(&format!("fn {name}() {{ {body} }}"));
            assert_eq!(runtime.tick(NO_INPUT), Err(limit(phase)), "{name}: {body}");
            assert!(
                !texts(runtime.take_output()).contains(&"after".to_owned()),
                "{name}: {body}"
            );
        }
    }
}

#[test]
fn the_budget_applies_to_each_call() {
    // Each call uses most of the budget, which together would exceed it
    // many times over. `x += 1` costs three operations per iteration.
    let mut runtime = started(
        r#"
        fn spin() { let x = 0; for i in 0..2000 { x += 1; } }
        spin();
        fn boot() { spin(); }
        fn tick() { spin(); }
        fn draw() { spin(); }
        "#,
    );
    for _ in 0..20 {
        assert_eq!(runtime.tick(NO_INPUT), Ok(()));
    }
    assert_eq!(
        start_failure("let x = 0; for i in 0..4000 { x += 1; }").error,
        limit(Phase::Entry)
    );
}

#[test]
fn a_runaway_module_is_named_and_charged_to_the_importing_call() {
    let failure = start_failure_with(
        "import \"outer\" as o;",
        Files::new(&[
            ("code/outer.rhai", "import \"spin\" as s;"),
            ("code/spin.rhai", "loop {}"),
        ]),
    );
    assert_eq!(
        failure.error,
        error(
            Phase::Entry,
            ScriptError::ModuleLimitExceeded("code/spin.rhai".into())
        )
    );
    assert_eq!(
        failure.error.to_string(),
        "Execution limit exceeded in code/spin.rhai"
    );

    let mut runtime = started_with(
        "fn tick() { import \"spin\" as s; }",
        Files::new(&[("code/spin.rhai", "loop {}")]),
    );
    assert_eq!(
        runtime.tick(NO_INPUT).unwrap_err().to_string(),
        "Execution limit exceeded in tick: code/spin.rhai"
    );
}

#[test]
fn running_out_after_a_module_loaded_is_charged_to_the_caller() {
    let failure = start_failure_with(
        "import \"ok\" as m;\nloop {}",
        Files::new(&[("code/ok.rhai", "fn f() {}")]),
    );
    assert_eq!(failure.error, limit(Phase::Entry));
}

/// Runs `source` on a thread with a 2 MiB stack, the smallest a host or
/// test thread is given by default, so a native stack overflow aborts the
/// test instead of passing unnoticed.
fn start_on_small_stack(source: &'static str) -> StartFailure {
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || start_failure(source))
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn deep_recursion_fails_cleanly_on_a_small_stack() {
    let plain = start_on_small_stack("fn f(n) { f(n + 1) }\nf(0);");
    assert_eq!(
        plain.error,
        runtime_error(Phase::Entry, "main.rhai:2: Stack overflow")
    );
    // Recursion through closures and a native function nests the deepest
    // native frames found while measuring the call-level limit.
    let through_native = start_on_small_stack("fn f(n) { [1].map(|v| f(n + v)) }\nf(0);");
    assert_eq!(through_native.error.phase, Phase::Entry);
    assert!(
        matches!(&through_native.error.error, ScriptError::Runtime(m) if m.ends_with("Stack overflow")),
        "{:?}",
        through_native.error
    );
}

/// The runtime error message of `source`, started with the default budget
/// so that only the limit under test can stop it.
fn sized_message(source: &str) -> String {
    match Runtime::start(
        "main.rhai",
        source.as_bytes(),
        Files::default(),
        Budget::default(),
    ) {
        Err(StartFailure {
            error:
                RuntimeError {
                    error: ScriptError::Runtime(message),
                    ..
                },
            ..
        }) => message,
        Err(other) => panic!("expected a runtime error, got {other:?}"),
        Ok(_) => panic!("expected the script to fail"),
    }
}

#[test]
fn single_values_are_bounded_in_size() {
    assert_eq!(
        sized_message("let s = \"\";\nfor i in 0..20000 { s += \"x\"; }"),
        "main.rhai:2: Length of string too large"
    );
    assert_eq!(
        sized_message("let a = [];\nfor i in 0..20000 { a.push(i); }"),
        "main.rhai:2: Size of array/BLOB too large"
    );
    // Nested arrays count every element.
    assert_eq!(
        sized_message("let a = [];\nfor i in 0..2000 { a.push([1, 2, 3, 4, 5, 6, 7, 8]); }"),
        "main.rhai:2: Size of array/BLOB too large"
    );
    // Growing a map by index is not checked; using it afterwards is.
    assert_eq!(
        sized_message("let m = #{};\nfor i in 0..5000 { m[`k${i}`] = i; }\nm.len();"),
        "main.rhai:3: Size of object map too large"
    );
}

#[test]
fn state_kept_in_this_is_bounded_across_ticks() {
    let mut runtime = Runtime::start(
        "main.rhai",
        b"fn boot() { this.log = []; }\nfn tick() { this.log.push(frame()); }",
        Files::default(),
        SMALL,
    )
    .unwrap();
    let mut failed = None;
    for tick in 0..20_000 {
        if let Err(err) = runtime.tick(NO_INPUT) {
            failed = Some((tick, err));
            break;
        }
    }
    let (tick, err) = failed.expect("growth of this is never stopped");
    assert_eq!(tick, 16 * 1024);
    // Rhai checks a value changed through `this` after the call that
    // changed it, without a position, so only the script can be named.
    assert_eq!(
        err,
        runtime_error(Phase::Tick, "main.rhai: Size of array/BLOB too large")
    );
}

#[test]
fn maps_grown_by_index_in_this_are_bounded_across_ticks() {
    // Index assignment alone never triggers Rhai's own size check.
    // Nested maps' entries count too: `this` itself holds `m`.
    for (source, last_tick) in [
        ("fn tick() { this[`k${frame()}`] = 1; }", 4 * 1024),
        (
            "fn boot() { this.m = #{}; }\nfn tick() { this.m[`k${frame()}`] = 1; }",
            4 * 1024 - 1,
        ),
    ] {
        let mut runtime =
            Runtime::start("main.rhai", source.as_bytes(), Files::default(), SMALL).unwrap();
        let mut stopped = None;
        for tick in 0..10_000 {
            if let Err(err) = runtime.tick(NO_INPUT) {
                stopped = Some((tick, err));
                break;
            }
        }
        let (tick, err) = stopped.expect("growth of this is never stopped");
        assert_eq!(tick, last_tick, "{source}");
        assert_eq!(
            err,
            runtime_error(Phase::Tick, "main.rhai: Size of object map too large"),
            "{source}"
        );
    }
}
