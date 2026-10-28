//! Luau type definitions for the Glaux API, generated from the API catalog.
//!
//! [`definitions`] renders the file that `luau-lsp` loads with
//! `--definitions` so a type checker knows what a cartridge can call. The
//! catalog is the only source: names, parameters, optional parameters,
//! varargs and result types are taken from the Luau side of each entry.
//!
//! `Button` and `Key` are declared as opaque `extern type`s, because a
//! cartridge can neither build one nor use a number or table in its place.
//! Their members come from the Rust items the catalog names in `source`.
//! `Waveform` is a union of the strings the runtime accepts.
//!
//! The catalog does not describe everything, and the output does not guess:
//!
//! - `Number` and `Int` are both `number`, since Luau has no integer type.
//!   Integer ranges are checked by the runtime and are not in the types.
//! - A function with no result type returns `()`.
//! - Callbacks such as `tick` are defined by the script, so they have no
//!   declaration.
//! - `require` has no declaration. A declared `require` replaces the
//!   analyzer's own, which makes every module `any` and hides unknown
//!   modules. The analyzer resolves the call against the cartridge instead.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use glaux_audio::Waveform;
use glaux_docs::{ApiCatalog, ApiEntry, Kind, Param, ScriptLanguage, ValueType};
use glaux_input::{Button, Key};

const LANGUAGE: ScriptLanguage = ScriptLanguage::Luau;

/// Globals the analyzer types itself. Declaring them would override it.
pub(crate) const ANALYZER_GLOBALS: &[&str] = &["require"];

/// Why the catalog cannot be rendered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DefinitionsError {
    /// A constant group names a source this generator does not know how to
    /// list.
    #[error("{id}: unknown constant source `{source_name}`")]
    UnknownSource { id: String, source_name: String },
}

/// The `glaux.d.luau` text for every entry of `api` that Luau has.
///
/// The result depends only on the catalog and the Rust constants it refers
/// to, so equal inputs give equal text.
pub fn definitions(api: &ApiCatalog) -> Result<String, DefinitionsError> {
    let mut out = String::from(
        "-- Glaux API definitions for Luau.\n\
         -- Generated from docs/api/*.ron by `mise run luau-defs`. Do not edit.\n\n",
    );

    out.push_str("type Waveform = ");
    let names: Vec<String> = Waveform::ALL
        .iter()
        .map(|waveform| format!("\"{}\"", waveform.name()))
        .collect();
    out.push_str(&names.join(" | "));
    out.push('\n');

    let mut groups = Vec::new();
    for entry in api.entries_for(LANGUAGE) {
        if entry.kind() == Kind::ConstantGroup {
            groups.push(constant_group(entry)?);
        }
    }
    for group in &groups {
        let _ = write!(out, "\ndeclare extern type {} with\nend\n", group.name);
    }
    for group in &groups {
        let _ = writeln!(out, "\ndeclare {}: {{", group.name);
        for member in &group.members {
            let _ = writeln!(out, "\t{member}: {},", group.name);
        }
        out.push_str("}\n");
    }

    let mut root = Namespace::default();
    for entry in api.entries_for(LANGUAGE) {
        let Some(binding) = entry.binding(LANGUAGE) else {
            continue;
        };
        if binding.path.is_empty() && ANALYZER_GLOBALS.contains(&binding.name.as_str()) {
            continue;
        }
        match entry.kind() {
            Kind::Function => root
                .descend(&binding.path)
                .functions
                .insert(binding.name.clone(), function_type(entry)),
            Kind::Namespace => {
                root.descend(&binding.path)
                    .descend(std::slice::from_ref(&binding.name));
                None
            }
            Kind::ConstantGroup | Kind::Keyword | Kind::Callback => None,
        };
    }

    out.push('\n');
    for (name, function) in &root.functions {
        let _ = writeln!(out, "declare function {name}{}", function.declaration());
    }
    for (name, namespace) in &root.children {
        let _ = write!(out, "\ndeclare {name}: ");
        namespace.render(&mut out, 0);
        out.push('\n');
    }
    Ok(out)
}

struct ConstantGroup {
    name: String,
    members: Vec<&'static str>,
}

fn constant_group(entry: &ApiEntry) -> Result<ConstantGroup, DefinitionsError> {
    let members = match entry.source() {
        Some("glaux_input::Button") => Button::ALL.iter().map(|b| b.name()).collect(),
        Some("glaux_input::Key") => Key::ALL.iter().map(|k| k.name()).collect(),
        other => {
            return Err(DefinitionsError::UnknownSource {
                id: entry.id().to_string(),
                source_name: other.unwrap_or_default().to_owned(),
            });
        }
    };
    let name = entry
        .binding(LANGUAGE)
        .map(|binding| binding.name.clone())
        .unwrap_or_default();
    Ok(ConstantGroup { name, members })
}

/// The functions and nested tables under one global, sorted by name.
#[derive(Default)]
struct Namespace {
    functions: BTreeMap<String, FunctionType>,
    children: BTreeMap<String, Namespace>,
}

impl Namespace {
    fn descend(&mut self, path: &[String]) -> &mut Namespace {
        match path.split_first() {
            None => self,
            Some((first, rest)) => self
                .children
                .entry(first.clone())
                .or_default()
                .descend(rest),
        }
    }

    fn render(&self, out: &mut String, depth: usize) {
        if self.functions.is_empty() && self.children.is_empty() {
            out.push_str("{}");
            return;
        }
        let indent = "\t".repeat(depth + 1);
        out.push_str("{\n");
        for (name, function) in &self.functions {
            let _ = writeln!(out, "{indent}{name}: {},", function.value());
        }
        for (name, child) in &self.children {
            let _ = write!(out, "{indent}{name}: ");
            child.render(out, depth + 1);
            out.push_str(",\n");
        }
        let _ = write!(out, "{}}}", "\t".repeat(depth));
    }
}

struct FunctionType {
    params: Vec<String>,
    returns: Option<&'static str>,
}

impl FunctionType {
    /// `(a: number) -> ()`, as a table member or a value.
    fn value(&self) -> String {
        format!(
            "({}) -> {}",
            self.params.join(", "),
            self.returns.unwrap_or("()")
        )
    }

    /// `(a: number): boolean`, as `declare function` spells it.
    fn declaration(&self) -> String {
        let returns = self.returns.map(|ty| format!(": {ty}")).unwrap_or_default();
        format!("({}){returns}", self.params.join(", "))
    }
}

fn function_type(entry: &ApiEntry) -> FunctionType {
    let mut params: Vec<String> = entry.params_for(LANGUAGE).iter().map(param).collect();
    if let Some(ty) = entry.varargs(LANGUAGE) {
        params.push(format!("...{}", type_name(ty)));
    }
    FunctionType {
        params,
        returns: entry.returns(LANGUAGE).map(type_name),
    }
}

fn param(param: &Param) -> String {
    let optional = if param.optional { "?" } else { "" };
    format!("{}: {}{optional}", param.name, type_name(param.ty))
}

fn type_name(ty: ValueType) -> &'static str {
    match ty {
        ValueType::Number | ValueType::Int => "number",
        ValueType::Text => "string",
        ValueType::Bool => "boolean",
        ValueType::Button => "Button",
        ValueType::Key => "Key",
        ValueType::Waveform => "Waveform",
        ValueType::Any => "any",
    }
}

#[cfg(test)]
mod tests;
