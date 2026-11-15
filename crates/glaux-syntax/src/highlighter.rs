use std::ops::Range;

use thiserror::Error;
use tree_sitter::{
    LanguageError, Node, Parser, Query, QueryCursor, QueryError, StreamingIterator, Tree,
};

use crate::edit;
use crate::grammar::Grammar;
use crate::kind::SyntaxKind;
use crate::resolve::{Capture, Span, resolve};

#[derive(Debug, Error)]
pub enum HighlighterError {
    #[error("the grammar was built for an incompatible Tree-sitter version")]
    Language(#[from] LanguageError),
    #[error("invalid highlight query")]
    Query(#[from] QueryError),
    #[error("the highlight query uses the unknown capture `@{0}`")]
    UnknownCapture(String),
}

/// Keeps a parse tree of one document up to date and answers highlight queries.
///
/// The highlighter keeps a copy of the text it last parsed. [`Highlighter::update`]
/// compares that copy with the new text to find what changed, so callers never have to
/// describe edits.
pub struct Highlighter {
    grammar: Grammar,
    parser: Parser,
    query: Query,
    kinds: Vec<SyntaxKind>,
    source: String,
    tree: Option<Tree>,
}

impl Highlighter {
    pub fn new(grammar: Grammar) -> Result<Self, HighlighterError> {
        let language = grammar.language();
        let mut parser = Parser::new();
        parser.set_language(&language)?;
        let query = Query::new(&language, grammar.query())?;
        let kinds = query
            .capture_names()
            .iter()
            .map(|name| {
                SyntaxKind::from_capture_name(name)
                    .ok_or_else(|| HighlighterError::UnknownCapture((*name).to_owned()))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            grammar,
            parser,
            query,
            kinds,
            source: String::new(),
            tree: None,
        })
    }

    pub fn grammar(&self) -> Grammar {
        self.grammar
    }

    /// Brings the parse tree in line with `text`, reusing the previous tree.
    ///
    /// The previous tree is reused only when neither it nor the result has a syntax
    /// error. Tree-sitter lexes the tokens of an error region while recovering, and
    /// those tokens depend on the text around them, so reusing them across an error
    /// can give a tree that a fresh parse would not.
    #[hotpath::measure(label = "syntax::highlighter_update")]
    pub fn update(&mut self, text: &str) {
        if self.tree.is_some() && self.source == text {
            return;
        }
        let mut old = self
            .tree
            .take()
            .filter(|tree| !tree.root_node().has_error());
        if let Some(tree) = &mut old {
            tree.edit(&edit::between(&self.source, text));
        }
        self.source.clear();
        self.source.push_str(text);
        let mut tree = self.parser.parse(text, old.as_ref());
        if old.is_some()
            && tree
                .as_ref()
                .is_some_and(|tree| tree.root_node().has_error())
        {
            tree = self.parser.parse(text, None);
        }
        self.tree = tree;
    }

    /// Whether the current tree was parsed from exactly `text`.
    pub fn is_parsed(&self, text: &str) -> bool {
        self.tree.is_some() && self.source == text
    }

    /// The tree root and the text it came from, for crate-internal queries.
    pub(crate) fn parsed(&self) -> Option<(Node<'_>, &str)> {
        self.tree
            .as_ref()
            .map(|tree| (tree.root_node(), self.source.as_str()))
    }

    /// Highlight spans inside `window`, sorted by position and never overlapping.
    ///
    /// Only the nodes that touch `window` are visited. Text without a span is plain.
    #[hotpath::measure(label = "syntax::highlighter_spans")]
    pub fn spans(&self, window: Range<usize>) -> Vec<Span> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(window.clone());
        let mut captures = Vec::new();
        let mut matches = cursor.matches(&self.query, tree.root_node(), self.source.as_bytes());
        while let Some(found) = matches.next() {
            for capture in found.captures() {
                let index = capture.index as usize;
                captures.push(Capture {
                    range: capture.node.byte_range(),
                    kind: self.kinds[index],
                    pattern: found.pattern_index,
                    index,
                });
            }
        }
        resolve(&captures, &window)
    }
}

impl std::fmt::Debug for Highlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Highlighter")
            .field("grammar", &self.grammar)
            .field("source_len", &self.source.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
