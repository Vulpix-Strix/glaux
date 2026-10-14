use pretty_assertions::assert_eq;
use proptest::prelude::*;

use super::*;
use SyntaxKind::*;

fn highlighter(grammar: Grammar, text: &str) -> Highlighter {
    let mut highlighter = Highlighter::new(grammar).expect("bundled query compiles");
    highlighter.update(text);
    highlighter
}

/// The styled slices of `text` with their kinds, in order.
fn tagged(grammar: Grammar, text: &str) -> Vec<(&str, SyntaxKind)> {
    highlighter(grammar, text)
        .spans(0..text.len())
        .into_iter()
        .map(|span| (&text[span.range], span.kind))
        .collect()
}

fn kind_of(tags: &[(&str, SyntaxKind)], token: &str) -> Option<SyntaxKind> {
    tags.iter().find(|(text, _)| *text == token).map(|t| t.1)
}

#[test]
fn every_query_capture_has_a_kind() {
    for grammar in [Grammar::Luau, Grammar::Rhai] {
        Highlighter::new(grammar).expect("capture names map to kinds");
    }
}

#[test]
fn luau_categories() {
    let text = "local MAX = 10\n\
                local function area(r: number): Vec2\n  return r * 3.14 -- circle\nend\n\
                print(\"hi\")";
    let tags = tagged(Grammar::Luau, text);
    assert_eq!(kind_of(&tags, "local"), Some(Keyword));
    assert_eq!(kind_of(&tags, "function"), Some(Keyword));
    assert_eq!(kind_of(&tags, "return"), Some(Keyword));
    assert_eq!(kind_of(&tags, "MAX"), Some(Constant));
    assert_eq!(kind_of(&tags, "10"), Some(Number));
    assert_eq!(kind_of(&tags, "3.14"), Some(Number));
    assert_eq!(kind_of(&tags, "area"), Some(Function));
    assert_eq!(kind_of(&tags, "print"), Some(Function));
    assert_eq!(kind_of(&tags, "r"), Some(Variable));
    assert_eq!(kind_of(&tags, "number"), Some(Type));
    assert_eq!(kind_of(&tags, "Vec2"), Some(Type));
    assert_eq!(kind_of(&tags, "\"hi\""), Some(String));
    assert_eq!(kind_of(&tags, "-- circle"), Some(Comment));
    assert_eq!(kind_of(&tags, "*"), Some(Operator));
}

#[test]
fn luau_lowercase_identifiers_are_not_constants() {
    // The upstream query relies on `#lua-match?`, which the Rust bindings do not enforce.
    let tags = tagged(Grammar::Luau, "local speed = 1");
    assert_eq!(kind_of(&tags, "speed"), Some(Variable));
}

#[test]
fn luau_method_and_field_calls_name_the_function() {
    let tags = tagged(Grammar::Luau, "player:jump()\nmath.floor(1)");
    assert_eq!(kind_of(&tags, "jump"), Some(Function));
    assert_eq!(kind_of(&tags, "floor"), Some(Function));
    assert_eq!(kind_of(&tags, "player"), Some(Variable));
}

#[test]
fn luau_multiline_constructs_are_single_spans() {
    let text = "--[[ one\ntwo ]] local s = [[a\nb]]";
    let tags = tagged(Grammar::Luau, text);
    assert_eq!(kind_of(&tags, "--[[ one\ntwo ]]"), Some(Comment));
    assert_eq!(kind_of(&tags, "[[a\nb]]"), Some(String));
}

#[test]
fn luau_interpolation_splits_the_string() {
    let tags = tagged(Grammar::Luau, "x = `a {r} b`");
    assert_eq!(
        tags.iter()
            .filter(|t| t.0 != "x" && t.0 != "=")
            .copied()
            .collect::<Vec<_>>(),
        vec![
            ("`a ", String),
            ("{", Operator),
            ("r", Variable),
            ("}", Operator),
            (" b`", String)
        ]
    );
}

#[test]
fn luau_incomplete_code_keeps_earlier_tokens() {
    let text = "local ok = 1\nfunction broken(\n  local s = \"open";
    let tags = tagged(Grammar::Luau, text);
    assert_eq!(kind_of(&tags, "local"), Some(Keyword));
    assert_eq!(kind_of(&tags, "1"), Some(Number));
}

#[test]
fn luau_multibyte_text_keeps_byte_ranges() {
    let text = "-- héllo ✓\nlocal ñ = \"日本語\" .. 1";
    let highlighter = highlighter(Grammar::Luau, text);
    for span in highlighter.spans(0..text.len()) {
        assert!(text.is_char_boundary(span.range.start));
        assert!(text.is_char_boundary(span.range.end));
    }
    let tags = tagged(Grammar::Luau, text);
    assert_eq!(kind_of(&tags, "-- héllo ✓"), Some(Comment));
    assert_eq!(kind_of(&tags, "\"日本語\""), Some(String));
    assert_eq!(kind_of(&tags, "1"), Some(Number));
}

