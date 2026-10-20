use pretty_assertions::assert_eq;

use super::*;

fn texts(lines: &[Line]) -> Vec<&str> {
    lines.iter().map(|line| line.text.as_str()).collect()
}

#[test]
fn short_lines_and_blank_lines_are_kept_as_they_are() {
    let lines = layout("one\n\n  two\n", Mode::Wrap, 20);
    assert_eq!(texts(&lines), ["one", "", "  two"]);
    assert!(lines.iter().all(|line| !line.continued));
}

#[test]
fn a_long_line_wraps_at_a_space_and_marks_the_continuation() {
    let lines = layout("aaa bbb ccc ddd", Mode::Wrap, 8);
    assert_eq!(texts(&lines), ["aaa bbb", "ccc ddd"]);
    assert_eq!(
        lines.iter().map(|line| line.continued).collect::<Vec<_>>(),
        [false, true]
    );
}

#[test]
fn continuations_keep_the_indent_of_their_line() {
    let lines = layout("    aaa bbb ccc", Mode::Wrap, 12);
    assert_eq!(texts(&lines), ["    aaa bbb", "    ccc"]);
}

#[test]
fn a_word_longer_than_a_line_is_split_in_it() {
    assert_eq!(
        texts(&layout("abcdefghij", Mode::Wrap, 4)),
        ["abcd", "efgh", "ij"]
    );
}

#[test]
fn tabs_advance_to_the_next_stop() {
    assert_eq!(texts(&layout("a\tb", Mode::Wrap, 20)), ["a       b"]);
}

#[test]
fn wrapping_loses_no_characters_apart_from_the_spaces_it_breaks_at() {
    let source = "The quick brown fox jumps over the lazy dog and keeps running";
    let joined: String = layout(source, Mode::Wrap, 13)
        .iter()
        .map(|line| line.text.trim())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(joined, source);
}

#[test]
fn reflowing_joins_a_paragraph_and_keeps_the_gaps_between_paragraphs() {
    let lines = layout("aa\nbb\n\ncc\ndd", Mode::Reflow, 20);
    assert_eq!(texts(&lines), ["aa bb", "", "cc dd"]);
}

#[test]
fn reflowing_keeps_the_indent_of_the_first_line() {
    let lines = layout("  aa\n  bb", Mode::Reflow, 20);
    assert_eq!(texts(&lines), ["  aa bb"]);
}
