//! The Markdown constructs a viewer draws, built from the tree that
//! [`Page::new`](super::Page::new) already parses.
//!
//! The model keeps only what a reader sees. It carries no widths, colors or
//! source positions, so every viewer can lay it out its own way.

use comrak::nodes::{AlertType, AstNode, ListType, NodeValue};

use super::{Scope, Section};
use crate::ScriptLanguage;

/// A top-level block and the language it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// [`Scope::Language`] inside a `Luau` or `Rhai` subsection and for a
    /// fenced block tagged with a language name, [`Scope::Common`]
    /// otherwise.
    pub scope: Scope,
    pub kind: BlockKind,
}

/// A piece of the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Heading {
        level: u8,
        inlines: Vec<Inline>,
    },
    Paragraph(Vec<Inline>),
    /// A code block, one string per line without its line ending.
    Code {
        /// The first word of the info string, such as `luau`.
        lang: Option<String>,
        lines: Vec<String>,
    },
    List {
        ordered: bool,
        /// The number of the first item of an ordered list.
        start: usize,
        items: Vec<Vec<BlockKind>>,
    },
    Table {
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Alert {
        kind: AlertKind,
        /// The title written after the marker, when it replaces the kind's
        /// own name.
        title: Option<String>,
        blocks: Vec<BlockKind>,
    },
    Quote(Vec<BlockKind>),
    Rule,
}

/// Which kind of callout an alert is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

/// A piece of text inside a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Link {
        target: String,
        children: Vec<Inline>,
    },
    Strong(Vec<Inline>),
    Emph(Vec<Inline>),
    /// A hard line break. A soft break is a space in [`Text`](Self::Text).
    Break,
}

/// The blocks of the document under `root`. `outline` is the page's
/// headings in order, which give each block its scope.
pub(super) fn build<'a>(root: &'a AstNode<'a>, outline: &[Section]) -> Vec<Block> {
    let mut headings = outline.iter();
    let mut scope = Scope::Common;
    let mut blocks = Vec::new();
    for node in root.children() {
        let kinds = block_kinds(node);
        if matches!(node.data().value, NodeValue::Heading(_))
            && let Some(section) = headings.next()
        {
            scope = section.scope;
        }
        for kind in kinds {
            let scope = match (&kind, scope) {
                (BlockKind::Code { lang, .. }, Scope::Common) => lang
                    .as_deref()
                    .and_then(language)
                    .map_or(scope, Scope::Language),
                _ => scope,
            };
            blocks.push(Block { scope, kind });
        }
    }
    blocks
}

fn language(tag: &str) -> Option<ScriptLanguage> {
    ScriptLanguage::ALL
        .into_iter()
        .find(|language| language.name().eq_ignore_ascii_case(tag))
}

/// The blocks `node` stands for: none for what a viewer does not draw.
fn block_kinds<'a>(node: &'a AstNode<'a>) -> Vec<BlockKind> {
    let data = node.data();
    let kind = match &data.value {
        NodeValue::Heading(heading) => BlockKind::Heading {
            level: heading.level,
            inlines: inlines(node),
        },
        NodeValue::Paragraph => BlockKind::Paragraph(inlines(node)),
        NodeValue::CodeBlock(block) => BlockKind::Code {
            lang: block.info.split_whitespace().next().map(str::to_owned),
            lines: block.literal.lines().map(str::to_owned).collect(),
        },
        NodeValue::List(list) => BlockKind::List {
            ordered: list.list_type == ListType::Ordered,
            start: list.start,
            items: node.children().map(children_kinds).collect(),
        },
        NodeValue::Table(_) => {
            let mut rows = node.children();
            let header = rows.next().map(cells).unwrap_or_default();
            BlockKind::Table {
                header,
                rows: rows.map(cells).collect(),
            }
        }
        NodeValue::Alert(alert) => BlockKind::Alert {
            kind: match alert.alert_type {
                AlertType::Note => AlertKind::Note,
                AlertType::Tip => AlertKind::Tip,
                AlertType::Important => AlertKind::Important,
                AlertType::Warning => AlertKind::Warning,
                AlertType::Caution => AlertKind::Caution,
            },
            title: alert.title.clone(),
            blocks: children_kinds(node),
        },
        NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => {
            BlockKind::Quote(children_kinds(node))
        }
        NodeValue::ThematicBreak => BlockKind::Rule,
        // Raw HTML and footnote definitions are never drawn.
        _ => return Vec::new(),
    };
    vec![kind]
}

fn children_kinds<'a>(node: &'a AstNode<'a>) -> Vec<BlockKind> {
    node.children().flat_map(block_kinds).collect()
}

fn cells<'a>(row: &'a AstNode<'a>) -> Vec<Vec<Inline>> {
    row.children().map(inlines).collect()
}

/// The inline content of `node`.
fn inlines<'a>(node: &'a AstNode<'a>) -> Vec<Inline> {
    let mut out = Vec::new();
    for child in node.children() {
        match &child.data().value {
            NodeValue::Text(text) => push_text(&mut out, text),
            NodeValue::Code(code) => out.push(Inline::Code(code.literal.clone())),
            NodeValue::SoftBreak => push_text(&mut out, " "),
            NodeValue::LineBreak => out.push(Inline::Break),
            NodeValue::Link(link) => out.push(Inline::Link {
                target: link.url.clone(),
                children: inlines(child),
            }),
            NodeValue::Strong => out.push(Inline::Strong(inlines(child))),
            NodeValue::Emph => out.push(Inline::Emph(inlines(child))),
            // An image shows its alt text, which is all a text screen can
            // do with it.
            NodeValue::Image(_) => {
                push_text(&mut out, "[");
                append(&mut out, inlines(child));
                push_text(&mut out, "]");
            }
            // Strikethrough and the like keep their text and lose the
            // styling.
            NodeValue::Strikethrough | NodeValue::Superscript | NodeValue::Subscript => {
                append(&mut out, inlines(child));
            }
            _ => {}
        }
    }
    out
}

/// Appends `more`, joining its first text run to one that ends the list.
fn append(out: &mut Vec<Inline>, more: Vec<Inline>) {
    for inline in more {
        match inline {
            Inline::Text(text) => push_text(out, &text),
            other => out.push(other),
        }
    }
}

/// Appends `text`, joining it to a text run that ends the list.
fn push_text(out: &mut Vec<Inline>, text: &str) {
    if let Some(Inline::Text(last)) = out.last_mut() {
        last.push_str(text);
    } else {
        out.push(Inline::Text(text.to_owned()));
    }
}

/// The plain text of `inlines` on one line.
pub fn plain_text(inlines: &[Inline]) -> String {
    let mut text = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(part) | Inline::Code(part) => text.push_str(part),
            Inline::Link { children, .. } | Inline::Strong(children) | Inline::Emph(children) => {
                text.push_str(&plain_text(children));
            }
            Inline::Break => text.push(' '),
        }
    }
    text
}

#[cfg(test)]
mod tests;