#[test]
fn rhai_categories() {
    let text = "const LIMIT = 3;\n\
                fn area(r) {\n  let x = r * 2.5; // scaled\n  return x;\n}\n\
                print(\"hi\");";
    let tags = tagged(Grammar::Rhai, text);
    assert_eq!(kind_of(&tags, "const"), Some(Keyword));
    assert_eq!(kind_of(&tags, "fn"), Some(Keyword));
    assert_eq!(kind_of(&tags, "let"), Some(Keyword));
    assert_eq!(kind_of(&tags, "return"), Some(Keyword));
    assert_eq!(kind_of(&tags, "LIMIT"), Some(Constant));
    assert_eq!(kind_of(&tags, "3"), Some(Number));
    assert_eq!(kind_of(&tags, "2.5"), Some(Number));
    assert_eq!(kind_of(&tags, "area"), Some(Function));
    assert_eq!(kind_of(&tags, "print"), Some(Function));
    assert_eq!(kind_of(&tags, "r"), Some(Variable));
    assert_eq!(kind_of(&tags, "\"hi\""), Some(String));
    assert_eq!(kind_of(&tags, "// scaled"), Some(Comment));
    assert_eq!(kind_of(&tags, "*"), Some(Operator));
}

#[test]
fn rhai_multiline_comment_and_template_string() {
    let text = "/* a\nb */ let s = `x ${1 + 2} y`;";
    let tags = tagged(Grammar::Rhai, text);
    assert_eq!(kind_of(&tags, "/* a\nb */"), Some(Comment));
    assert_eq!(kind_of(&tags, "`x "), Some(String));
    assert_eq!(kind_of(&tags, "1"), Some(Number));
    assert_eq!(kind_of(&tags, " y`"), Some(String));
}

#[test]
fn rhai_path_and_method_calls_name_the_function() {
    let tags = tagged(Grammar::Rhai, "foo::bar(1); a.b(2);");
    assert_eq!(kind_of(&tags, "bar"), Some(Function));
    assert_eq!(kind_of(&tags, "foo"), Some(Variable));
    assert_eq!(kind_of(&tags, "b"), Some(Function));
    assert_eq!(kind_of(&tags, "a"), Some(Variable));
}

#[test]
fn rhai_booleans_and_in_operator() {
    let tags = tagged(Grammar::Rhai, "let ok = true; for i in 0..3 {}");
    assert_eq!(kind_of(&tags, "true"), Some(Constant));
    assert_eq!(kind_of(&tags, "in"), Some(Keyword));
}

#[test]
fn rhai_incomplete_code_keeps_earlier_tokens() {
    let tags = tagged(Grammar::Rhai, "let a = 1;\nfn broken( {\n  let s = \"open");
    assert_eq!(kind_of(&tags, "let"), Some(Keyword));
    assert_eq!(kind_of(&tags, "1"), Some(Number));
}

#[test]
fn rhai_multibyte_text_keeps_byte_ranges() {
    let text = "// héllo ✓\nlet ñ = \"日本語\"; 1";
    let highlighter = highlighter(Grammar::Rhai, text);
    for span in highlighter.spans(0..text.len()) {
        assert!(text.is_char_boundary(span.range.start));
        assert!(text.is_char_boundary(span.range.end));
    }
    let tags = tagged(Grammar::Rhai, text);
    assert_eq!(kind_of(&tags, "// héllo ✓"), Some(Comment));
    assert_eq!(kind_of(&tags, "\"日本語\""), Some(String));
}

#[test]
fn window_limits_the_spans_without_changing_them() {
    let text = "local a = 1\nlocal b = 2\nlocal c = 3\n";
    let highlighter = highlighter(Grammar::Luau, text);
    let all = highlighter.spans(0..text.len());
    let window = 12..23;
    let part = highlighter.spans(window.clone());
    let expected: Vec<Span> = all
        .into_iter()
        .filter(|s| s.range.start < window.end && s.range.end > window.start)
        .collect();
    assert_eq!(part, expected);
}

#[test]
fn identical_text_keeps_the_tree() {
    let mut highlighter = highlighter(Grammar::Luau, "local a = 1");
    let before = highlighter.spans(0..11);
    highlighter.update("local a = 1");
    assert_eq!(highlighter.spans(0..11), before);
}

fn fresh(grammar: Grammar, text: &str) -> Vec<Span> {
    highlighter(grammar, text).spans(0..text.len())
}

#[test]
fn consecutive_edits_match_a_fresh_parse() {
    let steps = [
        "local a = 1",
        "local a = 12",
        "local a = 12\nlocal b = \"x\"",
        "local a = 12\nlocal b = \"x",
        "local a = 12\nlocal b = \"x\"",
        "function f() end",
        "",
        "local a = 1",
    ];
    let mut incremental = Highlighter::new(Grammar::Luau).unwrap();
    for text in steps {
        incremental.update(text);
        assert_eq!(incremental.spans(0..text.len()), fresh(Grammar::Luau, text));
    }
}

