use glaux_docs::ScriptLanguage;
use glaux_syntax::{Definition, DefinitionKind, Highlighter, Import, SymbolKind};

use crate::context::{Position, context};

/// Where a name leads when the user asks for its definition.
///
/// This only says what the text proves. Finding another document, and the member inside
/// it, is left to the caller, because this crate knows nothing about files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A declaration in the same document.
    Local(Definition),
    /// A module loaded by a literal name, reached from its string or from its alias.
    Import { module: String },
    /// A member of an imported module, as in `math.clamp` or `math::clamp`.
    Member { module: String, member: String },
    /// Several declarations fit, such as Rhai overloads or a function written twice.
    Ambiguous,
    /// Nothing the script defines: engine API, values with unknown members, a name
    /// being declared, or text the analysis cannot follow.
    Unresolved,
}

/// Resolves the name at `offset`. `syntax` must have parsed `text`.
///
/// A name is only followed through an import alias that is still the declaration in
/// effect, so a local that shadows the alias leads nowhere.
pub fn definition(
    language: ScriptLanguage,
    text: &str,
    offset: usize,
    syntax: &Highlighter,
) -> Resolution {
    let imports = syntax.imports();
    if let Some(import) = imports
        .iter()
        .find(|i| i.module_range.start <= offset && offset <= i.module_range.end)
    {
        return Resolution::Import {
            module: import.module.clone(),
        };
    }
    let Some(context) = context(language, text, offset, syntax) else {
        return Resolution::Unresolved;
    };
    let word = &text[context.anchor.clone()];
    if word.is_empty() {
        return Resolution::Unresolved;
    }
    // Scopes are read at the end of the word, so a name that opens the file still sees
    // the functions declared there. The caret itself can be at the very start of it.
    let offset = context.anchor.end;
    match &context.position {
        Position::Reference { .. } => reference(syntax, &imports, offset, word),
        Position::Member { receiver, .. } => match receiver.as_slice() {
            [alias] => match import_named(syntax, &imports, offset, alias) {
                Some(import) => Resolution::Member {
                    module: import.module.clone(),
                    member: word.to_owned(),
                },
                None => Resolution::Unresolved,
            },
            _ => Resolution::Unresolved,
        },
        Position::Declaration { .. } | Position::Suppressed => Resolution::Unresolved,
    }
}

fn reference(syntax: &Highlighter, imports: &[Import], offset: usize, word: &str) -> Resolution {
    let Some(found) = syntax
        .definitions_at(offset)
        .into_iter()
        .find(|definition| definition.name == word)
    else {
        return Resolution::Unresolved;
    };
    if let Some(import) = imports.iter().find(|i| i.alias_range == found.declared) {
        return Resolution::Import {
            module: import.module.clone(),
        };
    }
    if found.kind == DefinitionKind::Function && declared_more_than_once(syntax, word) {
        return Resolution::Ambiguous;
    }
    Resolution::Local(found)
}

/// The import that `alias` names at `offset`, unless a closer declaration hides it.
fn import_named<'i>(
    syntax: &Highlighter,
    imports: &'i [Import],
    offset: usize,
    alias: &str,
) -> Option<&'i Import> {
    let found = syntax
        .definitions_at(offset)
        .into_iter()
        .find(|definition| definition.name == alias)?;
    imports.iter().find(|i| i.alias_range == found.declared)
}

fn declared_more_than_once(syntax: &Highlighter, name: &str) -> bool {
    syntax
        .symbols()
        .iter()
        .filter(|s| s.depth == 0 && s.kind == SymbolKind::Function && s.name == name)
        .count()
        > 1
}

#[cfg(test)]
mod tests;
