//! What the documentation panel beside the suggestion list says about a
//! candidate.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "nothing asks the session for suggestions yet")
)]

use glaux_docs::{ApiCatalog, DocId, Kind, ScriptLanguage, ValueType};
use glaux_sense::{Candidate, CandidateKind, Doc};
use glaux_syntax::{Definition, DefinitionKind, SyntaxKind};

/// The text of one candidate's panel, ready to be laid out at any width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocPanel {
    /// The first line, in pieces that each take a syntax color.
    pub signature: Vec<(String, SyntaxKind)>,
    /// The page's opening paragraph, wrapped when drawn.
    pub summary: Option<String>,
    /// What each parameter takes, wrapped when drawn.
    pub params: Option<String>,
}

/// Where the panel's wording comes from. The panel does not read the
/// documentation itself, so the caller decides which language it is in.
pub struct PanelSource<'a> {
    pub language: ScriptLanguage,
    pub api: &'a ApiCatalog,
    /// The opening paragraph of the page with this id.
    pub summary: &'a dyn Fn(&DocId) -> Option<String>,
    /// The 1-based line of the document holding this byte offset.
    pub line_of: &'a dyn Fn(usize) -> usize,
}

/// The panel for `candidate`, or `None` when there is nothing to say, as
/// for keywords.
pub fn panel_for(candidate: &Candidate, source: &PanelSource<'_>) -> Option<DocPanel> {
    match &candidate.doc {
        Doc::None => None,
        Doc::Local(definition) => Some(local_panel(definition, source)),
        Doc::Api(id) => api_panel(candidate, id, source),
    }
}

fn local_panel(definition: &Definition, source: &PanelSource<'_>) -> DocPanel {
    let line = (source.line_of)(definition.declared.start);
    let (signature, what) = match definition.kind {
        DefinitionKind::Variable => (
            vec![(definition.name.clone(), SyntaxKind::Variable)],
            "Local variable",
        ),
        DefinitionKind::Parameter => (
            vec![(definition.name.clone(), SyntaxKind::Variable)],
            "Parameter",
        ),
        DefinitionKind::Function => {
            let mut signature = vec![
                (definition.name.clone(), SyntaxKind::Function),
                ("(".to_owned(), SyntaxKind::Operator),
            ];
            signature.push((definition.params.join(", "), SyntaxKind::Variable));
            signature.push((")".to_owned(), SyntaxKind::Operator));
            (signature, "Function")
        }
    };
    DocPanel {
        signature,
        summary: Some(format!("{what} declared on line {line}.")),
        params: None,
    }
}

fn api_panel(candidate: &Candidate, id: &DocId, source: &PanelSource<'_>) -> Option<DocPanel> {
    let entry = source.api.get(id)?;
    let language = source.language;
    let binding = entry.binding(language)?;
    let name = language.qualified_name(binding);
    let summary = (source.summary)(id);
    let mut panel = DocPanel {
        signature: Vec::new(),
        summary,
        params: None,
    };
    match entry.kind() {
        Kind::Function => {
            let op = |text: &str| (text.to_owned(), SyntaxKind::Operator);
            let names: Vec<String> = entry
                .params()
                .iter()
                .map(|param| {
                    if param.optional {
                        format!("{}?", param.name)
                    } else {
                        param.name.clone()
                    }
                })
                .collect();
            panel.signature = vec![
                (name, SyntaxKind::Function),
                op("("),
                (names.join(", "), SyntaxKind::Variable),
                op(")"),
            ];
            if let Some(returns) = entry.returns(language) {
                panel.signature.push(op(" : "));
                panel
                    .signature
                    .push((type_name(returns).to_owned(), SyntaxKind::Type));
            }
            if !entry.params().is_empty() {
                let typed: Vec<String> = entry
                    .params()
                    .iter()
                    .map(|param| {
                        let mark = if param.optional { "?" } else { "" };
                        format!("{}{mark}: {}", param.name, type_name(param.ty))
                    })
                    .collect();
                panel.params = Some(typed.join("  "));
            }
        }
        Kind::Namespace => panel.signature = vec![(name, SyntaxKind::Type)],
        Kind::ConstantGroup => {
            // A member is named under its group, as `Key.A` or `Key::A`.
            let member = candidate.kind == CandidateKind::Constant && candidate.label != name;
            let shown = if member {
                format!("{name}{}{}", language.path_separator(), candidate.label)
            } else {
                name
            };
            panel.signature = vec![(shown, SyntaxKind::Constant)];
        }
        Kind::Keyword => panel.signature = vec![(name, SyntaxKind::Keyword)],
        Kind::Callback => {
            let keyword = match language {
                ScriptLanguage::Luau => "function ",
                ScriptLanguage::Rhai => "fn ",
            };
            panel.signature = vec![
                (keyword.to_owned(), SyntaxKind::Keyword),
                (name, SyntaxKind::Function),
                ("()".to_owned(), SyntaxKind::Operator),
            ];
        }
    }
    Some(panel)
}

fn type_name(ty: ValueType) -> &'static str {
    match ty {
        ValueType::Number => "number",
        ValueType::Int => "int",
        ValueType::Text => "text",
        ValueType::Bool => "bool",
        ValueType::Button => "Button",
        ValueType::Key => "Key",
        ValueType::Waveform => "Waveform",
        ValueType::Any => "any",
    }
}

#[cfg(test)]
mod tests;
