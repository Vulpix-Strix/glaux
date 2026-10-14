//! Command line parsing, independent of what the commands do.
//!
//! A line is a sequence of words separated by whitespace. A word is made of
//! unquoted text and `'…'` or `"…"` segments, concatenated: `a"b c"` is the
//! single word `ab c`. Inside quotes, `\\` is a backslash and a backslash
//! before the enclosing quote is that quote. Any other backslash is kept as
//! written. Outside quotes a backslash is an ordinary character.

use std::iter::Peekable;

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Invocation<'a> {
    /// The first word, unquoted.
    pub(super) name: String,
    /// The remaining words, unquoted.
    pub(super) args: Vec<String>,
    /// Everything after the command name exactly as typed, minus leading
    /// whitespace, for commands that interpret their arguments themselves.
    pub(super) raw_args: &'a str,
    /// The command's valueless flags found among the words, by name
    /// without the leading `--`. The parser leaves this empty. Commands
    /// that take flags have them moved here out of `args`.
    pub(super) flags: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(super) enum ParseError {
    /// A quote opened at char index `start` is never closed.
    #[error("Unterminated {quote} quote")]
    UnterminatedQuote { quote: char, start: usize },
}

/// A malformed `--option` among a command's arguments.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(super) enum OptionError {
    #[error("Unknown option: {0}")]
    Unknown(String),
    #[error("Option --{0} needs a value")]
    MissingValue(&'static str),
    #[error("Option --{0} given more than once")]
    Repeated(&'static str),
}

/// Options given to a command, by name without the leading `--`.
pub(super) type Options<'a> = Vec<(&'static str, &'a str)>;

/// A positional argument among a command's options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Positional<'a> {
    pub(super) word: &'a str,
    /// Index in [`Split::options`] of the option given most recently before
    /// this word, if any. In `--name My Game`, `Game` follows `--name`. When
    /// a command gets more positional words than it takes, such words are
    /// likely the rest of a value with spaces that was not quoted.
    pub(super) after: Option<usize>,
}

/// A command's arguments separated by [`split_options`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Split<'a> {
    pub(super) positional: Vec<Positional<'a>>,
    pub(super) options: Options<'a>,
}

/// Separates `args` into positional arguments and the `known` options, each
/// written `--name value` or `--name=value`. Option names are matched
/// exactly. Every option takes a value and may be given once.
///
/// Works on unquoted words, so `--name "Space Adventure"` has the value
/// `Space Adventure`, and a word that starts with `--` is always an option,
/// quoted or not, except where it is an option's value.
pub(super) fn split_options<'a>(
    args: &'a [String],
    known: &[&'static str],
) -> Result<Split<'a>, OptionError> {
    let mut positional = Vec::new();
    let mut options: Options<'a> = Vec::new();
    let mut words = args.iter().map(String::as_str);
    while let Some(word) = words.next() {
        let Some(option) = word.strip_prefix("--") else {
            positional.push(Positional {
                word,
                after: options.len().checked_sub(1),
            });
            continue;
        };
        let (name, inline) = match option.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (option, None),
        };
        let Some(&name) = known.iter().find(|&&known| known == name) else {
            return Err(OptionError::Unknown(format!("--{name}")));
        };
        let value = inline
            .or_else(|| words.next())
            .ok_or(OptionError::MissingValue(name))?;
        if options.iter().any(|&(seen, _)| seen == name) {
            return Err(OptionError::Repeated(name));
        }
        options.push((name, value));
    }
    Ok(Split {
        positional,
        options,
    })
}

/// `word` as it must be typed to reach a command as a single argument: as
/// is when it has no whitespace or quotes, otherwise in double quotes with
/// `"` and `\` escaped.
pub(super) fn quote(word: &str) -> String {
    if !word.is_empty() && !word.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
        return word.to_owned();
    }
    let mut quoted = String::from('"');
    for c in word.chars() {
        if c == '"' || c == '\\' {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted
}

/// Parses `line` into a command invocation. Returns `None` for a line with
/// no words.
pub(super) fn parse(line: &str) -> Result<Option<Invocation<'_>>, ParseError> {
    let Some((name, name_end)) = next_word(line, 0)? else {
        return Ok(None);
    };
    let mut args = Vec::new();
    let mut pos = name_end;
    while let Some((arg, end)) = next_word(line, pos)? {
        args.push(arg);
        pos = end;
    }
    Ok(Some(Invocation {
        name,
        args,
        raw_args: line[name_end..].trim_start(),
        flags: Vec::new(),
    }))
}

/// Reads the first word at or after byte offset `from`. Returns the
/// unquoted word and the byte offset just past it.
fn next_word(line: &str, from: usize) -> Result<Option<(String, usize)>, ParseError> {
    let rest = &line[from..];
    let Some(skip) = rest.find(|c: char| !c.is_whitespace()) else {
        return Ok(None);
    };
    let start = from + skip;
    let mut chars = line[start..]
        .char_indices()
        .map(|(i, c)| (start + i, c))
        .peekable();
    let mut word = String::new();
    while let Some(&(i, c)) = chars.peek() {
        if c.is_whitespace() {
            return Ok(Some((word, i)));
        }
        chars.next();
        if c == '\'' || c == '"' {
            read_quoted(&mut chars, c, &mut word).ok_or(ParseError::UnterminatedQuote {
                quote: c,
                start: line[..i].chars().count(),
            })?;
        } else {
            word.push(c);
        }
    }
    Ok(Some((word, line.len())))
}

/// Appends the contents of a quoted segment to `word`, consuming the
/// closing `quote`. Returns `None` if the line ends first.
fn read_quoted(
    chars: &mut Peekable<impl Iterator<Item = (usize, char)>>,
    quote: char,
    word: &mut String,
) -> Option<()> {
    while let Some((_, c)) = chars.next() {
        if c == quote {
            return Some(());
        }
        if c == '\\' {
            let (_, escaped) = chars.next()?;
            if escaped != quote && escaped != '\\' {
                word.push('\\');
            }
            word.push(escaped);
        } else {
            word.push(c);
        }
    }
    None
}

#[cfg(test)]
mod tests;
