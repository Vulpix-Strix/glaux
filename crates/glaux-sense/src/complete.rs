use std::ops::Range;

use glaux_docs::{ApiCatalog, ApiEntry, DocId, Kind, ScriptLanguage};
use glaux_syntax::{Definition, DefinitionKind, Grammar, Highlighter};

use crate::context::{Context, Position, Separator, context};
use crate::resolve::{Resolved, constant_members, resolve};

/// The most candidates a single answer holds.
pub const MAX_CANDIDATES: usize = 100;

/// How completion was requested.
///
/// The engine remembers nothing about earlier edits. The caller decides when to ask,
/// and the trigger only changes how readily the engine answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The user typed an identifier character. The engine waits for two characters and
    /// stays quiet when the only candidate is exactly what was typed.
    Auto,
    /// The user typed a member separator. The engine answers only after one.
    Member,
    /// The user asked for completion, or an open list is being narrowed. The engine
    /// answers anywhere completion makes sense, even before anything is typed.
    Manual,
}

#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub language: ScriptLanguage,
    pub text: &'a str,
    /// Byte offset of the caret in `text`.
    pub caret: usize,
    /// The document revision, returned unchanged in the answer.
    pub revision: u64,
    pub trigger: Trigger,
    /// The line ending that multi-line insertions use.
    pub line_ending: &'a str,
    /// One step of indentation for the body of inserted declarations.
    pub indent: &'a str,
}

/// The answer to a [`Request`].
///
/// The answer is plain data. The caller can compare `revision` and `anchor` with the
/// document before applying a candidate, to discard an answer that went stale.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Completions {
    pub revision: u64,
    /// The range an accepted candidate replaces.
    pub anchor: Range<usize>,
    pub prefix: String,
    /// Ordered from the best match.
    pub items: Vec<Candidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub label: String,
    pub kind: CandidateKind,
    pub origin: Origin,
    pub insertion: Insertion,
    /// A short note for the list, such as the API category.
    pub detail: String,
    pub doc: Doc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    Function,
    Variable,
    Namespace,
    Keyword,
    Constant,
    /// A function the script defines for Glaux to call, like `tick`.
    Callback,
}

