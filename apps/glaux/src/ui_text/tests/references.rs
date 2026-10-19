use std::collections::BTreeSet;
use std::path::Path;

use glaux_i18n::{Catalog, Language};

use super::super::ENGLISH;

#[derive(Debug, PartialEq, Eq)]
enum Token<'a> {
    Word(&'a str),
    Text(&'a str),
    Mark(u8),
}

// Only Rust tokens needed to find literal UI lookups. Comments, strings and
// character literals cannot accidentally introduce a lookup of their own.
fn tokens(source: &str) -> Vec<Token<'_>> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at += source[at..].find('\n').unwrap_or(bytes.len() - at);
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at += 2;
                let mut depth = 1;
                while at + 1 < bytes.len() && depth > 0 {
                    match &bytes[at..at + 2] {
                        b"/*" => {
                            depth += 1;
                            at += 2;
                        }
                        b"*/" => {
                            depth -= 1;
                            at += 2;
                        }
                        _ => at += 1,
                    }
                }
            }
            b'\'' if bytes.get(at + 2) == Some(&b'\'') => at += 3,
            b'\'' if bytes.get(at + 1) == Some(&b'\\') => {
                at += 2;
                while at < bytes.len() && bytes[at] != b'\'' {
                    at += 1;
                }
                at += usize::from(at < bytes.len());
            }
            b'"' => {
                at += 1;
                let start = at;
                while at < bytes.len() && bytes[at] != b'"' {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
                if at <= bytes.len() {
                    out.push(Token::Text(&source[start..at]));
                }
                at += usize::from(at < bytes.len());
            }
            ch if ch.is_ascii_alphabetic() || ch == b'_' => {
                let start = at;
                at += 1;
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
                out.push(Token::Word(&source[start..at]));
            }
            ch if ch.is_ascii_whitespace() => at += 1,
            ch => {
                out.push(Token::Mark(ch));
                at += 1;
            }
        }
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
struct Reference<'a> {
    id: &'a str,
    variables: BTreeSet<&'a str>,
}

fn references(source: &str) -> Vec<Reference<'_>> {
    use Token::{Mark, Text, Word};
    let tokens = tokens(source);
    let mut out = Vec::new();
    for at in 0..tokens.len() {
        let rest = &tokens[at..];
        let lookup = match rest {
            [
                Word("ui_text"),
                Mark(b':'),
                Mark(b':'),
                Word(method @ ("get" | "get_with")),
                Mark(b'('),
                Text(id),
                tail @ ..,
            ] => Some((*method, *id, tail)),
            [
                Word("text"),
                Mark(b'.'),
                Word("get"),
                Mark(b'('),
                Text(id),
                tail @ ..,
            ] => Some(("get", *id, tail)),
            _ => None,
        };
        if let Some((method, id, tail)) = lookup {
            let mut variables = BTreeSet::new();
            if method == "get_with" {
                let start = tail.iter().position(|token| *token == Mark(b'[')).unwrap();
                let mut parens = 0;
                let mut brackets = 1;
                for (index, token) in tail.iter().enumerate().skip(start + 1) {
                    match token {
                        Mark(b'[') => brackets += 1,
                        Mark(b']') => {
                            brackets -= 1;
                            if brackets == 0 {
                                break;
                            }
                        }
                        Mark(b'(') => parens += 1,
                        Mark(b')') => parens -= 1,
                        Text(name)
                            if brackets == 1
                                && parens == 1
                                && tail.get(index - 1) == Some(&Mark(b'('))
                                && tail.get(index + 1) == Some(&Mark(b',')) =>
                        {
                            variables.insert(*name);
                        }
                        _ => {}
                    }
                }
            }
            out.push(Reference { id, variables });
        }
        if let [Word("description_id"), Mark(b':'), Text(id), ..] = rest {
            out.push(Reference {
                id,
                variables: BTreeSet::new(),
            });
        }
        // Tab names are selected by this method, rather than a literal lookup.
        if let [Word("fn"), Word("message_id"), ..] = rest {
            let start = rest.iter().position(|token| *token == Mark(b'{')).unwrap();
            let mut braces = 0;
            for token in &rest[start..] {
                match token {
                    Mark(b'{') => braces += 1,
                    Mark(b'}') => {
                        braces -= 1;
                        if braces == 0 {
                            break;
                        }
                    }
                    Text(id) => out.push(Reference {
                        id,
                        variables: BTreeSet::new(),
                    }),
                    _ => {}
                }
            }
        }
    }
    out
}

fn issues(catalog: &Catalog, source: &str) -> Vec<String> {
    references(source)
        .into_iter()
        .filter_map(|reference| {
            let Some(expected) = catalog.variables(reference.id) else {
                return Some(format!("missing message {}", reference.id));
            };
            let expected: BTreeSet<_> = expected.iter().map(String::as_str).collect();
            (expected != reference.variables).then(|| {
                format!(
                    "{}: expected {expected:?}, found {:?}",
                    reference.id, reference.variables
                )
            })
        })
        .collect()
}

fn check_sources(path: &Path, catalog: &Catalog, report: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.file_stem().unwrap() == "tests" {
            continue;
        }
        if path.is_dir() {
            check_sources(&path, catalog, report);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = std::fs::read_to_string(&path).unwrap();
            report.extend(
                issues(catalog, &source)
                    .into_iter()
                    .map(|issue| format!("{}: {issue}", path.display())),
            );
        }
    }
}

#[test]
fn ui_catalog_consistency_every_literal_lookup_and_its_variables_exist() {
    let catalog = Catalog::parse(Language::english(), ENGLISH).unwrap();
    let mut report = Vec::new();
    check_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &catalog,
        &mut report,
    );
    assert!(report.is_empty(), "{}", report.join("\n"));
}

#[test]
fn missing_keys_and_wrong_variables_are_reported() {
    let catalog = Catalog::parse(Language::english(), "hello = Hello { $name }\n").unwrap();
    assert_eq!(
        issues(&catalog, r#"ui_text::get("missing")"#),
        ["missing message missing"]
    );
    assert_eq!(
        issues(
            &catalog,
            r#"ui_text::get_with("hello", &[("wrong", Arg::Text("Ada"))])"#
        ),
        ["hello: expected {\"name\"}, found {\"wrong\"}"]
    );
    assert!(
        issues(
            &catalog,
            r#"ui_text::get_with("hello", &[("name", Arg::Text(&format!("{}", value)))])"#
        )
        .is_empty()
    );
    assert!(
        references(
            r#"// ui_text::get("ignored")
        let text = "ui_text::get(\"also-ignored\")";
        /* ui_text::get("ignored-again") */"#
        )
        .is_empty()
    );
}
