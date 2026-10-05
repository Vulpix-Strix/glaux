//! Command-name completion.

use std::ops::Range;

/// Char range of the command name being typed, or `None` when the cursor
/// is not on it.
///
/// The cursor is on the name when it lies inside it or right after it. On
/// a blank line the name is the empty range at the cursor.
pub(super) fn command_token(chars: &[char], cursor: usize) -> Option<Range<usize>> {
    let Some(start) = chars.iter().position(|c| !c.is_whitespace()) else {
        return Some(cursor..cursor);
    };
    let end = chars[start..]
        .iter()
        .position(|c| c.is_whitespace())
        .map_or(chars.len(), |len| start + len);
    (start..=end).contains(&cursor).then_some(start..end)
}

/// Result of completing a prefix against a list of names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Completion<'a> {
    /// No name starts with the prefix.
    None,
    /// Exactly one name starts with the prefix.
    Unique(&'a str),
    /// Several names match and share a prefix longer than the one typed.
    Extend(String),
    /// Several names match and share nothing beyond the typed prefix.
    Ambiguous(Vec<&'a str>),
}

/// Completes `prefix` against `names`, ignoring case. Extensions are taken
/// from `names`, so they use the names' spelling.
pub(super) fn complete<'a>(
    prefix: &str,
    names: impl IntoIterator<Item = &'a str>,
) -> Completion<'a> {
    let prefix = prefix.to_lowercase();
    let matches: Vec<&str> = names
        .into_iter()
        .filter(|name| name.to_lowercase().starts_with(&prefix))
        .collect();
    match matches.as_slice() {
        [] => Completion::None,
        [name] => Completion::Unique(name),
        [first, rest @ ..] => {
            let common = rest.iter().fold(first.chars().count(), |len, name| {
                first
                    .chars()
                    .zip(name.chars())
                    .take(len)
                    .take_while(|(a, b)| a == b)
                    .count()
            });
            if common > prefix.chars().count() {
                Completion::Extend(first.chars().take(common).collect())
            } else {
                Completion::Ambiguous(matches)
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
}
