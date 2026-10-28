//! The few LSP types and message builders the client needs.
//!
//! Only what diagnostics use is modelled. Positions are LSP positions: zero
//! based lines and UTF-16 code unit columns.

use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

impl<'de> Deserialize<'de> for Severity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match u8::deserialize(deserializer)? {
            1 => Self::Error,
            2 => Self::Warning,
            3 => Self::Information,
            _ => Self::Hint,
        })
    }
}

/// A diagnostic as a server reports it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    /// Servers may omit it. Clients treat a missing severity as an error.
    pub severity: Option<Severity>,
    #[serde(default, deserialize_with = "code_as_text")]
    pub code: Option<String>,
    pub message: String,
    pub source: Option<String>,
}

fn code_as_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<Value>::deserialize(deserializer)? {
        Some(Value::String(text)) => Some(text),
        Some(Value::Number(number)) => Some(number.to_string()),
        _ => None,
    })
}

/// Reads the `items` of a `textDocument/diagnostic` result or the
/// `diagnostics` of a `publishDiagnostics`. Entries that do not parse are
/// skipped so one odd diagnostic does not hide the rest.
pub(crate) fn parse_diagnostics(items: Option<&Value>) -> Vec<Diagnostic> {
    items
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn request(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

pub(crate) fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": method, "params": params})
}

pub(crate) fn response(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

pub(crate) fn method_not_found(id: &Value, method: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": -32601, "message": format!("method not found: {method}")},
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn diagnostics_parse_with_numeric_or_text_codes() {
        let items = json!([
            {"range": {"start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 5}},
             "severity": 2, "code": 1000, "message": "a", "source": "Luau"},
            {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
             "code": "LocalUnused", "message": "b"},
        ]);
        let parsed = parse_diagnostics(Some(&items));
        assert_eq!(parsed[0].severity, Some(Severity::Warning));
        assert_eq!(parsed[0].code.as_deref(), Some("1000"));
        assert_eq!(parsed[1].severity, None);
        assert_eq!(parsed[1].code.as_deref(), Some("LocalUnused"));
    }

    #[test]
    fn an_unparseable_entry_does_not_hide_the_others() {
        let items = json!([
            {"message": "no range"},
            {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
             "message": "ok"},
        ]);
        assert_eq!(parse_diagnostics(Some(&items)).len(), 1);
        assert_eq!(parse_diagnostics(None), vec![]);
    }
}
