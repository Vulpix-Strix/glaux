use std::collections::HashSet;
use std::ops::Range;

use tree_sitter::Node;

use crate::grammar::Grammar;
use crate::highlighter::Highlighter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionKind {
    /// A local, constant, loop variable or import alias.
    Variable,
    Parameter,
    Function,
}

/// A name declared by the script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    pub kind: DefinitionKind,
    /// Where the name is written in its declaration.
    pub declared: Range<usize>,
    /// The offset after which the name can be used. A local is not visible until its
    /// statement ends, so `local x = x` reads the outer `x`, while a function can
    /// already see itself in its own body.
    pub visible_from: usize,
    /// The parameter names of a function. Empty for anything else.
    pub params: Vec<String>,
}

/// Facts about a word that the parse tree knows and plain text does not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Site {
    /// The word names a declaration, such as `x` in `local x` or a parameter.
    pub declares_name: bool,
    /// The word is inside the `{...}` or `${...}` of a string template, which is code
    /// even though the text around it is a string.
    pub in_interpolation: bool,
}

impl Highlighter {
    /// The declarations that can be used at `offset`, innermost scope first.
    ///
    /// A name declared more than once appears once, as the declaration in effect at
    /// `offset`. Where the parser could not make sense of the text it loses block
    /// boundaries, so the result may then include names from a neighbouring scope.
    pub fn definitions_at(&self, offset: usize) -> Vec<Definition> {
        let Some((root, source)) = self.parsed() else {
            return Vec::new();
        };
        let mut scopes = Scopes {
            grammar: self.grammar(),
            source,
            offset,
            functions_only: false,
            seen: HashSet::new(),
            found: Vec::new(),
        };
        for node in enclosing(root, offset) {
            scopes.visit(node);
        }
        scopes.found
    }

    /// Describes the word at `word`, which is an empty range when nothing is typed yet.
    pub fn site_at(&self, word: Range<usize>) -> Site {
        let Some((root, _)) = self.parsed() else {
            return Site::default();
        };
        let grammar = self.grammar();
        let mut site = Site::default();
        if !word.is_empty()
            && let Some(node) = root.descendant_for_byte_range(word.start, word.end)
        {
            site.declares_name = declares_name(grammar, node, &word);
        }
        for node in enclosing(root, word.start) {
            if is_template_code(grammar, node)
                && node.start_byte() < word.start
                && word.start < node.end_byte()
            {
                site.in_interpolation = true;
            }
        }
        site
    }

    /// The functions the host can call by name.
    ///
    /// In Luau these are the global functions, so a `local function` does not count. In
    /// Rhai they are the top-level functions that are not `private`.
    pub fn global_functions(&self) -> Vec<String> {
        let Some((root, source)) = self.parsed() else {
            return Vec::new();
        };
        let mut names = Vec::new();
        for (child, field) in fields(root) {
            match self.grammar() {
                Grammar::Luau => {
                    if child.kind() == "function_declaration"
                        && field != Some("local_declaration")
                        && let Some(name) = child.child_by_field_name("name")
                        && name.kind() == "identifier"
                    {
                        names.push(text(source, name).to_owned());
                    }
                }
                Grammar::Rhai => {
                    let core = statement_core(child);
                    if core.kind() == "ExprFn"
                        && core.child(0).is_none_or(|first| first.kind() != "private")
                        && let Some(name) = rhai_fn_name(core)
                    {
                        names.push(text(source, name).to_owned());
                    }
                }
            }
        }
        names
    }
}

impl Highlighter {
    /// Every top-level function called `name`, in source order.
    ///
    /// Rhai allows several functions with one name when their parameter counts differ,
    /// and [`Highlighter::definitions_at`] keeps only one of them. Luau has no overloads,
    /// so the result is always empty there.
    pub fn overloads(&self, name: &str) -> Vec<Definition> {
        let Some((root, source)) = self.parsed() else {
            return Vec::new();
        };
        if self.grammar() != Grammar::Rhai {
            return Vec::new();
        }
        fields(root)
            .into_iter()
            .filter_map(|(child, _)| {
                let core = statement_core(child);
                let declared = rhai_fn_name(core).filter(|_| core.kind() == "ExprFn")?;
                (text(source, declared) == name).then(|| Definition {
                    name: name.to_owned(),
                    kind: DefinitionKind::Function,
                    declared: declared.byte_range(),
                    visible_from: 0,
                    params: rhai_params_of(core, source),
                })
            })
            .collect()
    }
}

struct Scopes<'a> {
    grammar: Grammar,
    source: &'a str,
    offset: usize,
    /// Set once the walk leaves a Rhai function, which cannot see the locals around it.
    functions_only: bool,
    seen: HashSet<String>,
    found: Vec<Definition>,
}