/// Where a candidate comes from. Among equally good matches, earlier origins rank first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    Local,
    Api,
    Keyword,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Insertion {
    Name(String),
    /// Source that declares something, possibly over several lines.
    Declaration {
        text: String,
        /// Byte offset in `text` where the caret ends up.
        caret_in_text: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Doc {
    None,
    /// The documentation page with this id.
    Api(DocId),
    Local(Definition),
}

/// Completes the name at the caret.
///
/// `syntax` must have parsed `request.text`. If it has not, or it is for the other
/// language, the answer is empty rather than based on offsets that could be wrong.
pub fn complete(request: &Request<'_>, api: &ApiCatalog, syntax: &Highlighter) -> Completions {
    let nothing = Completions {
        revision: request.revision,
        anchor: request.caret..request.caret,
        prefix: String::new(),
        items: Vec::new(),
    };
    if !syntax.is_parsed(request.text) || syntax.grammar() != grammar(request.language) {
        return nothing;
    }
    let Some(found) = context(request.language, request.text, request.caret, syntax) else {
        return nothing;
    };
    let wanted = !matches!(found.position, Position::Suppressed)
        && match request.trigger {
            Trigger::Member => matches!(found.position, Position::Member { .. }),
            Trigger::Auto => found.prefix.chars().count() >= 2,
            Trigger::Manual => true,
        };
    let mut completions = Completions {
        anchor: found.anchor.clone(),
        prefix: found.prefix.clone(),
        ..nothing
    };
    if !wanted {
        return completions;
    }

    let mut candidates = Vec::new();
    match &found.position {
        Position::Reference {
            top_level_statement,
        } => {
            local_candidates(request, syntax, &mut candidates);
            api_candidates(request.language, api, &mut candidates);
            word_candidates(request.language, &mut candidates);
            if *top_level_statement {
                callback_candidates(request, &found, api, syntax, true, &mut candidates);
            }
        }
        Position::Member {
            receiver,
            separator,
        } => {
            member_candidates(request, api, syntax, receiver, *separator, &mut candidates);
        }
        Position::Declaration { callback_site } => {
            if *callback_site {
                callback_candidates(request, &found, api, syntax, false, &mut candidates);
            }
        }
        Position::Suppressed => {}
    }

    completions.items = rank(candidates, &found.prefix);
    let only_what_was_typed =
        completions.items.len() == 1 && completions.items[0].label == found.prefix;
    if request.trigger != Trigger::Manual && only_what_was_typed {
        completions.items.clear();
    }
    completions
}

pub(crate) fn grammar(language: ScriptLanguage) -> Grammar {
    match language {
        ScriptLanguage::Luau => Grammar::Luau,
        ScriptLanguage::Rhai => Grammar::Rhai,
    }
}

fn local_candidates(request: &Request<'_>, syntax: &Highlighter, out: &mut Vec<Candidate>) {
    for definition in syntax.definitions_at(request.caret) {
        let (kind, detail) = match definition.kind {
            DefinitionKind::Variable => (CandidateKind::Variable, "local"),
            DefinitionKind::Parameter => (CandidateKind::Variable, "param"),
            DefinitionKind::Function => (CandidateKind::Function, "function"),
        };
        out.push(Candidate {
            label: definition.name.clone(),
            kind,
            origin: Origin::Local,
            insertion: Insertion::Name(definition.name.clone()),
            detail: detail.to_owned(),
            doc: Doc::Local(definition),
        });
    }
}

fn api_candidates(language: ScriptLanguage, api: &ApiCatalog, out: &mut Vec<Candidate>) {
    for entry in api.entries_for(language) {
        let Some(binding) = entry.binding(language).filter(|b| b.path.is_empty()) else {
            continue;
        };
        if entry.kind() == Kind::Callback {
            continue;
        }
        out.push(api_candidate(entry, &binding.name, entry.category()));
    }
}

fn api_candidate(entry: &ApiEntry, name: &str, detail: &str) -> Candidate {
    let kind = match entry.kind() {
        Kind::Function => CandidateKind::Function,
        Kind::Namespace => CandidateKind::Namespace,
        Kind::ConstantGroup => CandidateKind::Constant,
        Kind::Keyword => CandidateKind::Keyword,
        Kind::Callback => CandidateKind::Callback,
    };
    Candidate {
        label: name.to_owned(),
        kind,
        origin: Origin::Api,
        insertion: Insertion::Name(name.to_owned()),
        detail: detail.to_owned(),
        doc: Doc::Api(entry.id().clone()),
    }
}

fn word_candidates(language: ScriptLanguage, out: &mut Vec<Candidate>) {
    let grammar = grammar(language);
    for (words, kind) in [
        (grammar.keywords(), CandidateKind::Keyword),
        (grammar.literals(), CandidateKind::Constant),
    ] {
        for word in words {
            out.push(Candidate {
                label: (*word).to_owned(),
                kind,
                origin: Origin::Keyword,
                insertion: Insertion::Name((*word).to_owned()),
                detail: "keyword".to_owned(),
                doc: Doc::None,
            });
        }
    }
}

/// Offers the callbacks the script has not defined yet. With `whole` the declaration
/// starts with `function` or `fn`, otherwise the user already typed it.
fn callback_candidates(
    request: &Request<'_>,
    found: &Context,
    api: &ApiCatalog,
    syntax: &Highlighter,
    whole: bool,
    out: &mut Vec<Candidate>,
) {
    let defined = syntax.global_functions();
    for entry in api.entries_for(request.language) {
        let Some(binding) = entry.binding(request.language) else {
            continue;
        };
        if entry.kind() != Kind::Callback || defined.contains(&binding.name) {
            continue;
        }
        let (text, caret_in_text) = declaration(request, &binding.name, &found.line_indent, whole);
        out.push(Candidate {
            insertion: Insertion::Declaration {
                text,
                caret_in_text,
            },
            ..api_candidate(entry, &binding.name, "callback")
        });
    }
}

/// The source of an empty callback, and the caret offset inside its body.
fn declaration(
    request: &Request<'_>,
    name: &str,
    line_indent: &str,
    whole: bool,
) -> (String, usize) {
    let (keyword, open, close) = match request.language {
        ScriptLanguage::Luau => ("function ", "()", "end"),
        ScriptLanguage::Rhai => ("fn ", "() {", "}"),
    };
    let mut text = String::new();
    if whole {
        text.push_str(keyword);
    }
    text.push_str(name);
    text.push_str(open);
    text.push_str(request.line_ending);
    text.push_str(line_indent);
    text.push_str(request.indent);
    let caret = text.len();
    text.push_str(request.line_ending);
    text.push_str(line_indent);
    text.push_str(close);
    (text, caret)
}

fn member_candidates(
    request: &Request<'_>,
    api: &ApiCatalog,
    syntax: &Highlighter,
    receiver: &[String],
    separator: Separator,
    out: &mut Vec<Candidate>,
) {
    let language = request.language;
    let expected = match language {
        ScriptLanguage::Luau => Separator::Dot,
        ScriptLanguage::Rhai => Separator::Path,
    };
    if separator != expected || receiver.is_empty() {
        return;
    }
    let path: Vec<&str> = receiver.iter().map(String::as_str).collect();
    let Some(Resolved::Entry(owner)) = resolve(api, language, syntax, request.caret, &path) else {
        return;
    };
    match owner.kind() {
        Kind::Namespace => {
            for entry in api.entries_for(language) {
                let Some(binding) = entry.binding(language) else {
                    continue;
                };
                if binding
                    .path
                    .iter()
                    .map(String::as_str)
                    .eq(path.iter().copied())
                    && entry.kind() != Kind::Callback
                {
                    out.push(api_candidate(entry, &binding.name, entry.category()));
                }
            }
        }
        Kind::ConstantGroup => {
            let group = receiver.last().map_or("", String::as_str);
            for member in constant_members(owner).unwrap_or_default() {
                out.push(Candidate {
                    label: member.to_owned(),
                    kind: CandidateKind::Constant,
                    origin: Origin::Api,
                    insertion: Insertion::Name(member.to_owned()),
                    detail: group.to_owned(),
                    doc: Doc::Api(owner.id().clone()),
                });
            }
        }
        Kind::Function | Kind::Keyword | Kind::Callback => {}
    }
}

/// Scores how well `label` matches `prefix`, where lower is better.
fn match_level(prefix: &str, label: &str) -> Option<u8> {
    if label.starts_with(prefix) {
        return Some(0);
    }
    let (prefix, label) = (prefix.to_lowercase(), label.to_lowercase());
    if label.starts_with(&prefix) {
        return Some(1);
    }
    let mut remaining = label.chars();
    prefix
        .chars()
        .all(|wanted| remaining.any(|c| c == wanted))
        .then_some(2)
}

/// Removes duplicates and non-matches, then sorts what is left.
///
/// Sources are added from most to least specific, so when two candidates share a
/// name the first one wins. That lets a local hide an API name, and an API name hide a
/// keyword. A declaration never competes with a reference of the same name. Locals
/// keep the order they came in, which puts the innermost scope first.
fn rank(candidates: Vec<Candidate>, prefix: &str) -> Vec<Candidate> {
    let mut seen = std::collections::HashSet::new();
    let mut ranked: Vec<_> = candidates
        .into_iter()
        .enumerate()
        .filter(|(_, candidate)| {
            let declares = matches!(candidate.insertion, Insertion::Declaration { .. });
            seen.insert((candidate.label.clone(), declares))
        })
        .filter_map(|(order, candidate)| {
            let level = match_level(prefix, &candidate.label)?;
            Some((level, order, candidate))
        })
        .collect();
    ranked.sort_by(|(level_a, order_a, a), (level_b, order_b, b)| {
        level_a
            .cmp(level_b)
            .then(a.origin.cmp(&b.origin))
            .then_with(|| match a.origin {
                Origin::Local => order_a.cmp(order_b),
                _ => a
                    .label
                    .to_lowercase()
                    .cmp(&b.label.to_lowercase())
                    .then_with(|| a.label.cmp(&b.label)),
            })
    });
    ranked
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(_, _, candidate)| candidate)
        .collect()
}

#[cfg(test)]
mod tests;
