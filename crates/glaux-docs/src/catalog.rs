//! The Glaux script API as data.
//!
//! The catalog lists what scripts can call: names per language, parameters
//! and return types. It holds no prose, which lives in the documentation
//! page with the same id.
//!
//! # Authoring
//!
//! Entries are written by hand in `docs/api/<category>.ron`, one file per
//! category, and merged into one [`ApiCatalog`]. A consumer sees only the
//! merged catalog, never the files.
//!
//! # What is checked
//!
//! The runtime crates (`glaux-luau`, `glaux-rhai`) test that every entry's
//! names exist, that nothing is installed which the catalog does not list,
//! and, for Rhai, that every arity exists. **Parameter names, parameter
//! types and return types are maintained by hand and nothing verifies
//! them.** Neither does Luau arity. Review them against the runtime when
//! changing an API.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::{Diagnostic, DocId};

/// A language scripts are written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScriptLanguage {
    Luau,
    Rhai,
}

impl ScriptLanguage {
    pub const ALL: [Self; 2] = [Self::Luau, Self::Rhai];

    pub fn name(self) -> &'static str {
        match self {
            Self::Luau => "Luau",
            Self::Rhai => "Rhai",
        }
    }

    /// What separates a namespace from a member: `.` in Luau, `::` in Rhai.
    pub fn path_separator(self) -> &'static str {
        match self {
            Self::Luau => ".",
            Self::Rhai => "::",
        }
    }

    /// The name a script writes for `binding`, such as `audio.tone`.
    pub fn qualified_name(self, binding: &Binding) -> String {
        let mut name = String::new();
        for segment in &binding.path {
            name.push_str(segment);
            name.push_str(self.path_separator());
        }
        name.push_str(&binding.name);
        name
    }
}

/// What kind of thing an entry describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Kind {
    /// A function scripts call.
    Function,
    /// A table or module that groups functions, such as `audio`.
    Namespace,
    /// A table of constants whose members are defined elsewhere, such as
    /// `Key`. The entry's `source` names where.
    ConstantGroup,
    /// Syntax rather than a registered function, such as Rhai's `import`.
    /// Not looked up in the runtime.
    Keyword,
}

/// The type of a parameter or result, as scripts see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ValueType {
    /// Any number.
    Number,
    /// A whole number.
    Int,
    Text,
    Bool,
    Button,
    Key,
    /// One of the waveform names.
    Waveform,
    /// Any value, converted to text for display.
    Any,
}

/// How one language spells an entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    /// The namespaces to go through, outermost first.
    #[serde(default)]
    pub path: Vec<String>,
    pub name: String,
    /// The result type in this language, when it differs from the entry's.
    #[serde(default)]
    pub returns: Option<ValueType>,
}

/// A parameter of a function.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Param {
    pub name: String,
    pub ty: ValueType,
    #[serde(default)]
    pub optional: bool,
}

/// One documented piece of the script API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiEntry {
    id: DocId,
    category: String,
    kind: Kind,
    luau: Option<Binding>,
    rhai: Option<Binding>,
    params: Vec<Param>,
    returns: Option<ValueType>,
    source: Option<String>,
}

impl ApiEntry {
    /// The id of this entry and of its documentation page.
    pub fn id(&self) -> &DocId {
        &self.id
    }

    /// The category the entry is filed under, such as `graphics`. For
    /// grouping in lists only.
    pub fn category(&self) -> &str {
        &self.category
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// How `language` spells this entry, or `None` when it does not have it.
    pub fn binding(&self, language: ScriptLanguage) -> Option<&Binding> {
        match language {
            ScriptLanguage::Luau => self.luau.as_ref(),
            ScriptLanguage::Rhai => self.rhai.as_ref(),
        }
    }

    pub fn params(&self) -> &[Param] {
        &self.params
    }

    /// How many parameters a call must give.
    pub fn required_params(&self) -> usize {
        self.params.iter().filter(|param| !param.optional).count()
    }

    /// The result type in `language`, or `None` for no result.
    pub fn returns(&self, language: ScriptLanguage) -> Option<ValueType> {
        self.binding(language)
            .and_then(|binding| binding.returns)
            .or(self.returns)
    }

    /// For a constant group, the Rust item that defines its members.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }
}

/// The whole script API, merged from every category file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApiCatalog {
    entries: Vec<ApiEntry>,
}

impl ApiCatalog {
    /// Every entry, ordered by id.
    pub fn entries(&self) -> &[ApiEntry] {
        &self.entries
    }

    /// The entries `language` has, ordered by id.
    pub fn entries_for(&self, language: ScriptLanguage) -> impl Iterator<Item = &ApiEntry> {
        self.entries
            .iter()
            .filter(move |entry| entry.binding(language).is_some())
    }

