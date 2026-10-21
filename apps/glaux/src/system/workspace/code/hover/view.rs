//! What the popup says about a word, ready to be laid out at any width.

use glaux_docs::{ApiEntry, DocId, Kind, ScriptLanguage};
use glaux_sense::Subject;
use glaux_syntax::SyntaxKind;

use super::super::completion::{entry_signature, local_panel, type_name};
use crate::system::workspace::docs::DocTarget;

/// What one parameter takes and means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamRow {
    /// As the signature writes it, `?` marking an optional parameter.
    pub name: String,
    pub ty: &'static str,
    pub meaning: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverView {
    /// The first line, in pieces that each take a syntax color.
    pub signature: Vec<(String, SyntaxKind)>,
    /// Where the name comes from, shown at the end of the first line.
    pub tag: Option<String>,
    /// The page's opening paragraph, wrapped when drawn.
    pub summary: Option<String>,
    pub params: Vec<ParamRow>,
    pub returns: Option<&'static str>,
    /// Whether the full page can be opened from the popup.
    pub opens_page: bool,
}

/// Where the popup's wording comes from. The view does not read the
/// documentation itself, so the caller decides which language it is in.
pub struct ViewSource<'a> {
    pub language: ScriptLanguage,
    /// The opening paragraph of the page with this id.
    pub summary: &'a dyn Fn(&DocId) -> Option<String>,
    /// What the page with this id says about the parameter of this name.
    pub meaning: &'a dyn Fn(&DocId, &str) -> Option<String>,
    /// The 1-based line of the document holding this byte offset.
    pub line_of: &'a dyn Fn(usize) -> usize,
}

/// The view of `subject` and the page it opens. `None` when there is nothing to
/// say, as for a name the engine does not know.
pub fn view_for(
    subject: &Subject<'_>,
    source: &ViewSource<'_>,
) -> Option<(HoverView, Option<DocTarget>)> {
    match subject {
        Subject::Entry(entry) => Some(entry_view(entry, None, source)),
        Subject::Constant { group, member } => Some(entry_view(group, Some(member), source)),
        Subject::Local(definition) => {
            let panel = local_panel(definition, (source.line_of)(definition.declared.start));
            let view = HoverView {
                signature: panel.signature,
                tag: None,
                summary: panel.summary,
                params: Vec::new(),
                returns: None,
                opens_page: false,
            };
            Some((view, None))
        }
        Subject::Declared(_) | Subject::Unknown => None,
    }
}

fn entry_view(
    entry: &ApiEntry,
    member: Option<&str>,
    source: &ViewSource<'_>,
) -> (HoverView, Option<DocTarget>) {
    let language = source.language;
    let id = entry.id();
    let function = entry.kind() == Kind::Function;
    let params = if function {
        entry
            .params()
            .iter()
            .map(|param| ParamRow {
                name: if param.optional {
                    format!("{}?", param.name)
                } else {
                    param.name.clone()
                },
                ty: type_name(param.ty),
                meaning: (source.meaning)(id, &param.name).unwrap_or_default(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let view = HoverView {
        signature: entry_signature(entry, language, member),
        tag: Some(entry.category().to_owned()),
        summary: (source.summary)(id),
        params,
        returns: entry.returns(language).filter(|_| function).map(type_name),
        opens_page: true,
    };
    (view, Some(DocTarget::page(id.clone())))
}

#[cfg(test)]
mod tests;
