use pretty_assertions::assert_eq;
use proptest::prelude::*;

use super::*;
use Grammar::{Luau, Rhai};

const CARET: char = '¦';

/// The call at the caret marked in `marked`.
fn call(grammar: Grammar, marked: &str) -> Option<CallSite> {
    let caret = marked.find(CARET).expect("the text marks the caret");
    let text = marked.replacen(CARET, "", 1);
    let mut highlighter = Highlighter::new(grammar).expect("bundled query compiles");
    highlighter.update(&text);
    highlighter.call_at(caret)
}

/// The callee path and active argument of the call at the caret.
fn typing(grammar: Grammar, marked: &str) -> Option<(Vec<String>, Option<usize>)> {
    call(grammar, marked).map(|site| (site.callee, site.active))
}

#[track_caller]
fn assert_call(grammar: Grammar, marked: &str, callee: &[&str], active: usize) {
    let expected = (
        callee.iter().map(|s| (*s).to_owned()).collect(),
        Some(active),
    );
    assert_eq!(
        typing(grammar, marked),
        Some(expected),
        "{grammar:?}: {marked:?}"
    );
}

#[track_caller]
fn assert_no_call(grammar: Grammar, marked: &str) {
    assert_eq!(call(grammar, marked), None, "{grammar:?}: {marked:?}");
}

#[test]
fn a_call_being_typed_counts_the_commas_before_the_caret() {
    for grammar in [Luau, Rhai] {
        assert_call(grammar, "spr(¦", &["spr"], 0);
        assert_call(grammar, "spr(1, ¦", &["spr"], 1);
        assert_call(grammar, "spr(1, 2, 3, 4, ¦", &["spr"], 4);
        assert_call(grammar, "spr(1, ¦)", &["spr"], 1);
        assert_call(grammar, "spr(1, ¦ 3)", &["spr"], 1);
    }
}

#[test]
fn a_caret_inside_a_finished_call_finds_its_argument() {
    for grammar in [Luau, Rhai] {
        assert_call(grammar, "spr(¦1, 2)", &["spr"], 0);
        assert_call(grammar, "spr(1¦, 2)", &["spr"], 0);
        assert_call(grammar, "spr(1, 2¦, 3)", &["spr"], 1);
    }
}

#[test]
fn the_callee_keeps_its_path() {
    assert_call(Luau, "audio.tone(1, ¦", &["audio", "tone"], 1);
    assert_call(Luau, "audio.tone(1, ¦\nend", &["audio", "tone"], 1);
    assert_call(Rhai, "audio::tone(1, ¦", &["audio", "tone"], 1);
    assert_call(Rhai, "audio::tone(1, ¦\n}", &["audio", "tone"], 1);
    assert_call(Rhai, "obj.method(1, ¦", &["obj", "method"], 1);
}

#[test]
fn a_luau_method_call_is_marked_as_one() {
    let site = call(Luau, "obj:method(1, ¦").expect("a call");
    assert_eq!(site.callee, ["obj", "method"]);
    assert!(site.method);
    assert!(!call(Luau, "audio.tone(1, ¦").expect("a call").method);
}

#[test]
fn the_open_offset_is_the_opening_parenthesis() {
    for grammar in [Luau, Rhai] {
        let marked = "local x = 1\nspr(1, ¦";
        let site = call(grammar, marked).expect("a call");
        assert_eq!(site.open, marked.find('(').unwrap(), "{grammar:?}");
    }
}

#[test]
fn nested_calls_belong_to_the_innermost_open_one() {
    for grammar in [Luau, Rhai] {
        assert_call(grammar, "max(min(a, ¦", &["min"], 1);
        assert_call(grammar, "max(min(a, ¦b), c)", &["min"], 1);
        assert_call(grammar, "text(spr(1, ¦", &["spr"], 1);
    }
}

#[test]
fn a_finished_inner_call_hands_the_caret_back_to_the_outer_one() {
    for grammar in [Luau, Rhai] {
        assert_call(grammar, "max(min(a, b), ¦", &["max"], 1);
        assert_call(grammar, "max(min(a, b)¦", &["max"], 0);
        assert_call(grammar, "max(min(a, b), c¦)", &["max"], 1);
    }
}

#[test]
fn commas_inside_structured_arguments_are_not_counted() {
    assert_call(Luau, "f({a, b}, ¦", &["f"], 1);
    assert_call(Luau, "f({a, ¦", &["f"], 0);
    assert_call(Luau, "f(function(a, b) return a end, ¦", &["f"], 1);
    assert_call(Luau, "f(function(a, b) local x, y = 1, 2 end, ¦", &["f"], 1);
    assert_call(Rhai, "f(#{a: 1, b: 2}, ¦", &["f"], 1);
    assert_call(Rhai, "f(#{a: 1, ¦", &["f"], 0);
    assert_call(Rhai, "foo(|a, b| a + b, ¦", &["foo"], 1);
    assert_call(Rhai, "foo(|a, b| ¦", &["foo"], 0);
    assert_call(Rhai, "foo(|a, b| { let x = 1; x }, ¦", &["foo"], 1);
}

#[test]
fn a_call_inside_another_structure_is_found() {
    assert_call(Luau, "x = {spr(1, ¦", &["spr"], 1);
    assert_call(Luau, "local t = {\n  a = spr(1, ¦,\n}", &["spr"], 1);
    assert_call(Rhai, "let a = [spr(1, ¦", &["spr"], 1);
    assert_call(Rhai, "let m = #{ a: spr(1, ¦ };", &["spr"], 1);
}

