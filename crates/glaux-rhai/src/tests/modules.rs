//! `import` of cartridge modules.

use super::*;
use glaux_modules::ModuleNameError;
use glaux_script::{RequireError, RequireFailure};
use pretty_assertions::assert_eq;

fn require_failure(at: Option<&str>, error: RequireError) -> ScriptError {
    ScriptError::Require(Box::new(RequireFailure {
        at: at.map(str::to_owned),
        error,
    }))
}

/// `files` plus a record of every path requested from them.
fn recorded(files: &[(&'static str, &str)]) -> (Files, Rc<RefCell<Vec<String>>>) {
    let files = Files::new(files);
    let requested = Rc::clone(&files.requested);
    (files, requested)
}

#[test]
fn import_loads_modules_below_code_by_dotted_name() {
    let mut runtime = started_with(
        r#"
        import "util" as util;
        import "game.enemies" as enemies;
        fn tick() { log::info(util::double(enemies::COUNT)); }
        "#,
        Files::new(&[
            ("code/util.rhai", "fn double(x) { x * 2 }"),
            ("code/game/enemies.rhai", "export const COUNT = 21;"),
        ]),
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["42"]);
}

#[test]
fn a_module_runs_once_and_is_shared_by_every_import() {
    let (files, requested) = recorded(&[("code/once.rhai", "print(\"loaded\"); fn f() { 1 }")]);
    let mut runtime = started_with(
        r#"
        import "once" as a;
        fn tick() { import "once" as b; log::info(a::f() + b::f()); }
        "#,
        files,
    );
    runtime.tick(NO_INPUT).unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["loaded", "2", "2"]);
    assert_eq!(*requested.borrow(), ["code/once.rhai"]);
}

#[test]
fn modules_import_modules_and_see_the_glaux_api() {
    let mut runtime = started_with(
        "import \"a\" as a;\nfn draw() { a::paint(); }",
        Files::new(&[
            (
                "code/a.rhai",
                "import \"b\" as b;\nfn paint() { cls(b::COLOR); }",
            ),
            ("code/b.rhai", "export const COLOR = 9;"),
        ]),
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.frame(), [glaux_script::DrawCommand::Clear(9)]);
}

#[test]
fn module_functions_get_state_explicitly_and_cannot_reach_this() {
    let mut runtime = started_with(
        r#"
        import "player" as player;
        fn boot() { this.player = #{ x: 0 }; }
        fn tick() { this.player = player::step(this.player); }
        fn draw() { log::info(this.player.x); }
        "#,
        Files::new(&[(
            "code/player.rhai",
            "fn step(p) { p.x += 1; p }\nfn peek() {\n  this.x\n}",
        )]),
    );
    runtime.tick(NO_INPUT).unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["1", "2"]);

    let mut runtime = started_with(
        "import \"player\" as player;\nfn tick() { player::peek(); }",
        Files::new(&[("code/player.rhai", "fn peek() {\n  this.x\n}")]),
    );
    assert_eq!(
        runtime.tick(NO_INPUT),
        Err(runtime_error(
            Phase::Tick,
            "code/player.rhai:2: 'this' not bound"
        ))
    );
}

#[test]
fn cyclic_imports_fail_with_the_chain() {
    let failure = start_failure_with(
        "import \"a\" as a;",
        Files::new(&[
            ("code/a.rhai", "import \"b\" as b;"),
            ("code/b.rhai", "\nimport \"a\" as a;"),
        ]),
    );
    assert_eq!(
        failure.error.error,
        require_failure(
            Some("code/b.rhai:2"),
            RequireError::Cycle(vec![
                "code/a.rhai".into(),
                "code/b.rhai".into(),
                "code/a.rhai".into()
            ])
        )
    );
    assert_eq!(
        failure.error.to_string(),
        "Cyclic require: code/a.rhai -> code/b.rhai -> code/a.rhai (code/b.rhai:2)"
    );
}

#[test]
fn missing_and_unreadable_modules_name_their_path_and_caller() {
    assert_eq!(
        start_failure("\nimport \"nope\" as n;").error.error,
        require_failure(
            Some("main.rhai:2"),
            RequireError::NotFound("code/nope.rhai".into())
        )
    );
    let mut files = Files::new(&[("code/locked.rhai", "")]);
    files.unreadable.push("code/locked.rhai");
    assert_eq!(
        start_failure_with("import \"locked\" as l;", files)
            .error
            .error,
        require_failure(
            Some("main.rhai:1"),
            RequireError::Unreadable("code/locked.rhai".into())
        )
    );
}

#[test]
fn module_errors_name_the_module_and_the_callback() {
    assert_eq!(
        start_failure_with(
            "import \"bad\" as b;",
            Files::new(&[("code/bad.rhai", "let x = 1;\nlet y = ;")])
        )
        .error
        .error,
        ScriptError::Syntax("code/bad.rhai:2: Unexpected ';'".into())
    );

    let mut runtime = started_with(
        "fn tick() { import \"boom\" as b; }",
        Files::new(&[("code/boom.rhai", "\n\nthrow \"x\";")]),
    );
    let err = runtime.tick(NO_INPUT).unwrap_err();
    assert_eq!(err, runtime_error(Phase::Tick, "code/boom.rhai:3: x"));
    assert_eq!(
        err.to_string(),
        "Runtime error in tick: code/boom.rhai:3: x"
    );
}

#[test]
fn a_failed_module_fails_the_same_way_without_running_again() {
    let (files, requested) = recorded(&[("code/boom.rhai", "print(\"ran\"); throw \"x\";")]);
    let mut runtime = started_with(
        r#"
        fn tick() {
            try { import "boom" as b; } catch (e) { }
            import "boom" as b;
        }
        "#,
        files,
    );
    assert_eq!(
        runtime.tick(NO_INPUT),
        Err(runtime_error(Phase::Tick, "code/boom.rhai:1: x"))
    );
    assert_eq!(texts(runtime.take_output()), ["ran"]);
    assert_eq!(*requested.borrow(), ["code/boom.rhai"]);
}

#[test]
fn invalid_names_are_rejected_before_anything_is_loaded() {
    let cases = [
        ("", ModuleNameError::Empty),
        ("Game", ModuleNameError::InvalidCharacter('G')),
        ("1up", ModuleNameError::LeadingDigit("1up".into())),
        (
            "player.rhai",
            ModuleNameError::HasExtension {
                stem: "player".into(),
                extension: "rhai".into(),
            },
        ),
    ];
    for (name, reason) in cases {
        let (files, requested) = recorded(&[]);
        let failure = start_failure_with(&format!("import \"{name}\" as m;"), files);
        assert_eq!(
            failure.error.error,
            require_failure(
                Some("main.rhai:1"),
                RequireError::InvalidName {
                    name: name.into(),
                    reason
                }
            ),
            "{name}"
        );
        assert!(requested.borrow().is_empty(), "{name}");
    }
}

#[test]
fn top_level_imports_reach_callbacks_and_their_helpers() {
    let mut runtime = started_with(
        "import \"m\" as m;\nfn helper() { m::f() }\nfn tick() { log::info(m::f() + helper()); }",
        Files::new(&[("code/m.rhai", "fn f() { 5 }")]),
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["10"]);
}
