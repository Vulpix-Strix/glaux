//! Command line parsing, independent of what the commands do.
//!
//! A line is a sequence of words separated by whitespace. A word is made of
//! unquoted text and `'…'` or `"…"` segments, concatenated: `a"b c"` is the
//! single word `ab c`. Inside quotes, `\\` is a backslash and a backslash
//! before the enclosing quote is that quote; any other backslash is kept as
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
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(super) enum ParseError {
    /// A quote opened at char index `start` is never closed.
    #[error("UNTERMINATED {quote} QUOTE")]
    UnterminatedQuote { quote: char, start: usize },
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
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn words(line: &str) -> (String, Vec<String>) {
        let invocation = parse(line).unwrap().unwrap();
        (invocation.name, invocation.args)
    }

    fn owned(words: &[&str]) -> Vec<String> {
        words.iter().map(|&w| w.to_owned()).collect()
    }

    #[test]
    fn splits_words_on_any_whitespace() {
        assert_eq!(
            words("  copy\ta   b  "),
            ("copy".into(), owned(&["a", "b"]))
        );
    }

    #[test]
    fn name_without_arguments() {
        let invocation = parse("help").unwrap().unwrap();
        assert_eq!(invocation.name, "help");
        assert_eq!(invocation.args, Vec::<String>::new());
        assert_eq!(invocation.raw_args, "");
    }

    #[test]
    fn double_quotes_group_words() {
        assert_eq!(
            words(r#"new "Super Owl""#),
            ("new".into(), owned(&["Super Owl"]))
        );
    }

    #[test]
    fn single_quotes_group_words() {
        assert_eq!(words("say 'a  b' c"), ("say".into(), owned(&["a  b", "c"])));
    }

    #[test]
    fn other_quote_kind_is_literal_inside_quotes() {
        assert_eq!(
            words(r#"say "it's" 'a "b"'"#),
            ("say".into(), owned(&["it's", r#"a "b""#]))
        );
    }

    #[test]
    fn escapes_inside_quotes() {
        assert_eq!(
            words(r#"say "a\"b" 'c\'d' "e\\f" "g\nh""#),
            ("say".into(), owned(&[r#"a"b"#, "c'd", r"e\f", r"g\nh"]))
        );
    }

    #[test]
    fn backslash_outside_quotes_is_literal() {
        assert_eq!(words(r"cd a\b"), ("cd".into(), owned(&[r"a\b"])));
    }

    #[test]
    fn adjacent_segments_form_one_word() {
        assert_eq!(words(r#"say a"b c"'d'"#), ("say".into(), owned(&["ab cd"])));
    }

    #[test]
    fn empty_quotes_are_an_empty_argument() {
        assert_eq!(words(r#"say "" ''"#), ("say".into(), owned(&["", ""])));
    }

    #[test]
    fn arguments_keep_their_case() {
        assert_eq!(words("NEW Owl"), ("NEW".into(), owned(&["Owl"])));
    }

    #[test]
    fn raw_args_preserve_the_text_after_the_name() {
        let invocation = parse(r#"  eval  print("a  b") -- 'x' "#).unwrap().unwrap();
        assert_eq!(invocation.raw_args, r#"print("a  b") -- 'x' "#);
    }

    #[test]
    fn blank_line_has_no_invocation() {
        assert_eq!(parse(""), Ok(None));
        assert_eq!(parse(" \t "), Ok(None));
    }

    #[test]
    fn unterminated_quotes_are_errors() {
        assert_eq!(
            parse(r#"new "Super Owl"#),
            Err(ParseError::UnterminatedQuote {
                quote: '"',
                start: 4
            })
        );
        assert_eq!(
            parse("é 'x"),
            Err(ParseError::UnterminatedQuote {
                quote: '\'',
                start: 2
            })
        );
    }

    #[test]
    fn escaped_closing_quote_leaves_quote_open() {
        assert_eq!(
            parse(r#"say "abc\""#),
            Err(ParseError::UnterminatedQuote {
                quote: '"',
                start: 4
            })
        );
        assert_eq!(
            parse(r#"say "abc\"#),
            Err(ParseError::UnterminatedQuote {
                quote: '"',
                start: 4
            })
        );
    }

    #[test]
    fn error_message_names_the_quote() {
        let err = parse("say 'x").unwrap_err();
        assert_eq!(err.to_string(), "UNTERMINATED ' QUOTE");
    }
}
