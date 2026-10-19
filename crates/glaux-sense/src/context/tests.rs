use ScriptLanguage::{Luau, Rhai};
use pretty_assertions::assert_eq;

use super::*;

/// Reads the context at the `|` in `marked`.
fn read(language: ScriptLanguage, marked: &str) -> Context {
    let caret = marked.find('|').expect("the text marks the caret with `|`");
    let text = marked.replacen('|', "", 1);
    let grammar = match language {
        ScriptLanguage::Luau => glaux_syntax::Grammar::Luau,
        ScriptLanguage::Rhai => glaux_syntax::Grammar::Rhai,
    };
    let mut syntax = Highlighter::new(grammar).expect("bundled query compiles");
    syntax.update(&text);
    context(language, &text, caret, &syntax).expect("the caret is inside the text")
}

fn position(language: ScriptLanguage, marked: &str) -> Position {
    read(language, marked).position
}

fn member(receiver: &[&str], separator: Separator) -> Position {
    Position::Member {
        receiver: receiver.iter().map(|s| (*s).to_owned()).collect(),
        separator,
    }
}

fn reference(top_level_statement: bool) -> Position {
    Position::Reference {
        top_level_statement,
    }
}

#[test]
fn the_word_around_the_caret_is_the_anchor_and_the_text_before_it_the_prefix() {
    let context = read(Luau, "x = rect|fill(1)");
    assert_eq!(context.prefix, "rect");
    assert_eq!(context.anchor, 4..12);
    let context = read(Luau, "x = |");
    assert_eq!(context.prefix, "");
    assert_eq!(context.anchor, 4..4);
}

#[test]
fn members_follow_a_separator() {
    assert_eq!(
        position(Luau, "audio.|"),
        member(&["audio"], Separator::Dot)
    );
    assert_eq!(
        position(Luau, "audio.to|"),
        member(&["audio"], Separator::Dot)
    );
    assert_eq!(
        position(Rhai, "audio::|"),
        member(&["audio"], Separator::Path)
    );
    assert_eq!(
        position(Rhai, "f(Key::Sp|"),
        member(&["Key"], Separator::Path)
    );
}

#[test]
fn incomplete_code_after_the_separator_still_gives_a_member() {
    // These are the shapes the parse tree gets wrong.
    assert_eq!(
        position(Luau, "audio.|\nend"),
        member(&["audio"], Separator::Dot)
    );
    assert_eq!(
        position(Luau, "if key(Key.|\nend"),
        member(&["Key"], Separator::Dot)
    );
    assert_eq!(
        position(Rhai, "if key(Key::|"),
        member(&["Key"], Separator::Path)
    );
}

#[test]
fn receivers_can_be_chains() {
    assert_eq!(position(Luau, "a.b.|"), member(&["a", "b"], Separator::Dot));
    assert_eq!(
        position(Rhai, "a::b::|"),
        member(&["a", "b"], Separator::Path)
    );
}

#[test]
fn a_receiver_that_is_not_a_plain_chain_is_empty() {
    assert_eq!(position(Luau, "f().|"), member(&[], Separator::Dot));
    assert_eq!(position(Luau, "t[1].|"), member(&[], Separator::Dot));
    assert_eq!(position(Luau, "a.b().c.|"), member(&[], Separator::Dot));
    assert_eq!(position(Luau, "(a).|"), member(&[], Separator::Dot));
}

#[test]
fn concatenation_and_numbers_are_not_member_access() {
    assert_eq!(position(Luau, "x = a ..|"), reference(false));
    assert_eq!(position(Luau, "x = a..co|"), reference(false));
    assert_eq!(position(Luau, "x = 1.|"), Position::Suppressed);
    assert_eq!(position(Luau, "x = 12|"), Position::Suppressed);
    assert_eq!(position(Rhai, "x = 1.5|"), Position::Suppressed);
}

#[test]
fn luau_colons_start_types_and_methods() {
    assert_eq!(position(Luau, "local x: nu|"), Position::Suppressed);
    assert_eq!(position(Luau, "obj:me|"), Position::Suppressed);
    assert_eq!(position(Luau, "x = y :: nu|"), Position::Suppressed);
}

#[test]
fn a_rhai_colon_separates_map_keys_from_values() {
    assert_eq!(position(Rhai, "let m = #{ a: co|"), reference(false));
}

#[test]
fn comments_and_strings_suppress_everything() {
    for (language, marked) in [
        (Luau, "x = 1 -- co|"),
        (Luau, "--[[ co|"),
        (Luau, "--[[ audio.|"),
        (Luau, "x = \"co|"),
        (Luau, "x = [[ co|"),
        (Luau, "x = `co|"),
        (Rhai, "x = 1; // co|"),
        (Rhai, "/* co|"),
        (Rhai, "/* a /* b */ co|"),
        (Rhai, "x = \"co|"),
        (Rhai, "x = `co|"),
    ] {
        assert_eq!(position(language, marked), Position::Suppressed, "{marked}");
    }
}

