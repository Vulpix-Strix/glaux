use glaux_docs::Library;
use glaux_syntax::{DefinitionKind, Grammar};
use pretty_assertions::assert_eq;

use super::*;

/// Asks about the word at the `¦` in `marked`.
fn ask(language: ScriptLanguage, marked: &str) -> Option<(Range<usize>, String)> {
    let at = marked.find('¦').expect("a marker");
    let text = marked.replace('¦', "");
    let grammar = match language {
        ScriptLanguage::Luau => Grammar::Luau,
        ScriptLanguage::Rhai => Grammar::Rhai,
    };
    let mut syntax = Highlighter::new(grammar).expect("bundled query compiles");
    syntax.update(&text);
    let library = Library::embedded();
    let found = hover(library.api(), language, &text, at, &syntax)?;
    let subject = match found.subject {
        Subject::Entry(entry) => format!("entry {}", entry.id()),
        Subject::Constant { group, member } => format!("constant {} {member}", group.id()),
        Subject::Local(definition) => format!("local {} {:?}", definition.name, definition.kind),
        Subject::Declared(name) => format!("declared {name}"),
        Subject::Unknown => "unknown".to_owned(),
    };
    Some((found.range, subject))
}

fn subject(language: ScriptLanguage, marked: &str) -> String {
    ask(language, marked).expect("a word").1
}

const LUAU: ScriptLanguage = ScriptLanguage::Luau;
const RHAI: ScriptLanguage = ScriptLanguage::Rhai;

#[test]
fn api_names_resolve_with_or_without_a_receiver() {
    assert_eq!(subject(LUAU, "sp¦r(1)"), "entry api/graphics/spr");
    assert_eq!(subject(LUAU, "audio.to¦ne(1)"), "entry api/audio/tone");
    assert_eq!(subject(RHAI, "audio::to¦ne(1);"), "entry api/audio/tone");
}

#[test]
fn the_range_is_the_whole_word_for_any_offset_inside_it() {
    let (range, _) = ask(LUAU, "audio.¦tone(1)").unwrap();
    assert_eq!(range, 6..10);
    let (range, _) = ask(LUAU, "audio.tone¦(1)").unwrap();
    assert_eq!(range, 6..10);
}

#[test]
fn locals_hide_the_api_and_report_what_they_are() {
    assert_eq!(
        subject(LUAU, "local function mine() end\nmi¦ne()"),
        format!("local mine {:?}", DefinitionKind::Function)
    );
    assert_eq!(
        subject(LUAU, "local audio = {}\naudi¦o.tone(1)"),
        format!("local audio {:?}", DefinitionKind::Variable)
    );
    // The members of a user value are unknown.
    assert_eq!(subject(LUAU, "local audio = {}\naudio.to¦ne(1)"), "unknown");
}

#[test]
fn a_name_being_declared_is_the_scripts_own() {
    assert_eq!(subject(LUAU, "local sc¦ore = 1"), "declared score");
    assert_eq!(subject(LUAU, "local function fo¦o() end"), "declared foo");
    assert_eq!(subject(RHAI, "fn f(pa¦ram) {}"), "declared param");
}

#[test]
fn constant_members_stand_for_their_group() {
    let luau = subject(LUAU, "Key.¦A");
    assert!(
        luau.starts_with("constant ") && luau.ends_with(" A"),
        "{luau}"
    );
    let rhai = subject(RHAI, "Key::¦A");
    assert!(
        rhai.starts_with("constant ") && rhai.ends_with(" A"),
        "{rhai}"
    );
    assert_eq!(subject(LUAU, "Key.¦Nope"), "unknown");
}

#[test]
fn unknown_words_are_unknown() {
    assert_eq!(subject(LUAU, "nothing_h¦ere()"), "unknown");
}

#[test]
fn comments_strings_and_numbers_have_no_hover() {
    assert_eq!(ask(LUAU, "-- sp¦r"), None);
    assert_eq!(ask(LUAU, "x = \"sp¦r\""), None);
    assert_eq!(ask(LUAU, "x = 1¦2"), None);
}

#[test]
fn an_offset_outside_a_word_or_the_text_has_no_hover() {
    assert_eq!(ask(LUAU, "spr(¦)"), None);
    let mut syntax = Highlighter::new(Grammar::Luau).unwrap();
    syntax.update("spr()");
    let library = Library::embedded();
    assert_eq!(hover(library.api(), LUAU, "spr()", 99, &syntax), None);
}

#[test]
fn text_the_tree_has_not_parsed_still_answers_for_the_api() {
    let mut syntax = Highlighter::new(Grammar::Luau).unwrap();
    syntax.update("local a = 1");
    let library = Library::embedded();
    let found = hover(library.api(), LUAU, "spr(1)", 1, &syntax).expect("a word");
    assert_eq!(found.range, 0..3);
}
