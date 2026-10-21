use pretty_assertions::assert_eq;

use super::*;
use SymbolKind::*;

fn parse(grammar: Grammar, text: &str) -> Highlighter {
    let mut highlighter = Highlighter::new(grammar).expect("bundled query compiles");
    highlighter.update(text);
    highlighter
}

fn outline(grammar: Grammar, text: &str) -> Vec<(String, SymbolKind, usize)> {
    parse(grammar, text)
        .symbols()
        .into_iter()
        .map(|s| (s.name, s.kind, s.depth))
        .collect()
}

fn imports(grammar: Grammar, text: &str) -> Vec<(String, String)> {
    parse(grammar, text)
        .imports()
        .into_iter()
        .map(|i| (i.alias, i.module))
        .collect()
}

/// The exported names, or `None` when the module is not understood.
fn exported(grammar: Grammar, text: &str) -> Option<Vec<(String, SymbolKind)>> {
    match parse(grammar, text).exports() {
        Exports::Unknown => None,
        Exports::Known(list) => Some(list.into_iter().map(|e| (e.name, e.kind)).collect()),
    }
}

fn pair(name: &str, kind: SymbolKind) -> (String, SymbolKind) {
    (name.to_owned(), kind)
}

#[test]
fn luau_outline_lists_top_level_declarations_and_nested_functions_only() {
    let text = "local speed = 1\n\
                local run = function() end\n\
                function tick(dt)\n  local hidden = 2\n  local function helper() end\nend\n\
                function Player.new() end\n\
                for i = 1, 3 do local loop = i end\n";
    assert_eq!(
        outline(Grammar::Luau, text),
        vec![
            ("speed".to_owned(), Variable, 0),
            ("run".to_owned(), Function, 0),
            ("tick".to_owned(), Function, 0),
            ("helper".to_owned(), Function, 1),
            ("Player.new".to_owned(), Function, 0),
        ]
    );
}

#[test]
fn rhai_outline_distinguishes_constants_and_ignores_locals_in_functions() {
    let text =
        "const LIMIT = 3;\nlet score = 0;\nfn tick() { let inner = 1; }\nimport \"m\" as m;\n";
    assert_eq!(
        outline(Grammar::Rhai, text),
        vec![
            ("LIMIT".to_owned(), Constant, 0),
            ("score".to_owned(), Variable, 0),
            ("tick".to_owned(), Function, 0),
        ]
    );
}

#[test]
fn symbol_range_points_at_the_declared_name() {
    let text = "local x = 1\nfunction go() end\n";
    let symbols = parse(Grammar::Luau, text).symbols();
    let words: Vec<&str> = symbols.iter().map(|s| &text[s.declared.clone()]).collect();
    assert_eq!(words, vec!["x", "go"]);
}

#[test]
fn luau_imports_need_a_plain_string_and_an_alias_that_is_never_reassigned() {
    let text = "local a = require(\"game.a\")\n\
                local b = require(name)\n\
                local c = require(\"game.\" .. x)\n\
                local d = require(\"game.d\")\n\
                d = nil\n\
                local e, f = require(\"game.e\")\n";
    assert_eq!(
        imports(Grammar::Luau, text),
        vec![("a".to_owned(), "game.a".to_owned())]
    );
}

#[test]
fn rhai_imports_read_the_module_and_alias() {
    let text = "import \"util.math\" as math;\nlet x = 1;\nimport \"b\" as b;\n";
    assert_eq!(
        imports(Grammar::Rhai, text),
        vec![
            ("math".to_owned(), "util.math".to_owned()),
            ("b".to_owned(), "b".to_owned()),
        ]
    );
}

#[test]
fn luau_exports_a_returned_table_literal() {
    let text = "local function f() end\nreturn { f = f, limit = 3, g = function() end, 7 }\n";
    assert_eq!(
        exported(Grammar::Luau, text),
        Some(vec![
            pair("f", Variable),
            pair("limit", Variable),
            pair("g", Function),
        ])
    );
}

#[test]
fn luau_exports_the_members_added_to_a_returned_local_table() {
    let text = "local M = { limit = 3 }\n\
                function M.f() end\n\
                function M:m() end\n\
                M.h = function() end\n\
                M.n = 1\n\
                local function hidden() end\n\
                return M\n";
    assert_eq!(
        exported(Grammar::Luau, text),
        Some(vec![
            pair("limit", Variable),
            pair("f", Function),
            pair("m", Function),
            pair("h", Function),
            pair("n", Variable),
        ])
    );
}

#[test]
fn luau_module_without_a_return_exports_nothing() {
    assert_eq!(exported(Grammar::Luau, "local x = 1\n"), Some(vec![]));
}

#[test]
fn luau_exports_are_unknown_when_the_returned_value_is_not_provable() {
    let unproven = [
        "return build()\n",
        "local M = make()\nfunction M.f() end\nreturn M\n",
        "local M = {}\nM = other\nreturn M\n",
        "local M = {}\nlocal M = {}\nreturn M\n",
        "local M = {}\nif x then return {} end\nreturn M\n",
        "return a, b\n",
        "return M\n",
    ];
    for text in unproven {
        assert_eq!(exported(Grammar::Luau, text), None, "{text:?}");
    }
}

#[test]
fn luau_return_inside_a_function_does_not_make_the_module_unknown() {
    let text = "local M = {}\nfunction M.f() return 1 end\nreturn M\n";
    assert_eq!(
        exported(Grammar::Luau, text),
        Some(vec![pair("f", Function)])
    );
}

#[test]
fn rhai_exports_public_functions_and_exported_variables_only() {
    let text = "fn open() {}\nprivate fn hidden() {}\nexport let shared = 1;\nexport const LIMIT = 2;\nlet local = 3;\nconst other = 4;\n";
    assert_eq!(
        exported(Grammar::Rhai, text),
        Some(vec![
            pair("open", Function),
            pair("shared", Variable),
            pair("LIMIT", Constant),
        ])
    );
}

#[test]
fn rhai_overloads_are_exported_once_each() {
    let text = "fn f(a) {}\nfn f(a, b) {}\n";
    assert_eq!(
        exported(Grammar::Rhai, text),
        Some(vec![pair("f", Function), pair("f", Function)])
    );
}
