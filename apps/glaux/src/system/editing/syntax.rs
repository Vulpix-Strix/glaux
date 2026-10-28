//! Syntax highlighting state for one open document.

use std::ops::Range;

use glaux_syntax::{Grammar, Highlighter, Span};

use super::document::CodeDocument;
use crate::cartridge::Language;

/// What [`Highlighter`] knows about a document and the spans of the part of it
/// on screen.
///
/// The tree is brought up to date when the document's revision changes. The
/// spans are recomputed when the revision or the visible window changes.
#[derive(Debug)]
pub struct DocumentSyntax {
    language: Language,
    /// `None` when the grammar could not be set up, which leaves the text plain.
    highlighter: Option<Highlighter>,
    parsed: Option<u64>,
    window: Range<usize>,
    spans_revision: Option<u64>,
    spans: Vec<Span>,
}

impl DocumentSyntax {
    pub fn new(language: Language) -> Self {
        let highlighter = match Highlighter::new(grammar(language)) {
            Ok(highlighter) => Some(highlighter),
            Err(err) => {
                tracing::error!(error = %err, ?language, "syntax highlighting unavailable");
                None
            }
        };
        Self {
            language,
            highlighter,
            parsed: None,
            window: 0..0,
            spans_revision: None,
            spans: Vec::new(),
        }
    }

    /// Updates the spans for `window`, a byte range of `document`'s text,
    /// reparsing first if the text changed since the last call.
    #[hotpath::measure(label = "editing::syntax_refresh", impl_type = "DocumentSyntax")]
    pub fn refresh(&mut self, document: &CodeDocument, window: Range<usize>) {
        if document.language() != self.language {
            *self = Self::new(document.language());
        }
        let Some(highlighter) = &mut self.highlighter else {
            return;
        };
        let revision = document.revision();
        if self.parsed != Some(revision) {
            highlighter.update(document.text());
            self.parsed = Some(revision);
        }
        if self.spans_revision != Some(revision) || self.window != window {
            self.spans = highlighter.spans(window.clone());
            self.spans_revision = Some(revision);
            self.window = window;
        }
    }

    /// The tree brought up to date with `document`, reparsing only if the
    /// text changed. `None` when the grammar could not be set up.
    pub fn parsed(&mut self, document: &CodeDocument) -> Option<&Highlighter> {
        if document.language() != self.language {
            *self = Self::new(document.language());
        }
        let highlighter = self.highlighter.as_mut()?;
        let revision = document.revision();
        if self.parsed != Some(revision) {
            highlighter.update(document.text());
            self.parsed = Some(revision);
        }
        Some(highlighter)
    }

    /// The spans last computed, sorted and apart. Text between them is plain.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }
}

fn grammar(language: Language) -> Grammar {
    match language {
        Language::Luau => Grammar::Luau,
        Language::Rhai => Grammar::Rhai,
    }
}

#[cfg(test)]
mod tests;
