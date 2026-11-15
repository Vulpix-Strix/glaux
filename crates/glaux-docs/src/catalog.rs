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
//! The runtime crates (`glaux-luau`) test that every entry's names exist and
//! that nothing is installed which the catalog does not list. They also check
//! that a constant group's listed `constants` and `elements` are exactly the
//! runtime's table. **Parameter
//! names, parameter types and return types are maintained by hand and
//! nothing verifies them.** Neither does arity. Review them against the
//! runtime when changing an API.
//!
//! Callbacks are the exception to "names exist in the runtime". The script
//! defines them, so the runtime tests instead check that they are not
//! installed and that the catalog lists exactly the callbacks the runtimes
//! call.
//!
//! # Consumers
//!
//! An editor can build completion, signatures and snippets from the catalog
//! alone. A function or callback is declared as `name(params)` with the
//! parameters of [`ApiEntry::params`], spelled as [`ApiEntry::binding`] says
//! for the language being edited. A callback's declaration is
//! `function name() end` in Luau.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::{Diagnostic, DocId};

/// A language scripts are written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScriptLanguage {
    Luau,
}

impl ScriptLanguage {
    pub const ALL: [Self; 1] = [Self::Luau];

    pub fn name(self) -> &'static str {
        match self {
            Self::Luau => "Luau",
        }
    }

    /// What separates a namespace from a member: `.` in Luau.
    pub fn path_separator(self) -> &'static str {
        match self {
            Self::Luau => ".",
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
    /// A table of constants. Its members are either opaque values defined
    /// in Rust, such as `Key`, named by the entry's `source`, or values listed
    /// in the entry's `constants` and, for indexed tables, its `elements`.
    ConstantGroup,
    /// Syntax rather than a registered function.
    /// Not looked up in the runtime.
    Keyword,
    /// A function the script defines and Glaux calls, such as `tick`. The
    /// runtimes never register it, so it is not looked up there either.
    ///
    /// A callback is a global that takes no parameters and returns nothing.
    /// Its entry therefore has no `path`, no `params` and no `returns`.
    Callback,
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
    /// The parameters in this language, when they differ from the entry's.
    /// Needed when one language takes a value the others do not.
    #[serde(default)]
    pub params: Option<Vec<Param>>,
    /// The type of the extra arguments this language accepts after its
    /// parameters, zero or more. Never makes an argument required.
    #[serde(default)]
    pub varargs: Option<ValueType>,
}

/// One value a function gives back.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnValue {
    /// What the value means, when a function gives back several.
    #[serde(default)]
    pub name: Option<String>,
    pub ty: ValueType,
}

/// `returns: Int` for one value, or a list of named values.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawReturns {
    One(ValueType),
    Many(Vec<ReturnValue>),
}

impl RawReturns {
    fn into_values(self) -> Vec<ReturnValue> {
        match self {
            Self::One(ty) => vec![ReturnValue { name: None, ty }],
            Self::Many(values) => values,
        }
    }
}

/// Named constants of one type. Their values are defined by the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constants {
    pub ty: ValueType,
    pub names: Vec<String>,
}

