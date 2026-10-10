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
