//! The interrupt budget of each call.

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn runaway_entry_and_boot_are_interrupted() {
    for body in [RUNAWAY, CAUGHT_RUNAWAY] {
        assert_eq!(start_failure(body).error, limit(Phase::Entry), "{body}");
        let boot = format!("function boot() {body} end");
        assert_eq!(start_failure(&boot).error, limit(Phase::Boot), "{body}");
    }
    assert_eq!(
        start_failure("local function f() for i = 1, math.huge do end end f()").error,
        limit(Phase::Entry)
    );
}

#[test]
fn runaway_tick_and_draw_are_interrupted() {
    for body in [RUNAWAY, CAUGHT_RUNAWAY] {
        for (name, phase) in [("tick", Phase::Tick), ("draw", Phase::Draw)] {
            let mut runtime = started(&format!("function {name}() {body} end"));
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
    // many times over.
    let mut runtime = started(
        r#"
        local function spin() for i = 1, 7000 do end end
        spin()
        function boot() spin() end
        function tick() spin() end
        function draw() spin() end
        "#,
    );
    for _ in 0..20 {
        assert_eq!(runtime.tick(NO_INPUT), Ok(()));
    }
    // Two calls' worth of spinning in one call does not fit.
    assert_eq!(
        start_failure("for i = 1, 14000 do end").error,
        limit(Phase::Entry)
    );
}

#[test]
fn errors_name_their_phase() {
    let shown = |phase, error| self::error(phase, error).to_string();
    assert_eq!(
        shown(Phase::Entry, ScriptError::Runtime("main.luau:1: x".into())),
        "Runtime error: main.luau:1: x"
    );
    assert_eq!(
        shown(Phase::Tick, ScriptError::Runtime("main.luau:1: x".into())),
        "Runtime error in tick: main.luau:1: x"
    );
    assert_eq!(
        shown(Phase::Draw, ScriptError::LimitExceeded),
        "Execution limit exceeded in draw"
    );
    assert_eq!(
        shown(Phase::Boot, ScriptError::LimitExceeded),
        "Execution limit exceeded in boot"
    );
}
