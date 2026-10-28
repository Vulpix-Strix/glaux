use glaux_docs::{ApiCatalog, ApiEntry, Kind, ScriptLanguage, ValueType};
use glaux_syntax::{Definition, DefinitionKind, Highlighter};

use crate::complete::{Doc, grammar};
use crate::lexical::{self, State};
use crate::resolve::{Resolved, resolve};

/// The name Luau gives the rest of the arguments in a parameter list.
const VARARGS: &str = "...";

#[derive(Debug, Clone, Copy)]
pub struct SignatureRequest<'a> {
    pub language: ScriptLanguage,
    pub text: &'a str,
    /// Byte offset of the caret in `text`.
    pub caret: usize,
    /// The document revision, returned unchanged in the answer.
    pub revision: u64,
}

/// What the caret needs to know about the call it is typing arguments for.
///
/// The answer is plain data. The caller can compare `revision` with the document before
/// showing it, to discard an answer that went stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHelp {
    pub revision: u64,
    /// The offset of the call's opening parenthesis.
    pub open: usize,
    /// Never empty. Several signatures mean the script defines functions with the same
    /// name and different parameter counts, which Rhai allows.
    pub signatures: Vec<Signature>,
    /// The signature that fits the arguments typed so far.
    pub active_signature: usize,
    /// The parameter of the active signature that the caret is writing, if there is one.
    pub active_param: Option<usize>,
}

impl SignatureHelp {
    pub fn signature(&self) -> &Signature {
        &self.signatures[self.active_signature]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// The function as the script writes it, such as `audio.tone` or `audio::tone`.
    pub name: String,
    pub params: Vec<ParamInfo>,
    pub returns: Option<ValueType>,
    pub doc: Doc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamInfo {
    pub name: String,
    /// Known for API functions. A script's own parameters carry no type.
    pub ty: Option<ValueType>,
    pub optional: bool,
    /// The parameter takes every remaining argument.
    pub variadic: bool,
}

impl Signature {
    /// Whether a call with `count` arguments is valid for this signature.
    fn accepts(&self, count: usize) -> bool {
        count <= self.params.len() || self.params.last().is_some_and(|last| last.variadic)
    }

    fn param_at(&self, index: usize) -> Option<usize> {
        match self.params.get(index) {
            Some(_) => Some(index),
            None => self
                .params
                .last()
                .filter(|last| last.variadic)
                .map(|_| self.params.len() - 1),
        }
    }
}

/// Describes the call the caret is typing arguments for.
///
/// `syntax` must have parsed `request.text`. If it has not, or it is for the other
/// language, the answer is empty rather than based on offsets that could be wrong.
/// There is no answer either when the callee is unknown, is a Luau method or is not a
/// function, or when the caret is in a comment.
#[hotpath::measure(label = "sense::signature_help")]
pub fn signature_help(
    request: &SignatureRequest<'_>,
    api: &ApiCatalog,
    syntax: &Highlighter,
) -> Option<SignatureHelp> {
    if !syntax.is_parsed(request.text)
        || syntax.grammar() != grammar(request.language)
        || !request.text.is_char_boundary(request.caret)
    {
        return None;
    }
    let site = syntax.call_at(request.caret)?;
    if site.method {
        return None;
    }
    // An unclosed block comment parses as code, so only the text can tell.
    if lexical::scan(request.language, &request.text[..request.caret]).state == State::Comment {
        return None;
    }
    let path: Vec<&str> = site.callee.iter().map(String::as_str).collect();
    let signatures = match resolve(api, request.language, syntax, site.open, &path)? {
        Resolved::Entry(entry) => vec![api_signature(entry, request.language)?],
        Resolved::Local(definition) => local_signatures(&definition, syntax)?,
    };
    let given = site.active.map_or(0, |active| active + 1);
    let active_signature = fitting(&signatures, given);
    let active_param = site
        .active
        .and_then(|active| signatures[active_signature].param_at(active));
    Some(SignatureHelp {
        revision: request.revision,
        open: site.open,
        signatures,
        active_signature,
        active_param,
    })
}

/// The signature with the fewest parameters that still takes `given` arguments, or the
/// one with the most when none does.
fn fitting(signatures: &[Signature], given: usize) -> usize {
    signatures
        .iter()
        .enumerate()
        .filter(|(_, signature)| signature.accepts(given))
        .min_by_key(|(_, signature)| signature.params.len())
        .or_else(|| {
            signatures
                .iter()
                .enumerate()
                .max_by_key(|(_, signature)| signature.params.len())
        })
        .map_or(0, |(index, _)| index)
}

fn api_signature(entry: &ApiEntry, language: ScriptLanguage) -> Option<Signature> {
    if entry.kind() != Kind::Function {
        return None;
    }
    let binding = entry.binding(language)?;
    Some(Signature {
        name: language.qualified_name(binding),
        params: entry
            .params_for(language)
            .iter()
            .map(|param| ParamInfo {
                name: param.name.clone(),
                ty: Some(param.ty),
                optional: param.optional,
                variadic: false,
            })
            .chain(entry.varargs(language).map(|ty| ParamInfo {
                name: VARARGS.to_owned(),
                ty: Some(ty),
                optional: false,
                variadic: true,
            }))
            .collect(),
        returns: entry.returns(language),
        doc: Doc::Api(entry.id().clone()),
    })
}

/// The signatures of a function the script defines, one per overload.
fn local_signatures(definition: &Definition, syntax: &Highlighter) -> Option<Vec<Signature>> {
    if definition.kind != DefinitionKind::Function {
        return None;
    }
    let mut overloads = syntax.overloads(&definition.name);
    if overloads.is_empty() {
        overloads.push(definition.clone());
    }
    Some(overloads.into_iter().map(local_signature).collect())
}

fn local_signature(definition: Definition) -> Signature {
    Signature {
        name: definition.name.clone(),
        params: definition
            .params
            .iter()
            .map(|name| ParamInfo {
                name: name.clone(),
                ty: None,
                optional: false,
                variadic: name == VARARGS,
            })
            .collect(),
        returns: None,
        doc: Doc::Local(definition),
    }
}

#[cfg(test)]
mod tests;
