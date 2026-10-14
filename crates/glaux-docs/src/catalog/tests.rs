use pretty_assertions::assert_eq;

use super::*;

fn merge(files: &[(&str, &str)]) -> (ApiCatalog, Vec<Diagnostic>) {
    ApiCatalog::from_sources(files.iter().copied())
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<&'static str> {
    diagnostics.iter().map(|d| d.code).collect()
}

const GRAPHICS: &str = r#"#![enable(implicit_some)]
[
    (
        id: "api/graphics/pset",
        kind: Function,
        luau: (name: "pset"),
        rhai: (name: "pset"),
        params: [
            (name: "x", ty: Number),
            (name: "y", ty: Number),
            (name: "color", ty: Int),
        ],
    ),
    (
        id: "api/graphics/text",
        kind: Function,
        luau: (name: "print"),
        rhai: (name: "text"),
        params: [(name: "value", ty: Any), (name: "x", ty: Number, optional: true)],
    ),
]
"#;

const AUDIO: &str = r#"#![enable(implicit_some)]
[
    (id: "api/audio/index", kind: Namespace, luau: (name: "audio"), rhai: (name: "audio")),
    (
        id: "api/audio/tone",
        kind: Function,
        luau: (path: ["audio"], name: "tone"),
        rhai: (path: ["audio"], name: "tone"),
    ),
]
"#;

#[test]
fn files_merge_into_one_catalog_ordered_by_id() {
    let (catalog, diagnostics) = merge(&[("api/graphics.ron", GRAPHICS), ("api/audio.ron", AUDIO)]);
    assert_eq!(diagnostics, []);
    let ids: Vec<_> = catalog.entries().iter().map(|e| e.id().as_str()).collect();
    assert_eq!(
        ids,
        [
            "api/audio/index",
            "api/audio/tone",
            "api/graphics/pset",
            "api/graphics/text"
        ]
    );
}

#[test]
fn merging_does_not_depend_on_file_order() {
    let forward = merge(&[("api/graphics.ron", GRAPHICS), ("api/audio.ron", AUDIO)]);
    let backward = merge(&[("api/audio.ron", AUDIO), ("api/graphics.ron", GRAPHICS)]);
    assert_eq!(forward, backward);
}

#[test]
fn lookup_uses_each_languages_spelling() {
    let (catalog, _) = merge(&[("api/graphics.ron", GRAPHICS), ("api/audio.ron", AUDIO)]);
    let text = DocId::parse("api/graphics/text").unwrap();
    assert_eq!(
        catalog
            .lookup(ScriptLanguage::Luau, &[], "print")
            .map(|e| e.id()),
        Some(&text)
    );
    assert_eq!(
        catalog
            .lookup(ScriptLanguage::Rhai, &[], "text")
            .map(|e| e.id()),
        Some(&text)
    );
    assert_eq!(catalog.lookup(ScriptLanguage::Rhai, &[], "print"), None);
    assert!(
        catalog
            .lookup(ScriptLanguage::Rhai, &["audio"], "tone")
            .is_some()
    );
    assert_eq!(catalog.lookup(ScriptLanguage::Rhai, &[], "tone"), None);
}

#[test]
fn qualified_names_use_the_languages_separator() {
    let binding = Binding {
        path: vec!["audio".into()],
        name: "tone".into(),
        returns: None,
    };
    assert_eq!(ScriptLanguage::Luau.qualified_name(&binding), "audio.tone");
    assert_eq!(ScriptLanguage::Rhai.qualified_name(&binding), "audio::tone");
}

#[test]
fn entries_for_skips_entries_a_language_lacks() {
    let source = r#"#![enable(implicit_some)]
[(id: "api/logging/debug", kind: Function, luau: (path: ["log"], name: "debug"))]"#;
    let (catalog, diagnostics) = merge(&[("api/logging.ron", source)]);
    assert_eq!(diagnostics, []);
    assert_eq!(catalog.entries_for(ScriptLanguage::Luau).count(), 1);
    assert_eq!(catalog.entries_for(ScriptLanguage::Rhai).count(), 0);
}

#[test]
fn required_params_and_return_overrides() {
    let source = r#"#![enable(implicit_some)]
[(
    id: "api/time/frame",
    kind: Function,
    luau: (name: "frame"),
    rhai: (name: "frame", returns: Int),
    returns: Number,
    params: [(name: "a", ty: Int), (name: "b", ty: Int, optional: true)],
)]"#;
    let (catalog, diagnostics) = merge(&[("api/time.ron", source)]);
    assert_eq!(diagnostics, []);
    let entry = &catalog.entries()[0];
    assert_eq!(entry.required_params(), 1);
    assert_eq!(entry.returns(ScriptLanguage::Luau), Some(ValueType::Number));
    assert_eq!(entry.returns(ScriptLanguage::Rhai), Some(ValueType::Int));
}

