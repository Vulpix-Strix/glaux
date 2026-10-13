//! The selected text of a code document: a fixed anchor and the caret,
//! which is the end that moves.
//!
//! Both ends are caret positions, so a selection always starts and ends on
//! character boundaries within lines' content or at a line's start; see
//! [`code_cursor`](super::code_cursor).

use std::ops::Range;

use super::code_cursor::Caret;
use super::code_document::CodeDocument;

/// The selected range of a document, empty while the anchor and the caret
/// meet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    anchor: usize,
    caret: Caret,
}

impl Selection {
    /// Nothing selected, with the caret at `caret`.
    pub fn collapsed(caret: Caret) -> Self {
        Self {
            anchor: caret.byte(),
            caret,
        }
    }

    /// The text from `anchor` to `caret`, `anchor` moved to the nearest
    /// valid position of `document`.
    pub fn between(document: &CodeDocument, anchor: usize, caret: Caret) -> Self {
        Self {
            anchor: Caret::at(document, anchor).byte(),
            caret,
        }
    }

    /// The whole of `document`, the caret at its end.
    pub fn all(document: &CodeDocument) -> Self {
        Self {
            anchor: Caret::document_start(document).byte(),
            caret: Caret::document_end(document),
        }
    }

    pub fn caret(&self) -> Caret {
        self.caret
    }

    /// The end that stays put while the selection is extended.
    pub fn anchor(&self) -> usize {
        self.anchor
    }

    /// The selected bytes, from whichever end comes first.
    pub fn range(&self) -> Range<usize> {
        let caret = self.caret.byte();
        self.anchor.min(caret)..self.anchor.max(caret)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.caret.byte()
    }

    /// The selection with its caret moved to `caret`. With `select` the
    /// anchor stays, so the selection grows or shrinks toward the caret
    /// from either side; without it nothing stays selected.
    pub fn moved(self, caret: Caret, select: bool) -> Self {
        if select {
            Self { caret, ..self }
        } else {
            Self::collapsed(caret)
        }
    }

    /// The selection with its caret moved by `motion`, as
    /// [`moved`](Self::moved). Without `select`, a selection collapses to
    /// its `edge` end instead of moving, as a single step left or right
    /// does on desktop systems.
    pub fn stepped(
        self,
        document: &CodeDocument,
        select: bool,
        edge: Edge,
        motion: impl FnOnce(Caret) -> Caret,
    ) -> Self {
        if !select && !self.is_empty() {
            let range = self.range();
            let byte = match edge {
                Edge::Start => range.start,
                Edge::End => range.end,
            };
            return Self::collapsed(Caret::at(document, byte));
        }
        self.moved(motion(self.caret), select)
    }

    /// The selection checked against `document`: both ends moved to their
    /// nearest valid positions, and the caret without a preferred column.
    pub fn reset(self, document: &CodeDocument) -> Self {
        Self::between(document, self.anchor, self.caret.reset(document))
    }
}

/// Which end of a selection a step collapses it to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

#[cfg(test)]
mod tests;
