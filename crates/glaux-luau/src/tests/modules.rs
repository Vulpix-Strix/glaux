//! `require` and the module cache.

use super::*;
use pretty_assertions::assert_eq;

use crate::modules::ModuleState;

fn require_failure(at: Option<&str>, error: RequireError) -> ScriptError {
    ScriptError::Require(Box::new(RequireFailure {
        at: at.map(str::to_owned),
        error,
    }))
}

fn not_found(at: &str, path: &str) -> ScriptError {
    require_failure(Some(at), RequireError::NotFound(path.into()))
}

const PLAYER: (&str, &str) = (
    "code/player.luau",
    r#"log.info("player loaded") return { name = "owl" }"#,
);

#[test]
fn require_loads_the_module_below_code() {
    let files = Files::new(&[PLAYER]);
    let requested = Rc::clone(&files.requested);
    let mut runtime = started_with(r#"log.info(require("player").name)"#, files);
    assert_eq!(texts(runtime.take_output()), ["player loaded", "owl"]);
    assert_eq!(*requested.borrow(), ["code/player.luau"]);
}

#[test]
fn dotted_names_load_nested_files() {
    let files = Files::new(&[("code/game/enemies.luau", "return 7")]);
    let requested = Rc::clone(&files.requested);
    let mut runtime = started_with(r#"log.info(require("game.enemies"))"#, files);
    assert_eq!(texts(runtime.take_output()), ["7"]);
    assert_eq!(*requested.borrow(), ["code/game/enemies.luau"]);
}

#[test]
fn a_module_runs_once_and_its_result_is_cached() {
    let files = Files::new(&[PLAYER, ("code/empty.luau", "")]);
    let requested = Rc::clone(&files.requested);
    let source = r#"
        local a = require("player")
        local b = require("player")
        log.info(rawequal(a, b), require("empty"), require("empty"))
    "#;
    let mut runtime = started_with(source, files);
    assert_eq!(
        texts(runtime.take_output()),
        ["player loaded", "true\ttrue\ttrue"]
    );
    assert_eq!(*requested.borrow(), ["code/player.luau", "code/empty.luau"]);
}

#[test]
fn modules_require_modules_and_see_glaux_globals() {
    let files = Files::new(&[
        (
            "code/a.luau",
            r#"local b = require("b") log.info("a") return b + 1"#,
        ),
        ("code/b.luau", r#"log.info("b") return 1"#),
    ]);
    let mut runtime = started_with(r#"log.info(require("a"), require("b"))"#, files);
    assert_eq!(texts(runtime.take_output()), ["b", "a", "2\t1"]);
}

#[test]
fn cyclic_requires_fail_with_the_chain() {
    let files = Files::new(&[
        ("code/a.luau", r#"return require("b")"#),
        ("code/b.luau", r#"return require("a")"#),
        ("code/me.luau", r#"return require("me")"#),
    ]);
    let failure = start_failure_with(r#"require("a")"#, files);
    let chain = ["code/a.luau", "code/b.luau", "code/a.luau"].map(String::from);
    assert_eq!(
        failure.error.error,
        require_failure(Some("code/b.luau:1"), RequireError::Cycle(chain.into()))
    );
    assert_eq!(
        failure.error.to_string(),
        "Cyclic require: code/a.luau -> code/b.luau -> code/a.luau (code/b.luau:1)"
    );

    let files = Files::new(&[("code/me.luau", r#"return require("me")"#)]);
    let failure = start_failure_with(r#"require("me")"#, files);
    let chain = ["code/me.luau", "code/me.luau"].map(String::from);
    assert_eq!(
        failure.error.error,
        require_failure(Some("code/me.luau:1"), RequireError::Cycle(chain.into()))
    );
}

#[test]
fn missing_and_unreadable_modules_name_their_path_and_caller() {
    let failure = start_failure_with("local x = 1\nrequire(\"nope\")", Files::default());
    assert_eq!(
        failure.error.error,
        not_found("main.luau:2", "code/nope.luau")
    );
    assert_eq!(
        failure.error.to_string(),
        "Module not found: code/nope.luau (main.luau:2)"
    );

    let mut files = Files::new(&[("code/locked.luau", "return 1")]);
    files.unreadable.push("code/locked.luau");
    let failure = start_failure_with(r#"require("locked")"#, files);
    assert_eq!(
        failure.error.to_string(),
        "Cannot read module: code/locked.luau (main.luau:1)"
    );
}

#[test]
fn module_syntax_and_runtime_errors_name_the_module() {
    let files = Files::new(&[
        ("code/bad.luau", "local x = 1\nlocal = 2"),
        ("code/boom.luau", "local x = 1\nerror(\"boom\")"),
    ]);
    let failure = start_failure_with(r#"require("bad")"#, files);
    match failure.error.error {
        ScriptError::Syntax(message) => {
            assert!(message.starts_with("code/bad.luau:2:"), "{message}");
        }
        other => panic!("expected a syntax error, got {other:?}"),
    }

    let files = Files::new(&[("code/boom.luau", "local x = 1\nerror(\"boom\")")]);
    let failure = start_failure_with(r#"require("boom")"#, files);
    assert_eq!(
        failure.error.error,
        ScriptError::Runtime("code/boom.luau:2: boom".into())
    );
}

#[test]
fn a_failed_module_is_not_run_again() {
    let files = Files::new(&[("code/boom.luau", r#"log.info("ran") error("boom")"#)]);
    let source = r#"
        local ok1 = pcall(require, "boom")
        local ok2 = pcall(require, "boom")
        log.info(ok1, ok2)
        require("boom")
    "#;
    let failure = start_failure_with(source, files);
    assert_eq!(texts(failure.output), ["ran", "false\tfalse"]);
    assert_eq!(
        failure.error.error,
        ScriptError::Runtime("code/boom.luau:1: boom".into())
    );
}

/// One module for each way loading can fail.
fn failing_modules() -> Files {
    let mut files = Files::new(&[
        ("code/locked.luau", "return 1"),
        ("code/bad.luau", "local = 1"),
        ("code/boom.luau", r#"error("boom")"#),
        ("code/dep.luau", r#"return require("missing")"#),
        ("code/after.luau", r#"return require("boom")"#),
        ("code/fine.luau", "return 1"),
    ]);
    files.unreadable.push("code/locked.luau");
    files
}

#[test]
fn every_failure_leaves_its_module_failed_not_loading() {
    type Expected = fn(&ScriptError) -> bool;
    let cases: [(&str, Expected); 6] = [
        // Required through `pcall` first, so there is no script call site.
        ("missing", |e| {
            *e == require_failure(None, RequireError::NotFound("code/missing.luau".into()))
        }),
        (
            "locked",
            |e| matches!(e, ScriptError::Require(f) if matches!(f.error, RequireError::Unreadable(_))),
        ),
        ("bad", |e| matches!(e, ScriptError::Syntax(_))),
        ("boom", |e| {
            *e == ScriptError::Runtime("code/boom.luau:1: boom".into())
        }),
        ("dep", |e| {
            *e == not_found("code/dep.luau:1", "code/missing.luau")
        }),
        ("after", |e| {
            *e == ScriptError::Runtime("code/boom.luau:1: boom".into())
        }),
    ];
    for (name, expected) in cases {
        let source = format!(
            r#"pcall(require, "boom") pcall(require, "{name}") pcall(require, "{name}") require("{name}")"#
        );
        let failure = start_failure_with(&source, failing_modules());
        assert!(
            expected(&failure.error.error),
            "{name}: {:?}",
            failure.error
        );
    }

    let runtime = started_with(
        r#"
        for _, name in { "missing", "locked", "bad", "boom", "dep", "after", "fine" } do
            pcall(require, name)
        end
        "#,
        failing_modules(),
    );
    let modules = runtime.modules();
    assert!(modules.stack.is_empty());
    let mut states: Vec<_> = modules
        .states
        .iter()
        .map(|(path, state)| (path.as_str(), matches!(state, ModuleState::Loaded(_))))
        .collect();
    states.sort();
    assert_eq!(
        states,
        [
            ("code/after.luau", false),
            ("code/bad.luau", false),
            ("code/boom.luau", false),
            ("code/dep.luau", false),
            ("code/fine.luau", true),
            ("code/locked.luau", false),
            ("code/missing.luau", false),
        ]
    );
    assert!(
        !modules
            .states
            .values()
            .any(|state| matches!(state, ModuleState::Loading))
    );
}

#[test]
fn runaway_modules_hit_the_budget_of_the_requiring_call() {
    let files = || {
        Files::new(&[
            ("code/slow.luau", RUNAWAY),
            ("code/outer.luau", r#"return require("slow")"#),
            ("code/fine.luau", "return 1"),
        ])
    };
    let in_module = ScriptError::ModuleLimitExceeded("code/slow.luau".into());
    for source in [
        r#"require("slow")"#,
        r#"pcall(require, "slow") log.info("after")"#,
        r#"require("outer")"#,
    ] {
        let failure = start_failure_with(source, files());
        assert_eq!(
            failure.error,
            error(Phase::Entry, in_module.clone()),
            "{source}"
        );
        assert!(texts(failure.output).is_empty(), "{source}");
    }
    assert_eq!(
        error(Phase::Entry, in_module.clone()).to_string(),
        "Execution limit exceeded in code/slow.luau"
    );

    let mut runtime = started_with(r#"function tick() require("slow") end"#, files());
    let tick = runtime.tick(NO_INPUT).unwrap_err();
    assert_eq!(tick, error(Phase::Tick, in_module));
    assert_eq!(
        tick.to_string(),
        "Execution limit exceeded in tick: code/slow.luau"
    );

    // Exhausted before the module started: the limit is the caller's.
    let failure = start_failure_with(&format!("{CAUGHT_RUNAWAY} require(\"fine\")"), files());
    assert_eq!(failure.error, limit(Phase::Entry));
}

#[test]
fn invalid_module_names_are_rejected_before_loading() {
    let files = Files::default();
    let requested = Rc::clone(&files.requested);
    let names = [
        "../x",
        "/x",
        "a\\b",
        "c:x",
        "a..b",
        "",
        "Player",
        "1up",
        "player.luau",
        "x/../y",
    ];
    let source = names
        .iter()
        .map(|name| format!("assert(not pcall(require, {name:?}))"))
        .collect::<Vec<_>>()
        .join("\n");
    started_with(&source, files);
    assert!(requested.borrow().is_empty());

    let failure = start_failure_with(r#"require("../x")"#, Files::default());
    assert!(matches!(
        failure.error.error,
        ScriptError::Require(ref failure)
            if matches!(&failure.error, RequireError::InvalidName { name, .. } if name == "../x")
    ));
    let message = start_failure_with(r#"require("player.luau")"#, Files::default())
        .error
        .to_string();
    assert!(
        message.starts_with(r#"Invalid module name: "player.luau" - "#)
            && message.contains(r#"use "player""#)
            && message.ends_with("(main.luau:1)"),
        "{message}"
    );
    assert_eq!(
        runtime_message("require(1)"),
        "main.luau:1: bad argument #1 to 'require' (string expected, got number)"
    );
}

#[test]
fn bytecode_modules_are_rejected() {
    let mut files = Files::default();
    let bytecode = mlua::chunk::Compiler::new()
        .compile(r#"log.info("compiled")"#)
        .unwrap();
    files.files.insert("code/compiled.luau", bytecode);
    let failure = start_failure_with(r#"require("compiled")"#, files);
    assert!(failure.output.is_empty());
    assert!(matches!(failure.error.error, ScriptError::Syntax(_)));
}

#[test]
fn module_state_persists_across_ticks() {
    let files = Files::new(&[
        ("code/counter.luau", "return { n = 0 }"),
        (
            "code/lazy.luau",
            r#"log.info("lazy loaded") return { n = 0 }"#,
        ),
    ]);
    let source = r#"
        local counter = require("counter")
        function tick()
            counter.n += 1
            local lazy = require("lazy")
            lazy.n += 10
            log.info(require("counter").n, lazy.n)
        end
    "#;
    let mut runtime = started_with(source, files);
    for _ in 0..3 {
        runtime.tick(NO_INPUT).unwrap();
    }
    assert_eq!(
        texts(runtime.take_output()),
        ["lazy loaded", "1\t10", "2\t20", "3\t30"]
    );
}

#[test]
fn module_errors_in_callbacks_name_the_callback() {
    let mut runtime = started_with(r#"function tick() require("nope") end"#, Files::default());
    assert_eq!(
        runtime.tick(NO_INPUT).unwrap_err().to_string(),
        "Module not found in tick: code/nope.luau (main.luau:1)"
    );
}