#[test]
fn text_after_a_closed_comment_or_string_is_code() {
    assert_eq!(position(Luau, "--[[ a ]] x = co|"), reference(false));
    assert_eq!(position(Luau, "x = \"a\" .. co|"), reference(false));
    assert_eq!(position(Rhai, "/* a */ x = co|"), reference(false));
}

#[test]
fn interpolations_hold_code() {
    assert_eq!(position(Luau, "x = `a {co|} b`"), reference(false));
    assert_eq!(position(Rhai, "x = `a ${co|} b`"), reference(false));
    assert_eq!(
        position(Luau, "x = `a {audio.|}`"),
        member(&["audio"], Separator::Dot)
    );
}

#[test]
fn declaration_names_follow_declaring_words() {
    for (language, marked) in [
        (Luau, "local co|"),
        (Luau, "local function co|"),
        (Luau, "for co|"),
        (Luau, "function f(a, co|) end"),
        (Rhai, "let co|"),
        (Rhai, "const co|"),
        (Rhai, "fn f(a, co|) {}"),
        (Rhai, "import \"m\" as co|"),
    ] {
        assert!(
            matches!(position(language, marked), Position::Declaration { .. }),
            "{marked}"
        );
    }
}

#[test]
fn callbacks_are_declared_by_global_functions_at_the_top_level() {
    let site = |callback_site| Position::Declaration { callback_site };
    assert_eq!(position(Luau, "function ti|"), site(true));
    assert_eq!(position(Luau, "local x = 1\nfunction ti|"), site(true));
    assert_eq!(position(Luau, "local function ti|"), site(false));
    assert_eq!(position(Luau, "function f()\n  function ti|"), site(false));
    assert_eq!(position(Rhai, "fn ti|"), site(true));
    assert_eq!(position(Rhai, "private fn ti|"), site(false));
    assert_eq!(position(Rhai, "fn f() {\n  fn ti|"), site(false));
    assert_eq!(position(Luau, "x = function ti|"), site(false));
}

#[test]
fn a_word_starting_a_top_level_statement_is_marked() {
    assert_eq!(position(Luau, "ti|"), reference(true));
    assert_eq!(position(Luau, "local a = 1\n\nti|"), reference(true));
    assert_eq!(position(Luau, "function f() end\nti|"), reference(true));
    assert_eq!(position(Rhai, "let a = 1;\nti|"), reference(true));
    assert_eq!(position(Rhai, "fn f() {}\nti|"), reference(true));
    assert_eq!(position(Luau, "  ti|"), reference(true));
}

#[test]
fn a_word_inside_a_block_does_not_start_a_top_level_statement() {
    assert_eq!(position(Luau, "function f()\n  ti|"), reference(false));
    assert_eq!(position(Luau, "function f()\n  ti|\nend"), reference(false));
    assert_eq!(position(Luau, "if a then\n  ti|"), reference(false));
    assert_eq!(position(Luau, "while a do\n  ti|"), reference(false));
    assert_eq!(position(Rhai, "fn f() {\n  ti|"), reference(false));
    assert_eq!(position(Rhai, "if a {\n  ti|\n}"), reference(false));
}

#[test]
fn a_word_inside_an_expression_does_not_start_a_statement() {
    assert_eq!(position(Luau, "x = ti|"), reference(false));
    assert_eq!(position(Luau, "f(a,\n  ti|"), reference(false));
    assert_eq!(position(Luau, "local t = {\n  ti|"), reference(false));
    assert_eq!(position(Luau, "x = 1 +\n  ti|"), reference(false));
    assert_eq!(position(Rhai, "f(a,\n  ti|"), reference(false));
}

#[test]
fn a_finished_block_leaves_the_top_level_open_again() {
    assert_eq!(position(Luau, "if a then\nend\nti|"), reference(true));
    assert_eq!(
        position(Luau, "local a = if x then 1 else 2\nti|"),
        reference(true)
    );
}

#[test]
fn line_indent_is_that_of_the_line_with_the_word_or_its_function_keyword() {
    assert_eq!(read(Luau, "  \tti|").line_indent, "  \t");
    assert_eq!(read(Luau, "x = ti|").line_indent, "");
    assert_eq!(read(Luau, "  function ti|").line_indent, "  ");
}

#[test]
fn the_caret_must_be_inside_the_text() {
    let mut syntax = Highlighter::new(glaux_syntax::Grammar::Luau).expect("bundled query compiles");
    syntax.update("é");
    assert_eq!(context(Luau, "é", 1, &syntax), None);
    assert_eq!(context(Luau, "é", 9, &syntax), None);
}
