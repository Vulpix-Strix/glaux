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
    assert_eq!(
        shown(Phase::Entry, ScriptError::MemoryLimitExceeded),
        "Memory limit exceeded"
    );
    assert_eq!(
        shown(Phase::Tick, ScriptError::MemoryLimitExceeded),
        "Memory limit exceeded in tick"
    );
}

/// Starts `source` under the default budget, which is large enough to reach
/// the memory limit before the interrupt budget runs out.
fn roomy_with(source: &str, files: Files) -> Result<Runtime, StartFailure> {
    Runtime::start(
        "main.luau",
        source.as_bytes(),
        files,
        Budget::default(),
        font(),
    )
}

fn roomy(source: &str) -> Runtime {
    match roomy_with(source, Files::default()) {
        Ok(runtime) => runtime,
        Err(failure) => panic!("expected the script to start, got {failure:?}"),
    }
}

fn roomy_failure_with(source: &str, files: Files) -> StartFailure {
    match roomy_with(source, files) {
        Ok(_) => panic!("expected the script to fail to start"),
        Err(failure) => failure,
    }
}

fn roomy_failure(source: &str) -> StartFailure {
    roomy_failure_with(source, Files::default())
}

fn memory(phase: Phase) -> RuntimeError {
    error(phase, ScriptError::MemoryLimitExceeded)
}

/// Fills a table until the heap cannot take more, then logs `after`.
const GROWTH: &str = "local t = {} for i = 1, 1e8 do t[i] = { i } end log.info(\"after\")";
const CAUGHT_GROWTH: &str =
    "local t = {} pcall(function() for i = 1, 1e8 do t[i] = { i } end end) log.info(\"after\")";
/// A single builtin call asking for twice the heap.
const HUGE_STRING: &str = "local s = string.rep(\"x\", 8 * 1024 * 1024) log.info(\"after\")";

#[test]
fn memory_exhaustion_is_reported_in_every_phase() {
    for body in [HUGE_STRING, GROWTH, CAUGHT_GROWTH] {
        assert_eq!(roomy_failure(body).error, memory(Phase::Entry), "{body}");
        let boot = format!("function boot() {body} end");
        assert_eq!(roomy_failure(&boot).error, memory(Phase::Boot), "{body}");
        for (name, phase) in [("tick", Phase::Tick), ("draw", Phase::Draw)] {
            let mut runtime = roomy(&format!("function {name}() {body} end"));
            assert_eq!(runtime.tick(NO_INPUT), Err(memory(phase)), "{name}: {body}");
            assert!(
                !texts(runtime.take_output()).contains(&"after".to_owned()),
                "{name}: {body}"
            );
        }
    }
}

#[test]
fn pcall_cannot_resume_a_script_that_ran_out_of_memory() {
    let mut runtime = roomy(
        r#"
        function tick()
            local t = {}
            while true do
                pcall(function() for i = 1, 1e6 do t[#t + 1] = { i } end end)
                log.info("retry")
            end
        end
        "#,
    );
    assert_eq!(runtime.tick(NO_INPUT), Err(memory(Phase::Tick)));
    assert!(texts(runtime.take_output()).len() <= 1);
}

#[test]
fn a_caught_allocation_failure_leaves_the_heap_within_the_limit() {
    // One builtin asking for more than the heap holds fails before it
    // allocates anything, so a script may handle it and carry on.
    let mut runtime = roomy(
        r#"
        function tick()
            for i = 1, 10 do
                local ok = pcall(string.rep, "x", 8 * 1024 * 1024)
                log.info(ok)
            end
        end
        "#,
    );
    assert_eq!(runtime.tick(NO_INPUT), Ok(()));
    assert_eq!(texts(runtime.take_output()), vec!["false"; 10]);
    assert!(runtime.lua().used_memory() <= MEMORY_LIMIT);
}

#[test]
fn garbage_does_not_count_against_the_limit() {
    // About 1.7 MiB stay reachable while tables the script no longer holds
    // are created far faster than the incremental collector frees them.
    let mut runtime = roomy(
        r#"
        function tick()
            local live = {}
            for i = 1, 14000 do live[i] = { i, i + 1, i + 2, i + 3 } end
            for i = 1, 400000 do local garbage = { i, i } end
        end
        "#,
    );
    for _ in 0..5 {
        assert_eq!(runtime.tick(NO_INPUT), Ok(()));
    }
}

#[test]
fn a_script_holding_more_than_the_limit_fails_however_much_it_collects() {
    let mut runtime = roomy(
        r#"
        local live = {}
        function tick()
            for i = 1, 1e8 do
                live[i] = { i, i + 1, i + 2, i + 3 }
                if i % 1000 == 0 then collectgarbage() end
            end
        end
        "#,
    );
    assert_eq!(runtime.tick(NO_INPUT), Err(memory(Phase::Tick)));
}

#[test]
fn memory_is_released_with_the_runtime() {
    let script = "function tick() local t = {} for i = 1, 1e8 do t[i] = { i } end end";
    for _ in 0..3 {
        let mut runtime = roomy(script);
        assert_eq!(runtime.tick(NO_INPUT), Err(memory(Phase::Tick)));
    }
    let mut runtime = roomy("function tick() log.info(\"ok\") end");
    assert_eq!(runtime.tick(NO_INPUT), Ok(()));
    assert!(runtime.lua().used_memory() < MEMORY_LIMIT / 4);
}

#[test]
fn a_module_that_runs_out_of_memory_is_reported_as_memory() {
    let files = Files::new(&[("code/big.luau", "return string.rep(\"x\", 8 * 1024 * 1024)")]);
    let failure = roomy_failure_with("require(\"big\")", files);
    assert_eq!(failure.error, memory(Phase::Entry));
}

#[test]
fn a_coroutine_cannot_resume_a_script_that_ran_out_of_memory() {
    let mut runtime = roomy(
        r#"
        function tick()
            local co = coroutine.create(function()
                local t = {}
                while true do t[#t + 1] = { 1 } end
            end)
            coroutine.resume(co)
            log.info("after")
        end
        "#,
    );
    assert_eq!(runtime.tick(NO_INPUT), Err(memory(Phase::Tick)));
    assert!(texts(runtime.take_output()).is_empty());
}