fn single(category: &str, entry: &str) -> Vec<&'static str> {
    let source = format!("#![enable(implicit_some)]\n[{entry}]");
    codes(&merge(&[(&format!("api/{category}.ron"), &source)]).1)
}

#[test]
fn rejects_duplicate_ids_across_files() {
    let a = "#![enable(implicit_some)]\n[(id: \"api/a/x\", kind: Function, luau: (name: \"x\"))]";
    let b = "#![enable(implicit_some)]\n[(id: \"api/a/x\", kind: Function, rhai: (name: \"y\"))]";
    let (catalog, diagnostics) = merge(&[("api/a.ron", a), ("api/b.ron", b)]);
    // `b.ron` also has the wrong category for its id, which is reported first.
    assert_eq!(codes(&diagnostics), ["catalog-category"]);
    assert_eq!(catalog.entries().len(), 1);

    let (_, diagnostics) = merge(&[("api/a.ron", a), ("api/a.ron", a)]);
    assert_eq!(codes(&diagnostics), ["catalog-duplicate-id"]);
}

#[test]
fn rejects_an_id_whose_category_is_not_the_file() {
    assert_eq!(
        single(
            "audio",
            r#"(id: "api/graphics/x", kind: Function, luau: (name: "x"))"#
        ),
        ["catalog-category"]
    );
}

#[test]
fn rejects_ids_that_are_not_api_category_name() {
    for id in ["guide/audio/x", "api/audio", "api/audio/x/y", "Api/audio/x"] {
        let entry = format!(r#"(id: "{id}", kind: Function, luau: (name: "x"))"#);
        assert_eq!(single("audio", &entry), ["catalog-id"], "{id}");
    }
}

#[test]
fn rejects_a_binding_claimed_twice_even_across_files() {
    let a = "#![enable(implicit_some)]\n[(id: \"api/a/one\", kind: Function, luau: (name: \"same\"), rhai: (name: \"one\"))]";
    let b =
        "#![enable(implicit_some)]\n[(id: \"api/b/two\", kind: Function, luau: (name: \"same\"))]";
    let (catalog, diagnostics) = merge(&[("api/a.ron", a), ("api/b.ron", b)]);
    assert_eq!(codes(&diagnostics), ["catalog-duplicate-binding"]);
    assert_eq!(catalog.entries().len(), 1);
}

#[test]
fn rejects_invalid_definitions() {
    let cases = [
        r#"(id: "api/a/x", kind: Function)"#,
        r#"(id: "api/a/x", kind: Function, luau: (name: "1x"))"#,
        r#"(id: "api/a/x", kind: Function, luau: (path: ["a b"], name: "x"))"#,
        r#"(id: "api/a/x", kind: Function, luau: (name: "x"), params: [(name: "p", ty: Int), (name: "p", ty: Int)])"#,
        r#"(id: "api/a/x", kind: Function, luau: (name: "x"), params: [(name: "p", ty: Int, optional: true), (name: "q", ty: Int)])"#,
        r#"(id: "api/a/x", kind: Namespace, luau: (name: "x"), params: [(name: "p", ty: Int)])"#,
        r#"(id: "api/a/x", kind: ConstantGroup, luau: (name: "X"))"#,
        r#"(id: "api/a/x", kind: Function, luau: (name: "x"), source: "glaux_input::Key")"#,
    ];
    for case in cases {
        assert_eq!(single("a", case), ["catalog-definition"], "{case}");
    }
}

#[test]
fn a_constant_group_with_a_source_is_valid() {
    let entry = r#"(id: "api/a/key", kind: ConstantGroup, luau: (name: "Key"), source: "glaux_input::Key")"#;
    assert_eq!(single("a", entry), Vec::<&str>::new());
}

#[test]
fn a_broken_file_is_reported_without_hiding_the_others() {
    let (catalog, diagnostics) = merge(&[("api/bad.ron", "[(id: "), ("api/audio.ron", AUDIO)]);
    assert_eq!(codes(&diagnostics), ["catalog-parse"]);
    assert!(diagnostics[0].location.starts_with("api/bad.ron:"));
    assert_eq!(catalog.entries().len(), 2);
}

#[test]
fn unknown_fields_are_rejected() {
    let entry = r#"(id: "api/a/x", kind: Function, luau: (name: "x"), colour: 1)"#;
    assert_eq!(single("a", entry), ["catalog-parse"]);
}
