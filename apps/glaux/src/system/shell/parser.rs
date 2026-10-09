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
    /// The command's valueless flags found among the words, by name
    /// without the leading `--`. The parser leaves this empty; commands
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
    /// this word, if any. In `--name My Game`, `Game` follows `--name`: when
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
/// exactly; every option takes a value and may be given once.
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

    /// The positional words and options of `args`.
    fn split(args: &[String]) -> Result<(Vec<&str>, Options<'_>), OptionError> {
        let split = split_options(args, &["name", "version"])?;
        let words = split.positional.iter().map(|p| p.word).collect();
        Ok((words, split.options))
    }

    #[test]
    fn options_take_the_next_word_or_an_inline_value() {
        let args = parse(r#"new owl --name "Super Owl" --version=1.2.0"#)
            .unwrap()
            .unwrap()
            .args;
        assert_eq!(
            split(&args),
            Ok((
                vec!["owl"],
                vec![("name", "Super Owl"), ("version", "1.2.0")]
            ))
        );
        let args = owned(&["--name", "--odd", "x", "--version="]);
        assert_eq!(
            split(&args),
            Ok((vec!["x"], vec![("name", "--odd"), ("version", "")]))
        );
    }

    #[test]
    fn positional_words_know_the_option_before_them() {
        let args = owned(&["id", "--name", "My", "Game", "--version", "1.0.0", "x"]);
        let after: Vec<_> = split_options(&args, &["name", "version"])
            .unwrap()
            .positional
            .iter()
            .map(|p| (p.word, p.after))
            .collect();
        assert_eq!(after, [("id", None), ("Game", Some(0)), ("x", Some(1))]);
    }

    #[test]
    fn quoted_words_parse_back_to_themselves() {
        for word in [
            "owl",
            "1.2.0",
            "--odd",
            r"a\b",
            "My Game",
            "",
            "it's",
            r#"say "hi""#,
            r"c:\a b",
        ] {
            let line = format!("cmd {}", quote(word));
            assert_eq!(parse(&line).unwrap().unwrap().args, [word], "{line}");
        }
        assert_eq!(quote("owl"), "owl");
        assert_eq!(quote("My Game"), r#""My Game""#);
    }

    #[test]
    fn malformed_options_are_errors() {
        for (args, error) in [
            (
                &["--author", "x"][..],
                OptionError::Unknown("--author".into()),
            ),
            (&["--Name", "x"], OptionError::Unknown("--Name".into())),
            (&["a", "--name"], OptionError::MissingValue("name")),
            (&["--name", "a", "--name=b"], OptionError::Repeated("name")),
        ] {
            assert_eq!(split(&owned(args)), Err(error.clone()), "{args:?}");
        }
        assert_eq!(
            OptionError::MissingValue("name").to_string(),
            "Option --name needs a value"
        );
    }

    #[test]
    fn error_message_names_the_quote() {
        let err = parse("say 'x").unwrap_err();
        assert_eq!(err.to_string(), "Unterminated ' quote");
    }
}
