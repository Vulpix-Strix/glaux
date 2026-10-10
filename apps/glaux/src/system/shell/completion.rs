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
mod tests;