    pub fn get(&self, id: &DocId) -> Option<&ApiEntry> {
        self.entries
            .binary_search_by(|entry| entry.id.cmp(id))
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Finds the entry `language` spells with this path and name.
    pub fn lookup(&self, language: ScriptLanguage, path: &[&str], name: &str) -> Option<&ApiEntry> {
        self.entries_for(language).find(|entry| {
            entry.binding(language).is_some_and(|binding| {
                binding.name == name
                    && binding
                        .path
                        .iter()
                        .map(String::as_str)
                        .eq(path.iter().copied())
            })
        })
    }

    /// Merges category files. Each item is a file path such as
    /// `api/audio.ron` and its contents.
    ///
    /// A file or entry with a problem is reported and left out, so one
    /// mistake does not hide the rest of the catalog.
    pub fn from_sources<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> (Self, Vec<Diagnostic>) {
        let mut diagnostics = Vec::new();
        let mut entries: BTreeMap<DocId, (String, ApiEntry)> = BTreeMap::new();
        let mut bindings: BTreeMap<(ScriptLanguage, String), DocId> = BTreeMap::new();

        let mut files: Vec<_> = files.into_iter().collect();
        files.sort_by_key(|(path, _)| *path);
        for (path, source) in files {
            let category = Path::new(path)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default();
            let raw: Vec<RawEntry> = match ron::from_str(source) {
                Ok(raw) => raw,
                Err(error) => {
                    diagnostics.push(Diagnostic::error(
                        "catalog-parse",
                        format!("{path}:{}", error.span.start.line),
                        error.code.to_string(),
                    ));
                    continue;
                }
            };
            for raw in raw {
                let location = format!("{path}: entry {:?}", raw.id);
                let entry = match raw.into_entry(category) {
                    Ok(entry) => entry,
                    Err(problem) => {
                        diagnostics.push(problem.into_diagnostic(&location));
                        continue;
                    }
                };
                if let Some((first, _)) = entries.get(&entry.id) {
                    diagnostics.push(Diagnostic::error(
                        "catalog-duplicate-id",
                        location,
                        format!("id is already defined in {first}"),
                    ));
                    continue;
                }
                let mut clash = None;
                for language in ScriptLanguage::ALL {
                    if let Some(binding) = entry.binding(language) {
                        let key = (language, language.qualified_name(binding));
                        if let Some(other) = bindings.get(&key) {
                            clash = Some(format!(
                                "{} `{}` is already claimed by {other}",
                                language.name(),
                                key.1
                            ));
                            break;
                        }
                    }
                }
                if let Some(message) = clash {
                    diagnostics.push(Diagnostic::error(
                        "catalog-duplicate-binding",
                        location,
                        message,
                    ));
                    continue;
                }
                for language in ScriptLanguage::ALL {
                    if let Some(binding) = entry.binding(language) {
                        bindings.insert(
                            (language, language.qualified_name(binding)),
                            entry.id.clone(),
                        );
                    }
                }
                entries.insert(entry.id.clone(), (path.to_owned(), entry));
            }
        }
        let entries = entries.into_values().map(|(_, entry)| entry).collect();
        (Self { entries }, diagnostics)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    id: String,
    kind: Kind,
    #[serde(default)]
    luau: Option<Binding>,
    #[serde(default)]
    rhai: Option<Binding>,
    #[serde(default)]
    params: Vec<Param>,
    #[serde(default)]
    returns: Option<ValueType>,
    #[serde(default)]
    source: Option<String>,
}

/// Why an entry was left out of the catalog.
struct Problem {
    code: &'static str,
    message: String,
}

impl Problem {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn into_diagnostic(self, location: &str) -> Diagnostic {
        Diagnostic::error(self.code, location, self.message)
    }
}

impl RawEntry {
    fn into_entry(self, category: &str) -> Result<ApiEntry, Problem> {
        let id = DocId::parse(&self.id)
            .map_err(|error| Problem::new("catalog-id", error.to_string()))?;
        let segments: Vec<_> = id.as_str().split('/').collect();
        if segments.len() != 3 || segments[0] != "api" {
            return Err(Problem::new(
                "catalog-id",
                "id must have the form api/<category>/<name>",
            ));
        }
        if segments[1] != category {
            return Err(Problem::new(
                "catalog-category",
                format!(
                    "id is in category `{}` but the entry is in `{category}.ron`",
                    segments[1]
                ),
            ));
        }
        self.check_definition()?;
        Ok(ApiEntry {
            id,
            category: category.to_owned(),
            kind: self.kind,
            luau: self.luau,
            rhai: self.rhai,
            params: self.params,
            returns: self.returns,
            source: self.source,
        })
    }

    fn check_definition(&self) -> Result<(), Problem> {
        let definition = |message: String| Problem::new("catalog-definition", message);
        if self.luau.is_none() && self.rhai.is_none() {
            return Err(definition(
                "entry has neither a luau nor a rhai binding".into(),
            ));
        }
        for (language, binding) in [("luau", &self.luau), ("rhai", &self.rhai)] {
            let Some(binding) = binding else { continue };
            for segment in binding.path.iter().chain([&binding.name]) {
                if !is_identifier(segment) {
                    return Err(definition(format!(
                        "{language} name {segment:?} is not an identifier"
                    )));
                }
            }
        }
        let callable = matches!(self.kind, Kind::Function | Kind::Keyword);
        if !callable && (!self.params.is_empty() || self.returns.is_some()) {
            return Err(definition(
                "only functions and keywords take parameters or return values".into(),
            ));
        }
        match (self.kind, &self.source) {
            (Kind::ConstantGroup, None) => {
                return Err(definition("a constant group needs a `source`".into()));
            }
            (kind, Some(_)) if kind != Kind::ConstantGroup => {
                return Err(definition("only a constant group has a `source`".into()));
            }
            _ => {}
        }
        let mut names = BTreeSet::new();
        let mut optional_seen = false;
        for param in &self.params {
            if !is_identifier(&param.name) {
                return Err(definition(format!(
                    "parameter name {:?} is not an identifier",
                    param.name
                )));
            }
            if !names.insert(param.name.as_str()) {
                return Err(definition(format!(
                    "parameter `{}` appears twice",
                    param.name
                )));
            }
            if optional_seen && !param.optional {
                return Err(definition(format!(
                    "required parameter `{}` follows an optional one",
                    param.name
                )));
            }
            optional_seen |= param.optional;
        }
        Ok(())
    }
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests;
