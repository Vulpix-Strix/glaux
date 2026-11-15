use glaux_docs::{ApiCatalog, ApiEntry, ScriptLanguage};
use glaux_input::{Button, Key};
use glaux_syntax::{Definition, Highlighter};

use crate::context::INDEX;

/// What a name in a script refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved<'a> {
    Entry(&'a ApiEntry),
    Local(Definition),
}

/// Finds what `path` refers to at `offset`, for example `["audio", "tone"]`.
///
/// A local hides an API name of the same name. A path that goes through a local, such
/// as `audio.tone` after `local audio = {}`, resolves to nothing, because the members
/// of user values are unknown.
pub fn resolve<'a>(
    api: &'a ApiCatalog,
    language: ScriptLanguage,
    syntax: &Highlighter,
    offset: usize,
    path: &[&str],
) -> Option<Resolved<'a>> {
    let (name, outer) = path.split_last()?;
    let first = path.first()?;
    let local = syntax
        .definitions_at(offset)
        .into_iter()
        .find(|definition| definition.name == *first);
    if let Some(local) = local {
        return outer.is_empty().then_some(Resolved::Local(local));
    }
    api.lookup(language, outer, name).map(Resolved::Entry)
}

/// The member names of a constant group such as `Key` or `hardware.oam`.
///
/// Opaque groups name the Rust item that defines their members, so this maps that
/// source to the list. Other groups list their constants in the catalog. It returns
/// `None` for entries that are not constant groups.
pub fn constant_members(entry: &ApiEntry) -> Option<Vec<&str>> {
    if let Some(constants) = entry.constants() {
        return Some(constants.names.iter().map(String::as_str).collect());
    }
    match entry.source()? {
        "glaux_input::Key" => Some(Key::ALL.iter().map(|key| key.name()).collect()),
        "glaux_input::Button" => Some(Button::ALL.iter().map(|button| button.name()).collect()),
        _ => None,
    }
}

/// The constant group a receiver such as `hardware.oam` or `hardware.bg[0]` names,
/// with the members it offers. An index offers the members of the group's
/// indexed tables.
pub fn group_members<'a>(
    api: &'a ApiCatalog,
    language: ScriptLanguage,
    syntax: &Highlighter,
    offset: usize,
    receiver: &[&str],
) -> Option<(&'a ApiEntry, Vec<&'a str>)> {
    match receiver.split_last()? {
        (&INDEX, owner) => {
            let Some(Resolved::Entry(group)) = resolve(api, language, syntax, offset, owner) else {
                return None;
            };
            let names = &group.elements()?.constants.names;
            Some((group, names.iter().map(String::as_str).collect()))
        }
        _ => {
            let Some(Resolved::Entry(group)) = resolve(api, language, syntax, offset, receiver)
            else {
                return None;
            };
            Some((group, constant_members(group)?))
        }
    }
}

#[cfg(test)]
mod tests;
