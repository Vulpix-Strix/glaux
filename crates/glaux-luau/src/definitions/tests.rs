use std::path::Path;

use glaux_docs::Library;
use pretty_assertions::assert_eq;

use super::*;

fn generated() -> String {
    definitions(Library::embedded().api()).unwrap()
}

fn catalog(entries: &str) -> ApiCatalog {
    let source = format!("#![enable(implicit_some)]\n[{entries}]");
    let (api, diagnostics) = ApiCatalog::from_sources([("api/a.ron", source.as_str())]);
    assert_eq!(diagnostics, []);
    api
}

#[test]
fn generation_is_deterministic() {
    assert_eq!(generated(), generated());
}

#[test]
fn the_committed_file_is_what_the_catalog_generates() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(crate::DEFINITIONS_PATH);
    let committed = std::fs::read_to_string(&path).unwrap();
    assert!(
        committed == generated(),
        "{} is out of date: run `mise run luau-defs`",
        crate::DEFINITIONS_PATH
    );
}

#[test]
fn the_embedded_definitions_are_what_the_catalog_generates() {
    assert!(
        crate::DEFINITIONS == generated(),
        "run `mise run luau-defs`"
    );
}

#[test]
fn callbacks_and_other_language_names_are_not_declared() {
    let text = generated();
    for absent in ["boot", "tick", "draw", "import", "text"] {
        assert!(
            !text.contains(&format!("function {absent}(")),
            "{absent} is declared"
        );
    }
}

#[test]
fn require_is_left_to_the_analyzer() {
    let text = generated();
    assert!(!text.contains("function require("), "require is declared");

    let api = catalog(
        r#"(id: "api/a/require", kind: Function, luau: (name: "require"),
            params: [(name: "name", ty: Text)], returns: Any)"#,
    );
    assert!(!definitions(&api).unwrap().contains("require"));
}

#[test]
fn signatures_follow_the_luau_side_of_the_catalog() {
    let api = catalog(
        r#"
        (id: "api/a/one", kind: Function, luau: (name: "one"),
         params: [(name: "p", ty: Int), (name: "q", ty: Text, optional: true)], returns: Bool),
        (id: "api/a/two", kind: Function, luau: (name: "two"),
         rhai: (name: "other", returns: Int), returns: Number),
        (id: "api/a/three", kind: Function, luau: (path: ["ns"], name: "three", params: [], varargs: Any),
         rhai: (path: ["ns"], name: "three"), params: [(name: "value", ty: Any)]),
        (id: "api/a/four", kind: Function, luau: (path: ["ns"], name: "four", varargs: Any),
         params: [(name: "first", ty: Key)]),
        (id: "api/a/rhai", kind: Function, rhai: (name: "rhai_only")),
        (id: "api/a/ns", kind: Namespace, luau: (name: "ns")),
        (id: "api/a/empty", kind: Namespace, luau: (name: "empty")),
        "#,
    );
    let text = definitions(&api).unwrap();
    assert!(text.contains("declare function one(p: number, q: string?): boolean\n"));
    assert!(text.contains("declare function two(): number\n"));
    assert!(text.contains("\tthree: (...any) -> (),\n"));
    assert!(text.contains("\tfour: (first: Key, ...any) -> (),\n"));
    assert!(text.contains("declare empty: {}\n"));
    assert!(!text.contains("rhai_only") && !text.contains("other"));
}

#[test]
fn a_constant_group_with_an_unknown_source_is_an_error() {
    let api = catalog(
        r#"(id: "api/a/x", kind: ConstantGroup, luau: (name: "X"), source: "glaux_input::Nope")"#,
    );
    assert_eq!(
        definitions(&api),
        Err(DefinitionsError::UnknownSource {
            id: "api/a/x".into(),
            source_name: "glaux_input::Nope".into(),
        })
    );
}
