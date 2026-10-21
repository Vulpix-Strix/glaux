use glaux_syntax::Grammar;
use pretty_assertions::assert_eq;

use super::*;

const LUAU: ScriptLanguage = ScriptLanguage::Luau;
const RHAI: ScriptLanguage = ScriptLanguage::Rhai;

/// Resolves the name at the `¦` in `marked`, with the declared range of a local result
/// replaced by the text it covers.
fn resolve_at(language: ScriptLanguage, marked: &str) -> String {
    let at = marked.find('¦').expect("a marker");
    let text = marked.replace('¦', "");
    let grammar = match language {
        ScriptLanguage::Luau => Grammar::Luau,
        ScriptLanguage::Rhai => Grammar::Rhai,
    };
    let mut syntax = Highlighter::new(grammar).expect("bundled query compiles");
    syntax.update(&text);
    match definition(language, &text, at, &syntax) {
        Resolution::Local(found) => format!("local {}@{}", found.name, found.declared.start),
        Resolution::Import { module } => format!("import {module}"),
        Resolution::Member { module, member } => format!("member {module} {member}"),
        Resolution::Ambiguous => "ambiguous".to_owned(),
        Resolution::Unresolved => "unresolved".to_owned(),
    }
}

#[test]
fn a_use_leads_to_the_declaration_in_effect() {
    assert_eq!(resolve_at(LUAU, "local a = 1\nprint(¦a)"), "local a@6");
    assert_eq!(resolve_at(RHAI, "let a = 1;\nprint(¦a);"), "local a@4");
}

#[test]
fn an_inner_declaration_hides_the_outer_one() {
    let luau = "local a = 1\ndo local a = 2; print(¦a) end";
    assert_eq!(resolve_at(LUAU, luau), "local a@21");
    let rhai = "let a = 1;\n{ let a = 2; print(¦a); }";
    assert_eq!(resolve_at(RHAI, rhai), "local a@17");
}

#[test]
fn a_local_is_not_visible_in_its_own_initializer() {
    assert_eq!(resolve_at(LUAU, "local a = ¦a"), "unresolved");
    assert_eq!(resolve_at(LUAU, "local a = 1\nlocal a = ¦a"), "local a@6");
}

#[test]
fn functions_are_found_wherever_they_are_written() {
    assert_eq!(resolve_at(LUAU, "¦go()\nfunction go() end"), "local go@14");
    assert_eq!(resolve_at(RHAI, "¦go();\nfn go() {}"), "local go@9");
}

#[test]
fn implicit_self_has_no_declaration_to_jump_to_but_is_still_local() {
    let found = resolve_at(LUAU, "function Obj:run() return ¦self end");
    assert!(found.starts_with("local self@"), "{found}");
}

#[test]
fn a_rhai_function_cannot_reach_the_locals_around_it() {
    assert_eq!(resolve_at(RHAI, "let a = 1;\nfn f() { ¦a }"), "unresolved");
}

#[test]
fn engine_api_and_unknown_names_are_not_definitions() {
    assert_eq!(resolve_at(LUAU, "¦spr(1)"), "unresolved");
    assert_eq!(resolve_at(LUAU, "audio.¦tone(1)"), "unresolved");
    assert_eq!(resolve_at(RHAI, "¦nothing();"), "unresolved");
}

#[test]
fn a_name_on_its_own_declaration_resolves_to_nothing() {
    assert_eq!(resolve_at(LUAU, "local ¦a = 1"), "unresolved");
}

#[test]
fn functions_declared_twice_are_ambiguous() {
    assert_eq!(
        resolve_at(LUAU, "function f() end\nfunction f() end\n¦f()"),
        "ambiguous"
    );
    assert_eq!(
        resolve_at(RHAI, "fn f(a) {}\nfn f(a, b) {}\n¦f(1);"),
        "ambiguous"
    );
}

#[test]
fn an_import_leads_from_its_string_and_from_its_alias() {
    let luau = "local m = require(\"game.m\")\nprint(¦m)";
    assert_eq!(resolve_at(LUAU, luau), "import game.m");
    assert_eq!(
        resolve_at(LUAU, "local m = require(\"ga¦me.m\")"),
        "import game.m"
    );
    assert_eq!(
        resolve_at(RHAI, "import \"util\" as u;\nprint(¦u);"),
        "import util"
    );
    assert_eq!(resolve_at(RHAI, "import \"ut¦il\" as u;"), "import util");
}

#[test]
fn a_member_of_an_alias_names_the_module_and_the_member() {
    let luau = "local m = require(\"game.m\")\nm.¦clamp(1)";
    assert_eq!(resolve_at(LUAU, luau), "member game.m clamp");
    let rhai = "import \"util\" as u;\nu::¦clamp(1);";
    assert_eq!(resolve_at(RHAI, rhai), "member util clamp");
}

#[test]
fn a_shadowed_or_reassigned_alias_leads_nowhere() {
    let shadowed = "local m = require(\"game.m\")\ndo local m = {}; m.¦f() end";
    assert_eq!(resolve_at(LUAU, shadowed), "unresolved");
    let reassigned = "local m = require(\"game.m\")\nm = other\nm.¦f()";
    assert_eq!(resolve_at(LUAU, reassigned), "unresolved");
}

#[test]
fn members_of_ordinary_values_and_deep_paths_are_not_followed() {
    assert_eq!(resolve_at(LUAU, "local t = {}\nt.¦f()"), "unresolved");
    let deep = "local m = require(\"game.m\")\nm.a.¦b()";
    assert_eq!(resolve_at(LUAU, deep), "unresolved");
}
