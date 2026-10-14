//! Finding text in a code document.

use std::ops::Range;

use super::code_cursor::is_word_char;

/// Most matches one search reports. Past it the rest of the text is not
/// searched, so a short query in a large file stays quick.
pub const MAX_MATCHES: usize = 10_000;

/// The byte order mark, which no search matches.
const BOM: char = '\u{feff}';

/// How a query matches text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SearchOptions {
    /// Letters must have the same case as in the query.
    pub match_case: bool,
    /// The match must not continue a word on either side: the characters
    /// around it, if any, are not word characters.
    pub whole_word: bool,
}

/// Every place `query` occurs in `text`, in order and without overlapping,
/// up to [`MAX_MATCHES`]. Nothing matches an empty query.
///
/// Without [`match_case`](SearchOptions::match_case), characters match
/// when their lowercase forms are equal, so a match always holds as many
/// characters as the query, though not always as many bytes.
pub fn find_all(text: &str, query: &str, options: SearchOptions) -> Vec<Range<usize>> {
    let mut matches = Vec::new();
    if query.is_empty() {
        return matches;
    }
    let start = if text.starts_with(BOM) {
        BOM.len_utf8()
    } else {
        0
    };
    let mut from = start;
    while from < text.len() && matches.len() < MAX_MATCHES {
        let Some(found) = match_at(text, from, query, options.match_case) else {
            from += text[from..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        if options.whole_word && !stands_alone(text, start, found.clone()) {
            from += text[from..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        from = found.end;
        matches.push(found);
    }
    matches
}

/// The match of `query` starting exactly at `from`, if there is one.
fn match_at(text: &str, from: usize, query: &str, match_case: bool) -> Option<Range<usize>> {
    if match_case {
        return text[from..]
            .starts_with(query)
            .then(|| from..from + query.len());
    }
    let mut chars = text[from..].char_indices();
    let mut end = from;
    for wanted in query.chars() {
        let (at, ch) = chars.next()?;
        if ch != wanted && !ch.to_lowercase().eq(wanted.to_lowercase()) {
            return None;
        }
        end = from + at + ch.len_utf8();
    }
    Some(from..end)
}

/// Whether `found` neither follows nor precedes a word character, looking
/// no further back than `start`.
fn stands_alone(text: &str, start: usize, found: Range<usize>) -> bool {
    let before = text[start..found.start].chars().next_back();
    let after = text[found.end..].chars().next();
    !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
}

/// The first of `matches` at or after `byte`, wrapping to the first one.
pub fn next_from(matches: &[Range<usize>], byte: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    Some(
        matches
            .iter()
            .position(|found| found.start >= byte)
            .unwrap_or(0),
    )
}

/// The last of `matches` ending at or before `byte`, wrapping to the last
/// one.
pub fn previous_from(matches: &[Range<usize>], byte: usize) -> Option<usize> {
    let last = matches.len().checked_sub(1)?;
    Some(
        matches
            .iter()
            .rposition(|found| found.end <= byte)
            .unwrap_or(last),
    )
}

#[cfg(test)]
mod tests;
