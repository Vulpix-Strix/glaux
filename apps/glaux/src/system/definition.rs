//! Finds where a name in a script is defined, across the cartridge's scripts.
//!
//! GlauxSense says what a name refers to inside one document, and which module and
//! member to look for when it goes through an import. This turns that into a place:
//! it finds the module's file, preferring the open document so unsaved edits count, and
//! asks the module what it exports. Anything that cannot be proven leads nowhere.

use glaux_modules::ModuleName;
use glaux_sense::Resolution;
use glaux_syntax::{Exports, Grammar, Highlighter};

use super::code_document::DocumentId;
use super::code_session::{CodeSession, Destination, Spot};
use crate::cartridge::source::SourcePath;
use crate::cartridge::{Language, LoadedCartridge};

/// What asking for a definition came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Definition {
    Found(Destination),
    /// Several declarations fit, so none is chosen.
    Ambiguous,
    /// The name is not something the scripts define, or the analysis cannot follow it.
    Unresolved,
    /// The named module has no file in the cartridge.
    ModuleMissing(String),
}

/// Resolves the name at `caret` in document `id`.
pub fn locate(
    code: &mut CodeSession,
    cartridge: &LoadedCartridge,
    id: DocumentId,
    caret: usize,
) -> Definition {
    let Some(resolution) = code.resolve_at(id, caret) else {
        return Definition::Unresolved;
    };
    let Some(document) = code.document(id) else {
        return Definition::Unresolved;
    };
    let (path, language) = (document.path().clone(), document.language());
    match resolution {
        Resolution::Local(found) => Definition::Found(Destination {
            path,
            at: Spot::Offset(found.declared.start),
        }),
        Resolution::Ambiguous => Definition::Ambiguous,
        Resolution::Unresolved => Definition::Unresolved,
        Resolution::Import { module } => match module_file(cartridge, code, &module, language) {
            Some((path, _)) => Definition::Found(Destination {
                path,
                at: Spot::Offset(0),
            }),
            None => Definition::ModuleMissing(module),
        },
        Resolution::Member { module, member } => {
            let Some((path, text)) = module_file(cartridge, code, &module, language) else {
                return Definition::ModuleMissing(module);
            };
            member_target(path, &text, &member)
        }
    }
}

fn member_target(path: SourcePath, text: &str, member: &str) -> Definition {
    let grammar = match path.language() {
        Language::Luau => Grammar::Luau,
        Language::Rhai => Grammar::Rhai,
    };
    let Ok(mut syntax) = Highlighter::new(grammar) else {
        return Definition::Unresolved;
    };
    syntax.update(text);
    let Exports::Known(exports) = syntax.exports() else {
        return Definition::Unresolved;
    };
    let mut found = exports.iter().filter(|export| export.name == member);
    match (found.next(), found.next()) {
        (Some(export), None) => Definition::Found(Destination {
            path,
            at: Spot::Offset(export.declared.start),
        }),
        (Some(_), Some(_)) => Definition::Ambiguous,
        (None, _) => Definition::Unresolved,
    }
}

/// The file of `module` as `language` scripts load it, and its text as it is now.
///
/// An open document wins over the file, so edits not yet saved are what gets analysed.
fn module_file(
    cartridge: &LoadedCartridge,
    code: &CodeSession,
    module: &str,
    language: Language,
) -> Option<(SourcePath, String)> {
    let name = ModuleName::parse(module).ok()?;
    let path = SourcePath::parse(name.path(language.extension()).as_str()).ok()?;
    if let Some(document) = code.document_at(&path) {
        return Some((path, document.text().to_owned()));
    }
    let bytes = cartridge.read_source(&path).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    Some((path, text))
}

#[cfg(test)]
mod tests;
