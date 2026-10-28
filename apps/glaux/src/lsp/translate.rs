//! From the analyzer's diagnostics to the editor's.

use std::ops::Range;

use glaux_lsp::protocol::{Diagnostic, Position, Severity as Lsp};

use crate::system::{EditorDiagnostic, Severity};

/// The editor findings for `diagnostics` computed for `text`.
///
/// `callbacks` are the functions the host calls itself, such as `tick`. The
/// analyzer sees them defined and never called and says so, which is wrong
/// for them and only for them, so that one lint is dropped for those names.
pub(super) fn to_editor(
    text: &str,
    diagnostics: &[Diagnostic],
    callbacks: &[String],
) -> Vec<EditorDiagnostic> {
    diagnostics
        .iter()
        .filter(|d| !is_unused_callback(&d.message, callbacks))
        .map(|d| {
            let (code, message) = split_rule(d);
            EditorDiagnostic {
                range: byte_range(text, d.range.start, d.range.end),
                severity: match d.severity {
                    Some(Lsp::Warning) => Severity::Warning,
                    Some(Lsp::Information | Lsp::Hint) => Severity::Info,
                    Some(Lsp::Error) | None => Severity::Error,
                },
                code,
                message,
            }
        })
        .collect()
}

/// `FunctionUnused: Function 'tick' is never used; prefix with '_' ...`
fn is_unused_callback(message: &str, callbacks: &[String]) -> bool {
    let Some(rest) = message.strip_prefix("FunctionUnused: Function '") else {
        return false;
    };
    rest.split_once('\'')
        .is_some_and(|(name, _)| callbacks.iter().any(|callback| callback == name))
}

/// luau-lsp writes the rule as a prefix of the message, `TypeError: ...`.
fn split_rule(diagnostic: &Diagnostic) -> (Option<String>, String) {
    if let Some((rule, rest)) = diagnostic.message.split_once(": ")
        && !rule.is_empty()
        && rule.chars().all(|c| c.is_ascii_alphabetic())
    {
        return (Some(rule.to_owned()), rest.to_owned());
    }
    (diagnostic.code.clone(), diagnostic.message.clone())
}

/// The bytes of `text` between two LSP positions.
///
/// Columns count UTF-16 units and a column past the end of its line stops at
/// the end. An empty range grows by one character so it can be drawn.
fn byte_range(text: &str, start: Position, end: Position) -> Range<usize> {
    let from = offset(text, start);
    let mut to = offset(text, end).max(from);
    if to == from {
        to = text[from..]
            .chars()
            .next()
            .filter(|&c| c != '\n' && c != '\r')
            .map_or(from, |c| from + c.len_utf8());
    }
    from..to
}

fn offset(text: &str, position: Position) -> usize {
    let mut line_start = 0;
    for _ in 0..position.line {
        match text[line_start..].find('\n') {
            Some(newline) => line_start += newline + 1,
            None => return text.len(),
        }
    }
    let line = &text[line_start..];
    let line_end = line.find(['\n', '\r']).unwrap_or(line.len());
    let mut units = 0;
    for (at, c) in line[..line_end].char_indices() {
        if units >= position.character as usize {
            return line_start + at;
        }
        units += c.len_utf16();
    }
    line_start + line_end
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn at(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn diagnostic(message: &str, severity: Option<Lsp>) -> Diagnostic {
        Diagnostic {
            range: glaux_lsp::protocol::Range {
                start: at(0, 0),
                end: at(0, 1),
            },
            severity,
            code: Some("1000".into()),
            message: message.into(),
            source: None,
        }
    }

    #[test]
    fn columns_count_utf16_units_not_bytes() {
        let text = "a😀b\nñx";
        // The emoji is two UTF-16 units and four bytes.
        assert_eq!(byte_range(text, at(0, 3), at(0, 4)), 5..6);
        assert_eq!(&text[byte_range(text, at(1, 1), at(1, 2))], "x");
    }

    #[test]
    fn positions_past_the_end_are_clamped() {
        let text = "ab\ncd";
        assert_eq!(byte_range(text, at(0, 99), at(0, 99)), 2..2);
        assert_eq!(byte_range(text, at(9, 0), at(9, 3)), 5..5);
        assert_eq!(offset("ab\r\ncd", at(1, 1)), 5);
        assert_eq!(offset("ab\r\ncd", at(0, 99)), 2);
    }

    #[test]
    fn an_empty_range_covers_one_character_unless_at_a_line_end() {
        let text = "xy\nz";
        assert_eq!(byte_range(text, at(0, 0), at(0, 0)), 0..1);
        assert_eq!(byte_range(text, at(0, 2), at(0, 2)), 2..2);
    }

    #[test]
    fn the_rule_prefix_moves_to_the_code() {
        let found = to_editor(
            "x",
            &[diagnostic("TypeError: Unknown require: a", None)],
            &[],
        );
        assert_eq!(found[0].code.as_deref(), Some("TypeError"));
        assert_eq!(found[0].message, "Unknown require: a");
        assert_eq!(found[0].severity, Severity::Error);
        let plain = to_editor("x", &[diagnostic("just text", Some(Lsp::Warning))], &[]);
        assert_eq!(plain[0].code.as_deref(), Some("1000"));
        assert_eq!(plain[0].severity, Severity::Warning);
    }

    #[test]
    fn only_unused_callbacks_are_dropped() {
        let callbacks = ["tick".to_owned(), "draw".to_owned()];
        let messages = [
            "FunctionUnused: Function 'tick' is never used; prefix with '_' to silence",
            "FunctionUnused: Function 'helper' is never used; prefix with '_' to silence",
            "LocalUnused: Variable 'tick' is never used; prefix with '_' to silence",
            "TypeError: Key 'run' not found in table 'game'",
        ];
        let all: Vec<_> = messages
            .iter()
            .map(|m| diagnostic(m, Some(Lsp::Warning)))
            .collect();
        let kept: Vec<_> = to_editor("x", &all, &callbacks)
            .into_iter()
            .map(|d| d.message)
            .collect();
        assert_eq!(kept.len(), 3);
        assert!(kept[0].contains("helper"));
    }
}
