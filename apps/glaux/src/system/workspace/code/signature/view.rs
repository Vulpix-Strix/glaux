//! What the popup says about the call being typed, ready to be laid out at
//! any width.

use glaux_docs::DocId;
use glaux_sense::{Doc, ParamInfo, SignatureHelp};

use super::super::completion::type_name;

/// The parameter the caret is writing, as its name and, when it has one,
/// its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub name: String,
    pub ty: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureView {
    /// The function as the script writes it.
    pub name: String,
    /// Each parameter as the signature writes it, `?` marking an optional
    /// one.
    pub params: Vec<String>,
    pub returns: Option<&'static str>,
    /// The parameter the caret is writing.
    pub active: Option<usize>,
    /// Which of several signatures this is, as its position and the count.
    pub overload: Option<(usize, usize)>,
    pub head: Option<Head>,
    /// What the documentation says about the parameter, or where a
    /// function of the script is declared. Wrapped when drawn.
    pub note: Option<String>,
}

/// Where the popup's wording comes from. The view does not read the
/// documentation itself, so the caller decides which language it is in.
pub struct ViewSource<'a> {
    /// What the page with this id says about the parameter of this name.
    pub meaning: &'a dyn Fn(&DocId, &str) -> Option<String>,
    /// The 1-based line of the document holding this byte offset.
    pub line_of: &'a dyn Fn(usize) -> usize,
}

/// The view of the signature `help` picked.
pub fn view_for(help: &SignatureHelp, source: &ViewSource<'_>) -> SignatureView {
    let signature = help.signature();
    let active = help.active_param.and_then(|at| signature.params.get(at));
    let note = match &signature.doc {
        Doc::Api(id) => active.and_then(|param| (source.meaning)(id, &param.name)),
        Doc::Local(definition) => Some(format!(
            "Declared on line {}.",
            (source.line_of)(definition.declared.start)
        )),
        Doc::None => None,
    };
    SignatureView {
        name: signature.name.clone(),
        params: signature.params.iter().map(written).collect(),
        returns: signature.returns.map(type_name),
        active: help.active_param,
        overload: (help.signatures.len() > 1)
            .then_some((help.active_signature + 1, help.signatures.len())),
        head: active.map(|param| Head {
            name: written(param),
            ty: param.ty.map(type_name),
        }),
        note,
    }
}

fn written(param: &ParamInfo) -> String {
    if param.optional {
        format!("{}?", param.name)
    } else {
        param.name.clone()
    }
}

#[cfg(test)]
mod tests;
