use super::*;
use pretty_assertions::assert_eq;

const PLAIN: SearchOptions = SearchOptions {
    match_case: false,
    whole_word: false,
};
const CASED: SearchOptions = SearchOptions {
    match_case: true,
    whole_word: false,
};
const WORDS: SearchOptions = SearchOptions {
    match_case: false,
    whole_word: true,
};

/// Each match as `(start, end)`.
fn spans(text: &str, query: &str, options: SearchOptions) -> Vec<(usize, usize)> {
    find_all(text, query, options)
        .into_iter()
        .map(|range| (range.start, range.end))
        .collect()
}

/// The matched text of each match.
fn found<'a>(text: &'a str, query: &str, options: SearchOptions) -> Vec<&'a str> {
    find_all(text, query, options)
        .into_iter()
        .map(|range| &text[range])
        .collect()
}

#[test]
fn case_is_ignored_unless_asked_for() {
    let text = "Speed speed SPEED";
    assert_eq!(found(text, "speed", PLAIN), ["Speed", "speed", "SPEED"]);
    assert_eq!(found(text, "speed", CASED), ["speed"]);
    assert_eq!(found("ÉCOLE école", "école", PLAIN), ["ÉCOLE", "école"]);
}

#[test]
fn matches_never_overlap() {
    assert_eq!(spans("aaaa", "aa", PLAIN), [(0, 2), (2, 4)]);
    assert_eq!(spans("aaa", "aa", PLAIN), [(0, 2)]);
}

#[test]
fn an_empty_query_matches_nothing() {
    assert!(find_all("abc", "", PLAIN).is_empty());
}

#[test]
fn matches_are_byte_ranges_on_character_boundaries() {
    let text = "日本 x 日本";
    assert_eq!(spans(text, "日本", PLAIN), [(0, 6), (9, 15)]);
    // The byte order mark is never part of the text searched.
    assert_eq!(spans("\u{feff}\u{feff}a", "\u{feff}", PLAIN), [(3, 6)]);
}

#[test]
fn whole_words_do_not_match_inside_identifiers() {
    let text = "x = max(x, player_speed) + player.x";
    assert_eq!(spans(text, "x", WORDS), [(0, 1), (8, 9), (34, 35)]);
    assert_eq!(found(text, "player", WORDS), ["player"]);
    assert_eq!(spans(text, "player", WORDS), [(27, 33)]);
    assert_eq!(found("añob año", "año", WORDS), ["año"]);
    // At the very start and end of the text.
    assert_eq!(spans("end", "end", WORDS), [(0, 3)]);
}

#[test]
fn a_rejected_whole_word_candidate_does_not_hide_a_later_match() {
    // "ab" inside "aab" is rejected, then "ab" after the space matches.
    assert_eq!(spans("aab ab", "ab", WORDS), [(4, 6)]);
}

#[test]
fn stepping_through_matches_wraps_around() {
    let matches = [2..4, 6..8, 10..12];
    assert_eq!(next_from(&matches, 0), Some(0));
    assert_eq!(next_from(&matches, 6), Some(1));
    assert_eq!(next_from(&matches, 11), Some(0));
    assert_eq!(previous_from(&matches, 12), Some(2));
    assert_eq!(previous_from(&matches, 8), Some(1));
    assert_eq!(previous_from(&matches, 3), Some(2));
    assert_eq!(next_from(&[], 0), None);
    assert_eq!(previous_from(&[], 0), None);
}