impl Scopes<'_> {
    fn visit(&mut self, node: Node) {
        match self.grammar {
            Grammar::Luau => self.visit_luau(node),
            Grammar::Rhai => self.visit_rhai(node),
        }
    }

    fn visit_luau(&mut self, node: Node) {
        if !matches!(node.kind(), "chunk" | "block" | "ERROR")
            && let Some(body) = self.trailing_block(node)
        {
            self.luau_block(body);
        }
        match node.kind() {
            "chunk" | "block" | "ERROR" => self.luau_block(node),
            "function_declaration" | "function_definition" => {
                if let Some(parameters) = node.child_by_field_name("parameters") {
                    let after = parameters.end_byte();
                    for parameter in named(parameters).filter(|p| p.kind() == "parameter") {
                        if let Some(name) = parameter.named_child(0) {
                            self.push(name, DefinitionKind::Parameter, after, Vec::new());
                        }
                    }
                }
                let is_method = node
                    .child_by_field_name("name")
                    .is_some_and(|name| name.kind() == "method_index_expression");
                if is_method && let Some(parameters) = node.child_by_field_name("parameters") {
                    self.push_implicit("self", parameters, DefinitionKind::Parameter);
                }
            }
            "for_statement" => {
                if let Some(clause) = node.child_by_field_name("clause") {
                    for name in named(clause) {
                        match name.kind() {
                            "identifier" => self.push(
                                name,
                                DefinitionKind::Variable,
                                clause.end_byte(),
                                Vec::new(),
                            ),
                            "variable_list" => {
                                for name in named(name).filter(|n| n.kind() == "identifier") {
                                    self.push(
                                        name,
                                        DefinitionKind::Variable,
                                        clause.end_byte(),
                                        Vec::new(),
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "repeat_statement" => {
                // The condition of `until` can use the locals of the body.
                if let Some(body) = node.child_by_field_name("body")
                    && self.offset > body.end_byte()
                {
                    self.luau_block(body);
                }
            }
            _ => {}
        }
    }

    /// The block of `node` that the caret is still working in, though it lies after
    /// the block's last statement.
    ///
    /// A Luau block ends with its last statement, so a caret on a blank line before
    /// `end` is outside it. The same goes for a half-typed statement, which the parser
    /// sets aside as an error. A keyword such as `else` does end the block.
    fn trailing_block<'t>(&self, node: Node<'t>) -> Option<Node<'t>> {
        let last = fields(node)
            .into_iter()
            .map(|(child, _)| child)
            .take_while(|child| child.start_byte() < self.offset)
            .filter(|child| !matches!(child.kind(), "ERROR" | "comment"))
            .last()?;
        match last.kind() {
            "block" => Some(last).filter(|block| block.end_byte() <= self.offset),
            "else_statement" | "elseif_statement" => self.trailing_block(last),
            _ => None,
        }
    }

    fn luau_block(&mut self, block: Node) {
        let top = block.kind() == "chunk";
        let mut declared = Vec::new();
        for (child, field) in fields(block) {
            let local = field == Some("local_declaration");
            match child.kind() {
                "variable_declaration" => {
                    let end = initializer_end(self.source, child.end_byte());
                    for list in descendants_of_kind(child, "variable_list", 2) {
                        for name in named(list).filter(|n| n.kind() == "identifier") {
                            declared.push(self.definition(
                                name,
                                DefinitionKind::Variable,
                                end,
                                Vec::new(),
                            ));
                        }
                    }
                }
                "function_declaration" => {
                    let Some(name) = child.child_by_field_name("name") else {
                        continue;
                    };
                    if name.kind() != "identifier" {
                        continue;
                    }
                    // A global function exists for the whole script, wherever it is written.
                    let from = if local || !top { child.start_byte() } else { 0 };
                    let params = luau_params(child, self.source);
                    declared.push(self.definition(name, DefinitionKind::Function, from, params));
                }
                _ => {}
            }
        }
        self.extend_later_first(declared);
    }

    fn visit_rhai(&mut self, node: Node) {
        match node.kind() {
            "Rhai" | "ExprBlock" | "ERROR" => self.rhai_block(node),
            "ExprFn" => {
                if let Some(list) = child_of_kind(node, "ParamList") {
                    self.rhai_params(list);
                }
                self.functions_only = true;
            }
            "ExprClosure" => {
                if let Some(list) = child_of_kind(node, "ClosureParamList") {
                    self.rhai_params(list);
                }
            }
            "ExprFor" => {
                let after = child_of_kind(node, "ExprBlock").map_or(usize::MAX, |b| b.start_byte());
                if let Some(pattern) = child_of_kind(node, "Pat") {
                    for name in descendants_of_kind(pattern, "ident", usize::MAX) {
                        self.push(name, DefinitionKind::Variable, after, Vec::new());
                    }
                }
            }
            "ExprTry" => {
                if let Some(list) = child_of_kind(node, "ParamList") {
                    self.rhai_params(list);
                }
            }
            _ => {}
        }
    }

    fn rhai_params(&mut self, list: Node) {
        for param in named(list).filter(|n| n.kind() == "Param") {
            if let Some(name) = child_of_kind(param, "ident") {
                self.push(name, DefinitionKind::Parameter, list.end_byte(), Vec::new());
            }
        }
    }

    fn rhai_block(&mut self, block: Node) {
        let top = block.kind() == "Rhai";
        let mut declared = Vec::new();
        for (child, _) in fields(block) {
            let core = statement_core(child);
            match core.kind() {
                "ExprDeclareVar" => {
                    if let Some(name) = core.child_by_field_name("name") {
                        declared.push(self.definition(
                            name,
                            DefinitionKind::Variable,
                            initializer_end(self.source, core.end_byte()),
                            Vec::new(),
                        ));
                    }
                }
                "ExprImport" => {
                    if let Some(alias) = child_of_kind(core, "ident") {
                        declared.push(self.definition(
                            alias,
                            DefinitionKind::Variable,
                            core.end_byte(),
                            Vec::new(),
                        ));
                    }
                }
                // Rhai functions exist only at the top level, and each can call the
                // others whatever their order.
                "ExprFn" if top => {
                    if let Some(name) = rhai_fn_name(core) {
                        let params = rhai_params_of(core, self.source);
                        declared.push(self.definition(name, DefinitionKind::Function, 0, params));
                    }
                }
                _ => {}
            }
        }
        self.extend_later_first(declared);
    }

    fn definition(
        &self,
        name: Node,
        kind: DefinitionKind,
        visible_from: usize,
        params: Vec<String>,
    ) -> Definition {
        Definition {
            name: text(self.source, name).to_owned(),
            kind,
            declared: name.byte_range(),
            visible_from,
            params,
        }
    }

    fn push(&mut self, name: Node, kind: DefinitionKind, from: usize, params: Vec<String>) {
        let definition = self.definition(name, kind, from, params);
        self.accept(definition);
    }

    fn push_implicit(&mut self, name: &str, anchor: Node, kind: DefinitionKind) {
        if !self.seen.contains(name) && anchor.end_byte() < self.offset && !self.functions_only {
            self.seen.insert(name.to_owned());
            self.found.push(Definition {
                name: name.to_owned(),
                kind,
                declared: anchor.start_byte()..anchor.start_byte(),
                visible_from: anchor.end_byte(),
                params: Vec::new(),
            });
        }
    }

    /// Accepts the declarations of one block from last to first, because a later
    /// declaration hides an earlier one of the same name.
    fn extend_later_first(&mut self, declared: Vec<Definition>) {
        for definition in declared.into_iter().rev() {
            self.accept(definition);
        }
    }

    fn accept(&mut self, definition: Definition) {
        let touches =
            definition.declared.start <= self.offset && self.offset <= definition.declared.end;
        let in_effect = definition.visible_from < self.offset;
        let reachable = !self.functions_only || definition.kind == DefinitionKind::Function;
        if touches || !in_effect || !reachable || self.seen.contains(&definition.name) {
            return;
        }
        self.seen.insert(definition.name.clone());
        self.found.push(definition);
    }
}

/// Moves the end of a declaration past an initializer the parser dropped.
///
/// While the expression is still missing, `local y = ` ends before the `=`. The name
/// has to stay hidden until the expression is written, including the word being typed.
fn initializer_end(source: &str, end: usize) -> usize {
    let rest = source.get(end..).unwrap_or_default();
    let after_space = rest.trim_start();
    let Some(after_equals) = after_space
        .strip_prefix('=')
        .filter(|r| !r.starts_with('='))
    else {
        return end;
    };
    let value = after_equals.trim_start();
    let word = value
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(value.len());
    source.len() - value.len() + word
}

/// The nodes that contain `offset`, innermost first.
fn enclosing(root: Node, offset: usize) -> Vec<Node> {
    let Some(start) = root.descendant_for_byte_range(offset.saturating_sub(1), offset) else {
        return vec![root];
    };
    let mut chain = Vec::new();
    let mut node = Some(start);
    while let Some(current) = node {
        if encloses(current, offset) {
            chain.push(current);
        }
        node = current.parent();
    }
    chain
}

/// Whether `offset` is inside `node` rather than at its edge. An unfinished construct,
/// one without its closing token, also contains the offset at its very end.
fn encloses(node: Node, offset: usize) -> bool {
    if node.parent().is_none() {
        return true;
    }
    if offset <= node.start_byte() {
        return false;
    }
    if offset < node.end_byte() {
        return true;
    }
    offset == node.end_byte() && !is_closed(node)
}

fn is_closed(node: Node) -> bool {
    node.child(node.child_count().saturating_sub(1))
        .is_some_and(|last| !last.is_missing() && matches!(last.kind(), "end" | "}"))
}

fn is_template_code(grammar: Grammar, node: Node) -> bool {
    match grammar {
        Grammar::Luau => node.kind() == "interpolation",
        Grammar::Rhai => node.kind() == "str_template_expr",
    }
}

fn declares_name(grammar: Grammar, node: Node, word: &Range<usize>) -> bool {
    let kind = match grammar {
        Grammar::Luau => "identifier",
        Grammar::Rhai => "ident",
    };
    if node.kind() != kind || node.byte_range() != *word {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    let named_as = |field: &str| parent.child_by_field_name(field) == Some(node);
    match (grammar, parent.kind()) {
        (Grammar::Luau, "variable_list") => {
            // `x, y = 1, 2` only assigns. A list declares when it sits under `local` or
            // `for`, and an unfinished `local x` has no assignment in between.
            let above = parent.parent();
            let grand = above.and_then(|node| node.parent());
            [above, grand]
                .into_iter()
                .flatten()
                .any(|node| matches!(node.kind(), "variable_declaration" | "for_generic_clause"))
        }
        (Grammar::Luau, "parameter") => parent.named_child(0) == Some(node),
        (Grammar::Luau, "for_numeric_clause") => named_as("name"),
        (Grammar::Luau, "function_declaration") => named_as("name"),
        (Grammar::Rhai, "ExprDeclareVar") => named_as("name"),
        (Grammar::Rhai, "FnDeclName" | "Param" | "PatIdent" | "PatTuple" | "ExprImport") => true,
        _ => false,
    }
}

/// Unwraps the `Stmt`, `Item` and `Expr` nodes Rhai puts around a statement.
fn statement_core(mut node: Node) -> Node {
    while matches!(node.kind(), "Stmt" | "Item" | "Expr")
        && let Some(inner) = node.named_child(0)
    {
        node = inner;
    }
    node
}

fn rhai_fn_name(function: Node) -> Option<Node> {
    child_of_kind(function, "FnDeclName").and_then(|name| child_of_kind(name, "ident"))
}

fn rhai_params_of(function: Node, source: &str) -> Vec<String> {
    child_of_kind(function, "ParamList")
        .map(|list| {
            named(list)
                .filter_map(|param| child_of_kind(param, "ident"))
                .map(|name| text(source, name).to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn luau_params(function: Node, source: &str) -> Vec<String> {
    function
        .child_by_field_name("parameters")
        .map(|parameters| {
            named(parameters)
                .filter(|p| p.kind() == "parameter")
                .filter_map(|p| p.named_child(0))
                .map(|name| text(source, name).to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn text<'s>(source: &'s str, node: Node) -> &'s str {
    source.get(node.byte_range()).unwrap_or_default()
}

fn named(node: Node) -> impl Iterator<Item = Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .collect::<Vec<_>>()
        .into_iter()
}

fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    named(node).find(|child| child.kind() == kind)
}

/// The descendants of `kind` within `depth` levels below `node`.
fn descendants_of_kind<'t>(node: Node<'t>, kind: &str, depth: usize) -> Vec<Node<'t>> {
    let mut found = Vec::new();
    let mut pending = vec![(node, 0usize)];
    while let Some((current, level)) = pending.pop() {
        if level > 0 && current.kind() == kind {
            found.push(current);
            continue;
        }
        if level < depth {
            pending.extend(named(current).map(|child| (child, level + 1)));
        }
    }
    found.sort_by_key(Node::start_byte);
    found
}

/// The children of `node` paired with the field each one fills.
fn fields<'t>(node: Node<'t>) -> Vec<(Node<'t>, Option<&'t str>)> {
    let mut cursor = node.walk();
    let mut children = Vec::with_capacity(node.child_count() as usize);
    if cursor.goto_first_child() {
        loop {
            children.push((cursor.node(), cursor.field_name()));
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    children
}

mod symbols;

pub use symbols::{Export, Exports, Import, Symbol, SymbolKind};

#[cfg(test)]
mod tests;
