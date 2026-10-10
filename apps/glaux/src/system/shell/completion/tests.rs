use super::*;
use pretty_assertions::assert_eq;

fn token(line: &str, cursor: usize) -> Option<Range<usize>> {
    let chars: Vec<char> = line.chars().collect();
    command_token(&chars, cursor)
}

const NAMES: [&str; 4] = ["help", "hello", "helium", "info"];

#[test]
fn token_is_found_while_cursor_is_on_the_name() {
    assert_eq!(token("hel", 3), Some(0..3));
    assert_eq!(token("help foo", 2), Some(0..4));
    assert_eq!(token("help foo", 4), Some(0..4));
    assert_eq!(token("  he", 4), Some(2..4));
}

#[test]
fn no_token_once_the_cursor_leaves_the_name() {
    assert_eq!(token("help ", 5), None);
    assert_eq!(token("help foo", 7), None);
    assert_eq!(token("  he", 1), None);
}

#[test]
fn blank_line_token_is_empty_at_cursor() {
    assert_eq!(token("", 0), Some(0..0));
    assert_eq!(token("   ", 2), Some(2..2));
}

#[test]
fn unique_match() {
    assert_eq!(complete("in", NAMES), Completion::Unique("info"));
    assert_eq!(complete("HELI", NAMES), Completion::Unique("helium"));
    assert_eq!(complete("info", NAMES), Completion::Unique("info"));
}

#[test]
fn several_matches_extend_to_common_prefix() {
    assert_eq!(complete("h", NAMES), Completion::Extend("hel".into()));
    assert_eq!(complete("HE", NAMES), Completion::Extend("hel".into()));
}

#[test]
fn several_matches_without_more_in_common_are_ambiguous() {
    assert_eq!(
        complete("hel", NAMES),
        Completion::Ambiguous(vec!["help", "hello", "helium"])
    );
    assert_eq!(complete("", NAMES), Completion::Ambiguous(NAMES.to_vec()));
}

#[test]
fn exact_name_that_prefixes_others_is_ambiguous() {
    assert_eq!(
        complete("help", ["help", "helper"]),
        Completion::Ambiguous(vec!["help", "helper"])
    );
}

#[test]
fn no_match() {
    assert_eq!(complete("x", NAMES), Completion::None);
}