/// The tables a constant group holds at the integer keys `first` to `last`,
/// inclusive, each with the same constants.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Elements {
    pub first: i64,
    pub last: i64,
    pub constants: Constants,
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
    params: Vec<Param>,
    returns: Vec<ReturnValue>,
    source: Option<String>,
    constants: Option<Constants>,
    elements: Option<Elements>,
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
        }
    }

    /// The parameters shared by the languages, unless a binding overrides
    /// them. See [`ApiEntry::params_for`].
    pub fn params(&self) -> &[Param] {
        &self.params
    }

    /// The parameters of `language`: those of its binding when it has its
    /// own, the entry's otherwise. Varargs are not included.
    pub fn params_for(&self, language: ScriptLanguage) -> &[Param] {
        self.binding(language)
            .and_then(|binding| binding.params.as_deref())
            .unwrap_or(&self.params)
    }

    /// The type of the extra arguments `language` accepts after its
    /// parameters, if it accepts any.
    pub fn varargs(&self, language: ScriptLanguage) -> Option<ValueType> {
        self.binding(language).and_then(|binding| binding.varargs)
    }

    /// How many parameters a call in `language` must give.
    pub fn required_params(&self, language: ScriptLanguage) -> usize {
        self.params_for(language)
            .iter()
            .filter(|param| !param.optional)
            .count()
    }

    /// The values a call in `language` gives back, in order. Empty when it
    /// gives back nothing.
    pub fn returns(&self, language: ScriptLanguage) -> Vec<ReturnValue> {
        match self.binding(language).and_then(|binding| binding.returns) {
            Some(ty) => vec![ReturnValue { name: None, ty }],
            None => self.returns.clone(),
        }
    }

    /// For a constant group of opaque values, the Rust item that defines its
    /// members.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// For a constant group of listed values, its named constants.
    pub fn constants(&self) -> Option<&Constants> {
        self.constants.as_ref()
    }

    /// For a constant group with indexed tables, what they hold.
    pub fn elements(&self) -> Option<&Elements> {
        self.elements.as_ref()
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
    params: Vec<Param>,
    #[serde(default)]
    returns: Option<RawReturns>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    constants: Option<Constants>,
    #[serde(default)]
    elements: Option<Elements>,
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
            params: self.params,
            returns: self
                .returns
                .map(RawReturns::into_values)
                .unwrap_or_default(),
            source: self.source,
            constants: self.constants,
            elements: self.elements,
        })
    }

    fn check_definition(&self) -> Result<(), Problem> {
        let definition = |message: String| Problem::new("catalog-definition", message);
        if self.luau.is_none() {
            return Err(definition("entry has no luau binding".into()));
        }
        if let Some(binding) = &self.luau {
            for segment in binding.path.iter().chain([&binding.name]) {
                if !is_identifier(segment) {
                    return Err(definition(format!(
                        "luau name {segment:?} is not an identifier"
                    )));
                }
            }
        }
        if self.kind == Kind::Callback {
            self.check_callback()?;
        }
        let callable = matches!(self.kind, Kind::Function | Kind::Keyword);
        let overrides = self
            .luau
            .iter()
            .any(|binding| binding.params.is_some() || binding.varargs.is_some());
        if !callable && (!self.params.is_empty() || self.returns.is_some() || overrides) {
            return Err(definition(
                "only functions and keywords take parameters or return values".into(),
            ));
        }
        self.check_constants()?;
        check_params(&self.params)?;
        if let Some(RawReturns::Many(values)) = &self.returns {
            check_returns(values)?;
        }
        if let Some(binding) = &self.luau
            && let Some(params) = &binding.params
        {
            check_params(params)?;
        }
        Ok(())
    }
}

fn check_params(params: &[Param]) -> Result<(), Problem> {
    let definition = |message: String| Problem::new("catalog-definition", message);
    let mut names = BTreeSet::new();
    let mut optional_seen = false;
    for param in params {
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

fn check_returns(values: &[ReturnValue]) -> Result<(), Problem> {
    let definition = |message: String| Problem::new("catalog-definition", message);
    if values.len() < 2 {
        return Err(definition(
            "a list of results needs at least two; write `returns: Type` for one".into(),
        ));
    }
    let mut names = BTreeSet::new();
    for value in values {
        let Some(name) = &value.name else {
            return Err(definition("each of several results needs a name".into()));
        };
        if !is_identifier(name) || !names.insert(name.as_str()) {
            return Err(definition(format!(
                "result name {name:?} is invalid or repeated"
            )));
        }
    }
    Ok(())
}

fn check_constants(constants: &Constants) -> Result<(), Problem> {
    let definition = |message: String| Problem::new("catalog-definition", message);
    if constants.names.is_empty() {
        return Err(definition("a constant list needs at least one name".into()));
    }
    let mut names = BTreeSet::new();
    for name in &constants.names {
        if !is_identifier(name) || !names.insert(name.as_str()) {
            return Err(definition(format!(
                "constant name {name:?} is invalid or repeated"
            )));
        }
    }
    Ok(())
}

impl RawEntry {
    /// A constant group has either a `source` or `constants`, and only a
    /// group with `constants` may have `elements`.
    fn check_constants(&self) -> Result<(), Problem> {
        let definition = |message: &str| Problem::new("catalog-definition", message);
        let listed = self.constants.is_some() || self.elements.is_some();
        if self.kind != Kind::ConstantGroup {
            if self.source.is_some() || listed {
                return Err(definition(
                    "only a constant group has a `source`, `constants` or `elements`",
                ));
            }
            return Ok(());
        }
        match (&self.source, &self.constants) {
            (None, None) => {
                return Err(definition(
                    "a constant group needs a `source` or `constants`",
                ));
            }
            (Some(_), _) if listed => {
                return Err(definition(
                    "a constant group has a `source` or `constants`, not both",
                ));
            }
            _ => {}
        }
        if let Some(constants) = &self.constants {
            check_constants(constants)?;
        }
        if let Some(elements) = &self.elements {
            if elements.first > elements.last {
                return Err(definition("`elements` needs `first` at most `last`"));
            }
            check_constants(&elements.constants)?;
        }
        Ok(())
    }

    /// A callback is a plain global.
    fn check_callback(&self) -> Result<(), Problem> {
        let definition = |message: &str| Problem::new("catalog-definition", message);
        let Some(luau) = &self.luau else {
            return Err(definition("a callback needs a luau binding"));
        };
        if !luau.path.is_empty() {
            return Err(definition("a callback is a global and has no path"));
        }
        if luau.returns.is_some() {
            return Err(definition("a callback returns nothing"));
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