#[test]
fn a_call_inside_a_block_is_found_across_lines() {
    assert_call(Luau, "function tick()\n  spr(1, ¦\nend", &["spr"], 1);
    assert_call(
        Luau,
        "function tick()\n  spr(1,\n    2,\n    ¦\nend",
        &["spr"],
        2,
    );
    assert_call(
        Luau,
        "for i = 1, 3 do\n  audio.tone(1, ¦\nend",
        &["audio", "tone"],
        1,
    );
    assert_call(Luau, "f(function(a, b)\n  g(1, ¦\nend)", &["g"], 1);
    assert_call(Rhai, "fn tick() {\n  spr(1, ¦\n}", &["spr"], 1);
    assert_call(Rhai, "fn tick() {\n  spr(1,\n    2,\n    ¦\n}", &["spr"], 2);
    assert_call(
        Rhai,
        "for i in 0..3 {\n  audio::tone(1, ¦\n}",
        &["audio", "tone"],
        1,
    );
    assert_call(Rhai, "foo(|a, b| {\n  g(1, ¦\n})", &["g"], 1);
}

#[test]
fn the_text_after_the_caret_does_not_change_the_answer() {
    for grammar in [Luau, Rhai] {
        assert_call(grammar, "spr(1, ¦\nspr(2, 3)", &["spr"], 1);
    }
    assert_call(Luau, "spr(1, ¦\nprint('x')", &["spr"], 1);
    assert_call(Luau, "local x = spr(1, ¦\nlocal y = 2", &["spr"], 1);
    assert_call(Rhai, "let x = spr(1, ¦;\nlet y = 2;", &["spr"], 1);
    assert_call(Rhai, "spr(1, ¦\ntext(\"x\");", &["spr"], 1);
}

#[test]
fn an_earlier_statement_is_not_part_of_the_call() {
    assert_call(Luau, "print(\"x\")\nspr(1, ¦", &["spr"], 1);
    assert_call(Rhai, "print(\"x\");\nspr(1, ¦", &["spr"], 1);
    assert_call(Luau, "return f(1, ¦", &["f"], 1);
    assert_call(Rhai, "return f(1, ¦", &["f"], 1);
    assert_call(Luau, "if key(Key.A, ¦) then end", &["key"], 1);
    assert_call(Rhai, "if key(Key::A, ¦) { }", &["key"], 1);
}

#[test]
fn strings_and_comments_do_not_confuse_the_count() {
    for grammar in [Luau, Rhai] {
        assert_call(grammar, "spr(\"a,b(\", ¦", &["spr"], 1);
        assert_call(grammar, "spr(\"ab¦", &["spr"], 0);
        assert_call(grammar, "spr(1, \"ab¦", &["spr"], 1);
    }
    assert_call(Luau, "spr(1, -- ( ,\n  ¦", &["spr"], 1);
    assert_call(Rhai, "spr(1, // ( ,\n  ¦", &["spr"], 1);
    assert_call(Luau, "f(`x{a}`, ¦", &["f"], 1);
    assert_call(Rhai, "f(`x${a}`, ¦", &["f"], 1);
}

#[test]
fn offsets_count_bytes_not_characters() {
    for grammar in [Luau, Rhai] {
        assert_call(grammar, "spr(\"é,ü\", ¦", &["spr"], 1);
    }
}

#[test]
fn declarations_grouping_and_calls_on_results_are_not_calls() {
    for grammar in [Luau, Rhai] {
        assert_no_call(grammar, "f(x)(¦");
        assert_no_call(grammar, "f(a)(b, ¦");
        assert_no_call(grammar, "spr(1) ¦");
        assert_no_call(grammar, "spr(1, 2)¦");
        assert_no_call(grammar, "if (a ¦");
        assert_no_call(grammar, "spr¦");
        assert_no_call(grammar, "¦");
    }
    assert_no_call(Luau, "function f(a, ¦");
    assert_no_call(Luau, "local function f(a, ¦");
    assert_no_call(Luau, "local x = (a + ¦");
    assert_no_call(Rhai, "fn f(a, ¦");
    assert_no_call(Rhai, "let x = (a + ¦");
}

#[test]
fn a_function_name_that_ends_in_fn_is_still_a_call() {
    assert_call(Rhai, "myfn(1, ¦", &["myfn"], 1);
    assert_call(Luau, "myfunction(1, ¦", &["myfunction"], 1);
}

#[test]
fn an_unclosed_anonymous_function_in_luau_hides_the_active_argument() {
    let site = call(Luau, "f(function(a, b) local x, y = 1, ¦").expect("a call");
    assert_eq!(site.callee, ["f"]);
    assert_eq!(site.active, None);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn any_text_and_caret_give_an_answer_without_panicking(
        text in "[a-zA-Z0-9_ .:,\n\t\"'`{}()\\[\\]#|/*é-]{0,80}",
        caret in 0usize..96,
        luau in any::<bool>(),
    ) {
        let grammar = if luau { Luau } else { Rhai };
        let mut caret = caret.min(text.len());
        while !text.is_char_boundary(caret) {
            caret -= 1;
        }
        let mut highlighter = Highlighter::new(grammar).expect("bundled query compiles");
        highlighter.update(&text);
        if let Some(site) = highlighter.call_at(caret) {
            prop_assert!(site.open < caret);
            prop_assert_eq!(text.as_bytes()[site.open], b'(');
            prop_assert!(!site.callee.is_empty());
        }
    }
}