fn source_strategy() -> impl Strategy<Value = Vec<std::string::String>> {
    let piece = prop_oneof![
        Just("local ".to_owned()),
        Just("function f() ".to_owned()),
        Just("end\n".to_owned()),
        Just("\"é日\"".to_owned()),
        Just("\"".to_owned()),
        Just("--[[".to_owned()),
        Just("]]".to_owned()),
        Just("-- c\n".to_owned()),
        Just("x = 12 + y\n".to_owned()),
        Just("if a then ".to_owned()),
        Just("✓".to_owned()),
        Just("\n".to_owned()),
    ];
    prop::collection::vec(
        prop::collection::vec(piece, 0..12).prop_map(|p| p.concat()),
        1..8,
    )
}

proptest! {
    #[test]
    fn luau_updates_match_a_fresh_parse(texts in source_strategy()) {
        let mut incremental = Highlighter::new(Grammar::Luau).unwrap();
        // Revisit earlier texts too, which is what undo and reload do.
        let order: Vec<&std::string::String> =
            texts.iter().chain(texts.iter().rev()).collect();
        for text in order {
            incremental.update(text);
            prop_assert_eq!(incremental.spans(0..text.len()), fresh(Grammar::Luau, text));
        }
    }

    #[test]
    fn rhai_updates_match_a_fresh_parse(texts in source_strategy()) {
        let mut incremental = Highlighter::new(Grammar::Rhai).unwrap();
        let order: Vec<&std::string::String> =
            texts.iter().chain(texts.iter().rev()).collect();
        for text in order {
            incremental.update(text);
            prop_assert_eq!(incremental.spans(0..text.len()), fresh(Grammar::Rhai, text));
        }
    }
}

#[test]
fn keywords_inside_comments_and_strings_are_not_highlighted_as_keywords() {
    let tags = tagged(Grammar::Luau, "-- local if end\nx = \"local end\"");
    assert_eq!(kind_of(&tags, "-- local if end"), Some(Comment));
    assert_eq!(kind_of(&tags, "\"local end\""), Some(String));
    let tags = tagged(Grammar::Rhai, "// let fn if\nlet s = \"let fn\";");
    assert_eq!(kind_of(&tags, "// let fn if"), Some(Comment));
    assert_eq!(kind_of(&tags, "\"let fn\""), Some(String));
}

fn large_document(grammar: Grammar, functions: usize) -> std::string::String {
    (0..functions)
        .map(|i| match grammar {
            Grammar::Luau => format!(
                "local function f{i}(a: number, b: Vec2): number\n  -- step {i}\n  return a * {i} + b.x .. \"ü\"\nend\n"
            ),
            Grammar::Rhai => format!(
                "fn f{i}(a, b) {{\n  // step {i}\n  let s = `v ${{a}}`;\n  return a * {i} + b.x;\n}}\n"
            ),
        })
        .collect()
}

/// The end of a comment about halfway through a large document, a spot where
/// appending a digit keeps the program valid.
fn comment_end(text: &str) -> usize {
    let from = text.len() / 2;
    let comment = text[from..].find("step ").unwrap() + from;
    comment + text[comment..].find('\n').unwrap()
}

#[test]
fn an_edit_in_a_large_document_reparses_only_a_small_region() {
    for grammar in [Grammar::Luau, Grammar::Rhai] {
        let text = large_document(grammar, 2_000);
        let mut highlighter = highlighter(grammar, &text);
        let mut before = highlighter.tree.clone().expect("parsed");

        let mut edited = text.clone();
        edited.insert(comment_end(&text), '9');
        before.edit(&edit::between(&text, &edited));
        highlighter.update(&edited);

        let after = highlighter.tree.as_ref().expect("parsed");
        let changed: usize = after
            .changed_ranges(&before)
            .map(|range| range.end_byte - range.start_byte)
            .sum();
        assert!(
            changed < text.len() / 100,
            "{grammar:?} changed {changed} bytes"
        );
    }
}

/// Run with `cargo test -p glaux-syntax --release -- --ignored --nocapture timings`.
#[test]
#[ignore = "measures time; run by hand"]
fn timings() {
    use std::time::Instant;
    for (grammar, functions) in [
        (Grammar::Luau, 1_000),
        (Grammar::Luau, 40_000),
        (Grammar::Rhai, 1_000),
        (Grammar::Rhai, 40_000),
    ] {
        let text = large_document(grammar, functions);
        let mut highlighter = Highlighter::new(grammar).unwrap();

        let start = Instant::now();
        highlighter.update(&text);
        let initial = start.elapsed();

        let mut edited = text.clone();
        edited.insert(comment_end(&text), '9');
        let start = Instant::now();
        let diff = edit::between(&text, &edited);
        let diffing = start.elapsed();
        let start = Instant::now();
        highlighter.update(&edited);
        let reparse = start.elapsed();
        let _ = diff;
        println!("diff alone {diffing:?}");

        let mid = edited.len() / 2;
        let start = Instant::now();
        let spans = highlighter.spans(mid..mid + 4_000);
        let window = start.elapsed();

        println!(
            "{grammar:?}: {} KiB, initial parse {initial:?}, one-char edit {reparse:?}, \
             4000-byte window ({} spans) {window:?}",
            text.len() / 1024,
            spans.len()
        );
    }
}
