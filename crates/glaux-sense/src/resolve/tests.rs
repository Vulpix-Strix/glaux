use glaux_docs::{Kind, Library};
use glaux_syntax::{DefinitionKind, Grammar};
use pretty_assertions::assert_eq;

use super::*;

fn syntax(grammar: Grammar, text: &str) -> Highlighter {
    let mut syntax = Highlighter::new(grammar).expect("bundled query compiles");
    syntax.update(text);
    syntax
}

#[test]
fn api_names_resolve_by_path_and_language() {
    let library = Library::embedded();
    let api = library.api();
    let syntax = syntax(Grammar::Luau, "");

    let tone = resolve(api, ScriptLanguage::Luau, &syntax, 0, &["audio", "tone"]);
    let Some(Resolved::Entry(tone)) = tone else {
        panic!("audio.tone should resolve");
    };
    assert_eq!(tone.kind(), Kind::Function);

    // Luau spells the text function `print`, Rhai spells it `text`.
    assert!(resolve(api, ScriptLanguage::Luau, &syntax, 0, &["print"]).is_some());
    assert!(resolve(api, ScriptLanguage::Luau, &syntax, 0, &["text"]).is_none());
    assert!(resolve(api, ScriptLanguage::Rhai, &syntax, 0, &["text"]).is_some());
    assert!(resolve(api, ScriptLanguage::Luau, &syntax, 0, &["audio", "nope"]).is_none());
    assert!(resolve(api, ScriptLanguage::Luau, &syntax, 0, &[]).is_none());
}

#[test]
fn a_local_hides_the_api_name() {
    let library = Library::embedded();
    let text = "local audio = {}\n";
    let syntax = syntax(Grammar::Luau, text);

    let found = resolve(
        library.api(),
        ScriptLanguage::Luau,
        &syntax,
        text.len(),
        &["audio"],
    );
    let Some(Resolved::Local(local)) = found else {
        panic!("the local should win");
    };
    assert_eq!(local.kind, DefinitionKind::Variable);

    // Nothing is known about the members of a user value.
    let member = resolve(
        library.api(),
        ScriptLanguage::Luau,
        &syntax,
        text.len(),
        &["audio", "tone"],
    );
    assert_eq!(member, None);
}

#[test]
fn a_local_function_resolves_with_its_parameters() {
    let library = Library::embedded();
    let text = "local function area(w, h) end\n";
    let syntax = syntax(Grammar::Luau, text);
    let found = resolve(
        library.api(),
        ScriptLanguage::Luau,
        &syntax,
        text.len(),
        &["area"],
    );
    let Some(Resolved::Local(local)) = found else {
        panic!("area should resolve to the local");
    };
    assert_eq!(local.params, ["w", "h"]);
}

#[test]
fn every_constant_group_of_the_catalog_has_members() {
    let library = Library::embedded();
    let groups: Vec<_> = library
        .api()
        .entries()
        .iter()
        .filter(|entry| entry.kind() == Kind::ConstantGroup)
        .collect();
    assert!(!groups.is_empty());
    for group in groups {
        let members = constant_members(group);
        assert!(
            members.is_some_and(|members| !members.is_empty()),
            "{} has no member table in glaux-sense",
            group.id()
        );
    }
}

#[test]
fn constant_members_are_the_names_cartridges_write() {
    let library = Library::embedded();
    let keys = library.api().lookup(ScriptLanguage::Luau, &[], "Key");
    let members = keys.and_then(constant_members).expect("Key has members");
    assert!(members.contains(&"PageUp"));
    assert!(members.contains(&"Space"));
    let buttons = library.api().lookup(ScriptLanguage::Luau, &[], "Button");
    assert_eq!(
        buttons.and_then(constant_members),
        Some(vec!["Up", "Down", "Left", "Right", "A", "B"])
    );
}
