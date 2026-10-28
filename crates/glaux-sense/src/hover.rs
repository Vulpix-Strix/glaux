use std::ops::Range;

use glaux_docs::{ApiCatalog, ApiEntry, ScriptLanguage};
use glaux_syntax::{Definition, Highlighter};

use crate::context::{Position, context};
use crate::resolve::{Resolved, constant_members, resolve};

/// What the word under a pointer names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject<'a> {
    /// A name of the script API.
    Entry(&'a ApiEntry),
    /// A member of a constant group, such as `A` in `Key.A`. The members have no pages
    /// of their own, so the group stands for them.
    Constant {
        group: &'a ApiEntry,
        member: &'static str,
    },
    /// Something the script defines.
    Local(Definition),
    /// A name being declared, such as `x` in `local x`.
    Declared(String),
    /// A word that names nothing the engine knows.
    Unknown,
}

/// The word at an offset and what it refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover<'a> {
    /// The whole word, which is where a popup anchors.
    pub range: Range<usize>,
    pub subject: Subject<'a>,
}

/// Finds what the word around `offset` refers to.
///
/// Any offset in or next to a word works, not only a caret. `syntax` must have parsed
/// `text`. Returns `None` where there is no word to ask about, as in a comment, a
/// string or a number.
#[hotpath::measure(label = "sense::hover")]
pub fn hover<'a>(
    api: &'a ApiCatalog,
    language: ScriptLanguage,
    text: &str,
    offset: usize,
    syntax: &Highlighter,
) -> Option<Hover<'a>> {
    let context = context(language, text, offset, syntax)?;
    let range = context.anchor;
    let word = &text[range.clone()];
    if word.is_empty() {
        return None;
    }
    let mut path: Vec<&str> = match &context.position {
        Position::Member { receiver, .. } => receiver.iter().map(String::as_str).collect(),
        Position::Reference { .. } => Vec::new(),
        // The scope of a name starts after its declaration, so the tree has nothing
        // to say about it yet.
        Position::Declaration { .. } => {
            return Some(Hover {
                range,
                subject: Subject::Declared(word.to_owned()),
            });
        }
        Position::Suppressed => return None,
    };
    path.push(word);
    let subject = match resolve(api, language, syntax, offset, &path) {
        Some(Resolved::Entry(entry)) => Subject::Entry(entry),
        Some(Resolved::Local(definition)) => Subject::Local(definition),
        None => constant(api, language, syntax, offset, &path).unwrap_or(Subject::Unknown),
    };
    Some(Hover { range, subject })
}

/// Recognizes `Key.A` and `Key::A`, which `resolve` cannot find because the members
/// are not catalog entries.
fn constant<'a>(
    api: &'a ApiCatalog,
    language: ScriptLanguage,
    syntax: &Highlighter,
    offset: usize,
    path: &[&str],
) -> Option<Subject<'a>> {
    let (member, owner) = path.split_last()?;
    let Some(Resolved::Entry(group)) = resolve(api, language, syntax, offset, owner) else {
        return None;
    };
    let member = constant_members(group)?
        .into_iter()
        .find(|name| name == member)?;
    Some(Subject::Constant { group, member })
}

#[cfg(test)]
mod tests;
