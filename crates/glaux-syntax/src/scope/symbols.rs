use std::ops::Range;

use tree_sitter::Node;

use super::{
    child_of_kind, descendants_of_kind, fields, named, rhai_fn_name, statement_core, text,
};
use crate::grammar::Grammar;
use crate::highlighter::Highlighter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Variable,
    Constant,
}

/// A declaration worth listing in an outline of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// The declared name. Luau fields and methods keep their path, as in `Player.new`.
    pub name: String,
    pub kind: SymbolKind,
    /// Where the name is written in its declaration.
    pub declared: Range<usize>,
    /// How many functions enclose the declaration. Top-level symbols are at 0.
    pub depth: usize,
}

/// A module the script loads with a literal name, bound to an alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub alias: String,
    /// Where the alias is written in its declaration.
    pub alias_range: Range<usize>,
    /// The module name exactly as written in the string.
    pub module: String,
    /// The string literal including its quotes.
    pub module_range: Range<usize>,
}

/// A name a module makes available to the scripts that load it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub kind: SymbolKind,
    pub declared: Range<usize>,
}

/// What a module hands to its importers, as far as the text proves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exports {
    /// The module does something the analysis does not follow, such as returning a call
    /// or choosing the returned table at run time. Nothing can be said about its members.
    Unknown,
    /// Every export the analysis could prove. A name may appear more than once.
    Known(Vec<Export>),
}

