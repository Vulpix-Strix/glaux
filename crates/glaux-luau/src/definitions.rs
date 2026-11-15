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
//! A constant group whose catalog entry lists its `constants`, such as
//! `hardware.oam`, is an `extern type` too, so the analyzer rejects names
//! the group does not have. Its `elements` become a number indexer. The
//! default type solver has no read-only table properties, so writes are
//! only rejected by the runtime.
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
use glaux_docs::{
    ApiCatalog, ApiEntry, Constants, Kind, Param, ReturnValue, ScriptLanguage, ValueType,
};
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
        if entry.kind() == Kind::ConstantGroup && entry.constants().is_none() {
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
    let mut tables = String::new();
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
            Kind::ConstantGroup => {
                if let Some(constants) = entry.constants() {
                    let ty = table_type_name(&binding.path, &binding.name);
                    write_table_types(&mut tables, &ty, constants, entry);
                    root.descend(&binding.path)
                        .tables
                        .insert(binding.name.clone(), ty);
                }
                None
            }
            Kind::Keyword | Kind::Callback => None,
        };
    }

    out.push_str(&tables);
    out.push('\n');
    for (name, function) in &root.functions {
        let _ = writeln!(out, "declare function {name}{}", function.declaration());
    }
    for (name, ty) in &root.tables {
        let _ = writeln!(out, "declare {name}: {ty}");
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

/// `HardwareOam` for `hardware.oam`.
fn table_type_name(path: &[String], name: &str) -> String {
    path.iter()
        .map(String::as_str)
        .chain([name])
        .flat_map(|segment| segment.split('_'))
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase()
            })
        })
        .collect()
}

/// Declares `ty` with the group's constants and, for indexed tables, the
/// `<ty>Element` type they share.
fn write_table_types(out: &mut String, ty: &str, constants: &Constants, entry: &ApiEntry) {
    let element = entry.elements().map(|elements| {
        let element = format!("{ty}Element");
        write_extern_type(out, &element, &elements.constants, None);
        element
    });
    write_extern_type(out, ty, constants, element.as_deref());
}

fn write_extern_type(out: &mut String, ty: &str, constants: &Constants, indexer: Option<&str>) {
    let _ = writeln!(out, "\ndeclare extern type {ty} with");
    for name in &constants.names {
        let _ = writeln!(out, "\t{name}: {}", type_name(constants.ty));
    }
    if let Some(element) = indexer {
        let _ = writeln!(out, "\t[number]: {element}");
    }
    out.push_str("end\n");
}

/// The functions and nested tables under one global, sorted by name.
#[derive(Default)]
struct Namespace {
    functions: BTreeMap<String, FunctionType>,
    children: BTreeMap<String, Namespace>,
    /// Constant groups, by the name of their declared type.
    tables: BTreeMap<String, String>,
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
        if self.functions.is_empty() && self.children.is_empty() && self.tables.is_empty() {
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
        for (name, ty) in &self.tables {
            let _ = writeln!(out, "{indent}{name}: {ty},");
        }
        let _ = write!(out, "{}}}", "\t".repeat(depth));
    }
}

struct FunctionType {
    params: Vec<String>,
    /// `number` or `(number, number)`, `None` for no result.
    returns: Option<String>,
}

impl FunctionType {
    /// `(a: number) -> ()`, as a table member or a value.
    fn value(&self) -> String {
        format!(
            "({}) -> {}",
            self.params.join(", "),
            self.returns.as_deref().unwrap_or("()")
        )
    }

    /// `(a: number): boolean`, as `declare function` spells it.
    fn declaration(&self) -> String {
        let returns = self
            .returns
            .as_ref()
            .map(|ty| format!(": {ty}"))
            .unwrap_or_default();
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
        returns: returns_type(&entry.returns(LANGUAGE)),
    }
}

fn returns_type(values: &[ReturnValue]) -> Option<String> {
    match values {
        [] => None,
        [one] => Some(type_name(one.ty).to_owned()),
        many => {
            let types: Vec<_> = many.iter().map(|value| type_name(value.ty)).collect();
            Some(format!("({})", types.join(", ")))
        }
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