impl Highlighter {
    /// The declarations of the document in source order, for an outline.
    ///
    /// Parameters and loop variables are left out. Variables are listed only at the top
    /// level. Luau also lists named functions nested in other code, while Rhai functions
    /// can only be top-level.
    pub fn symbols(&self) -> Vec<Symbol> {
        let Some((root, source)) = self.parsed() else {
            return Vec::new();
        };
        let mut symbols = Vec::new();
        match self.grammar() {
            Grammar::Luau => luau_symbols(root, source, 0, true, &mut symbols),
            Grammar::Rhai => {
                for (child, _) in fields(root) {
                    let core = statement_core(child);
                    match core.kind() {
                        "ExprFn" => {
                            if let Some(name) = rhai_fn_name(core) {
                                symbols.push(symbol(source, name, SymbolKind::Function, 0));
                            }
                        }
                        "ExprDeclareVar" => {
                            if let Some(name) = core.child_by_field_name("name") {
                                let kind = rhai_variable_kind(core);
                                symbols.push(symbol(source, name, kind, 0));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        symbols
    }

    /// The top-level modules the script loads by literal name.
    ///
    /// An import is dropped when its alias is assigned again later, since the alias then
    /// no longer proves what it holds. A name that is not a plain string is ignored.
    pub fn imports(&self) -> Vec<Import> {
        let Some((root, source)) = self.parsed() else {
            return Vec::new();
        };
        let mut imports = Vec::new();
        match self.grammar() {
            Grammar::Luau => {
                let assigned = luau_assigned_names(root, source);
                for (child, _) in fields(root) {
                    if let Some(import) = luau_import(child, source)
                        && !assigned.contains(&import.alias)
                    {
                        imports.push(import);
                    }
                }
            }
            Grammar::Rhai => {
                for (child, _) in fields(root) {
                    let core = statement_core(child);
                    if core.kind() == "ExprImport"
                        && let Some(alias) = child_of_kind(core, "ident")
                        && let Some((module, module_range)) = named(core)
                            .find(|n| n.kind() == "Expr")
                            .and_then(|expr| rhai_literal_string(expr, source))
                    {
                        imports.push(Import {
                            alias: text(source, alias).to_owned(),
                            alias_range: alias.byte_range(),
                            module,
                            module_range,
                        });
                    }
                }
            }
        }
        imports
    }

    /// What loading this document as a module makes available.
    ///
    /// In Luau a module is whatever its chunk returns, so only the shapes that name their
    /// members are followed: a returned table literal, or a returned local table that is
    /// filled in at the top level. In Rhai a module exports its top-level functions that
    /// are not `private`, and the variables declared with `export`.
    pub fn exports(&self) -> Exports {
        let Some((root, source)) = self.parsed() else {
            return Exports::Known(Vec::new());
        };
        match self.grammar() {
            Grammar::Luau => luau_exports(root, source),
            Grammar::Rhai => Exports::Known(rhai_exports(root, source)),
        }
    }
}

fn symbol(source: &str, name: Node, kind: SymbolKind, depth: usize) -> Symbol {
    Symbol {
        name: text(source, name).to_owned(),
        kind,
        declared: name.byte_range(),
        depth,
    }
}

fn rhai_variable_kind(declaration: Node) -> SymbolKind {
    let is_const = (0..declaration.child_count())
        .filter_map(|at| declaration.child(at as _))
        .any(|child| child.kind() == "const");
    if is_const {
        SymbolKind::Constant
    } else {
        SymbolKind::Variable
    }
}

fn luau_symbols(node: Node, source: &str, depth: usize, top: bool, out: &mut Vec<Symbol>) {
    for (child, _) in fields(node) {
        match child.kind() {
            "variable_declaration" if top => {
                for list in descendants_of_kind(child, "variable_list", 2) {
                    for name in named(list).filter(|n| n.kind() == "identifier") {
                        let kind = if luau_binds_function(child) {
                            SymbolKind::Function
                        } else {
                            SymbolKind::Variable
                        };
                        out.push(symbol(source, name, kind, depth));
                    }
                }
            }
            "function_declaration" => {
                if let Some(name) = child.child_by_field_name("name") {
                    out.push(symbol(source, name, SymbolKind::Function, depth));
                }
                luau_symbols(child, source, depth + 1, false, out);
            }
            "function_definition" => luau_symbols(child, source, depth + 1, false, out),
            _ => luau_symbols(child, source, depth, false, out),
        }
    }
}

/// Whether `local f = function() ... end`, so the variable reads as a function.
fn luau_binds_function(declaration: Node) -> bool {
    let Some(values) = descendants_of_kind(declaration, "expression_list", 2).pop() else {
        return false;
    };
    let mut values = named(values);
    matches!(
        (values.next(), values.next()),
        (Some(only), None) if only.kind() == "function_definition"
    )
}

/// The text of a Luau string with no escapes or interpolation.
fn luau_literal_string(string: Node, source: &str) -> Option<(String, Range<usize>)> {
    if string.kind() != "string" {
        return None;
    }
    let mut parts = named(string);
    let content = parts.next()?;
    (content.kind() == "string_content" && parts.next().is_none())
        .then(|| (text(source, content).to_owned(), string.byte_range()))
}

/// The text and range of a Rhai expression that is a plain string literal.
fn rhai_literal_string(expr: Node, source: &str) -> Option<(String, Range<usize>)> {
    let literal = descendants_of_kind(expr, "lit_str", 4).pop()?;
    let raw = text(source, literal);
    let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.contains('\\')).then(|| (inner.to_owned(), literal.byte_range()))
}

/// `local alias = require("literal")` with exactly one name and one value.
fn luau_import(statement: Node, source: &str) -> Option<Import> {
    if statement.kind() != "variable_declaration" {
        return None;
    }
    let lists = descendants_of_kind(statement, "variable_list", 2);
    let values = descendants_of_kind(statement, "expression_list", 2);
    let (list, values) = (lists.first()?, values.first()?);
    let mut names = named(*list);
    let alias = names.next().filter(|n| n.kind() == "identifier")?;
    if names.next().is_some() {
        return None;
    }
    let mut values = named(*values);
    let call = values.next().filter(|n| n.kind() == "function_call")?;
    if values.next().is_some() || text(source, call.child_by_field_name("name")?) != "require" {
        return None;
    }
    let mut arguments = named(call.child_by_field_name("arguments")?);
    let (module, module_range) = luau_literal_string(arguments.next()?, source)?;
    arguments.next().is_none().then(|| Import {
        alias: text(source, alias).to_owned(),
        alias_range: alias.byte_range(),
        module,
        module_range,
    })
}

/// The names assigned anywhere after being declared, such as `m = other`.
fn luau_assigned_names(root: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.kind() == "assignment_statement"
            && node
                .parent()
                .is_some_and(|p| p.kind() != "variable_declaration")
        {
            for list in named(node).filter(|n| n.kind() == "variable_list") {
                for name in named(list).filter(|n| n.kind() == "identifier") {
                    names.push(text(source, name).to_owned());
                }
            }
        }
        pending.extend(named(node));
    }
    names
}

fn luau_exports(root: Node, source: &str) -> Exports {
    let returns: Vec<Node> = fields(root)
        .into_iter()
        .map(|(child, _)| child)
        .filter(|child| child.kind() == "return_statement")
        .collect();
    if has_conditional_return(root) || returns.len() > 1 {
        return Exports::Unknown;
    }
    let Some(statement) = returns.first() else {
        return Exports::Known(Vec::new());
    };
    let Some(list) = named(*statement).find(|n| n.kind() == "expression_list") else {
        return Exports::Known(Vec::new());
    };
    let mut values = named(list).filter(|n| n.kind() != "comment");
    let (Some(value), None) = (values.next(), values.next()) else {
        return Exports::Unknown;
    };
    match value.kind() {
        "table_constructor" => Exports::Known(table_fields(value, source)),
        "identifier" => local_table_exports(root, source, text(source, value)),
        _ => Exports::Unknown,
    }
}

/// Whether a `return` sits inside a block of the chunk, which could end the module early
/// with a different value. Returns inside function bodies are not the module's.
fn has_conditional_return(root: Node) -> bool {
    let mut pending: Vec<Node> = named(root)
        .filter(|n| !matches!(n.kind(), "function_declaration" | "return_statement"))
        .collect();
    while let Some(node) = pending.pop() {
        match node.kind() {
            "return_statement" => return true,
            "function_declaration" | "function_definition" => {}
            _ => pending.extend(named(node)),
        }
    }
    false
}

fn table_fields(table: Node, source: &str) -> Vec<Export> {
    named(table)
        .filter(|n| n.kind() == "field")
        .filter_map(|field| {
            let name = field.child_by_field_name("name")?;
            let value = field.child_by_field_name("value")?;
            let kind = if value.kind() == "function_definition" {
                SymbolKind::Function
            } else {
                SymbolKind::Variable
            };
            Some(export(source, name, kind))
        })
        .collect()
}

fn export(source: &str, name: Node, kind: SymbolKind) -> Export {
    Export {
        name: text(source, name).to_owned(),
        kind,
        declared: name.byte_range(),
    }
}

/// The members of `local name = {...}` that the chunk returns, when nothing else could
/// have replaced `name` before the return.
fn local_table_exports(root: Node, source: &str, name: &str) -> Exports {
    let mut table = None;
    for (child, _) in fields(root) {
        if child.kind() != "variable_declaration" {
            continue;
        }
        let binds = descendants_of_kind(child, "variable_list", 2)
            .iter()
            .flat_map(|list| named(*list))
            .any(|n| n.kind() == "identifier" && text(source, n) == name);
        if !binds {
            continue;
        }
        if table.is_some() {
            return Exports::Unknown;
        }
        table = luau_table_value(child, source, name);
        if table.is_none() {
            return Exports::Unknown;
        }
    }
    let Some(table) = table else {
        return Exports::Unknown;
    };
    if luau_assigned_names(root, source).iter().any(|n| n == name) {
        return Exports::Unknown;
    }
    let mut exports = table_fields(table, source);
    for (child, _) in fields(root) {
        match child.kind() {
            "function_declaration" => {
                let Some(target) = child.child_by_field_name("name") else {
                    continue;
                };
                let member = match target.kind() {
                    "dot_index_expression" => target.child_by_field_name("field"),
                    "method_index_expression" => target.child_by_field_name("method"),
                    _ => None,
                };
                if let Some(member) = member
                    && indexes(target, source, name)
                {
                    exports.push(export(source, member, SymbolKind::Function));
                }
            }
            "assignment_statement" => {
                let mut lists = named(child).filter(|n| n.kind() == "variable_list");
                let (Some(list), None) = (lists.next(), lists.next()) else {
                    continue;
                };
                let mut targets = named(list);
                let (Some(target), None) = (targets.next(), targets.next()) else {
                    continue;
                };
                if target.kind() == "dot_index_expression"
                    && indexes(target, source, name)
                    && let Some(member) = target.child_by_field_name("field")
                {
                    let kind = match descendants_of_kind(child, "expression_list", 2).pop() {
                        Some(values)
                            if named(values).any(|v| v.kind() == "function_definition") =>
                        {
                            SymbolKind::Function
                        }
                        _ => SymbolKind::Variable,
                    };
                    exports.push(export(source, member, kind));
                }
            }
            _ => {}
        }
    }
    Exports::Known(exports)
}

/// The table constructor of `local name = {...}`, if that is all the statement does.
fn luau_table_value<'t>(declaration: Node<'t>, source: &str, name: &str) -> Option<Node<'t>> {
    let lists = descendants_of_kind(declaration, "variable_list", 2);
    let values = descendants_of_kind(declaration, "expression_list", 2);
    let mut names = named(*lists.first()?);
    let only = names.next().filter(|n| text(source, *n) == name)?;
    let _ = only;
    if names.next().is_some() {
        return None;
    }
    let mut values = named(*values.first()?);
    let table = values.next().filter(|n| n.kind() == "table_constructor")?;
    values.next().is_none().then_some(table)
}

/// Whether `target` is `name.member` or `name:member`.
fn indexes(target: Node, source: &str, name: &str) -> bool {
    target
        .child_by_field_name("table")
        .is_some_and(|table| table.kind() == "identifier" && text(source, table) == name)
}

fn rhai_exports(root: Node, source: &str) -> Vec<Export> {
    let mut exports = Vec::new();
    for (child, _) in fields(root) {
        let core = statement_core(child);
        let first = core.child(0).map(|n| n.kind());
        match core.kind() {
            "ExprFn" if first != Some("private") => {
                if let Some(name) = rhai_fn_name(core) {
                    exports.push(export(source, name, SymbolKind::Function));
                }
            }
            "ExprDeclareVar" if first == Some("export") => {
                if let Some(name) = core.child_by_field_name("name") {
                    exports.push(export(source, name, rhai_variable_kind(core)));
                }
            }
            _ => {}
        }
    }
    exports
}

#[cfg(test)]
mod tests;
