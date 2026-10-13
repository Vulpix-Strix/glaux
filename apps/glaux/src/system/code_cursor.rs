//! The text caret of a code document and how keys move it.
//!
//! Positions are byte offsets into the document's unaltered text, the same
//! offsets [`Cell::byte`](super::code_text::Cell::byte) names, and always
//! sit on a character boundary within a line's content: never inside a
//! byte order mark, between a `\r` and its `\n`, or inside a multi-byte
//! character. How a position shows on screen is left to
//! [`code_text`](super::code_text).

use std::ops::Range;

use super::code_document::CodeDocument;
use super::code_text::{TextMetrics, spot, visual_rows};

/// Where the caret sits in a document, and the column vertical moves aim
/// for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Caret {
    byte: usize,
    /// Column within its row the caret had when a run of vertical moves
    /// began, so passing through shorter lines doesn't lose it. `None`
    /// until the caret first moves vertically after any other move.
    preferred_column: Option<usize>,
}

impl Caret {
    /// A caret at `byte`, moved to the nearest valid position of
    /// `document`.
    pub fn at(document: &CodeDocument, byte: usize) -> Self {
        Self {
            byte: valid(document, byte),
            preferred_column: None,
        }
    }

    /// Offset in the document's text.
    pub fn byte(&self) -> usize {
        self.byte
    }

    #[cfg(test)]
    pub fn preferred_column(&self) -> Option<usize> {
        self.preferred_column
    }

    /// The caret at its nearest valid position of `document`, with no
    /// preferred column, for when columns change meaning.
    pub fn reset(self, document: &CodeDocument) -> Self {
        Self::at(document, self.byte)
    }

    /// The caret moved one character left, to the previous line's end from
    /// the start of a line.
    pub fn left(self, document: &CodeDocument) -> Self {
        Self::at(document, left(document, self.byte))
    }

    /// The caret moved one character right, to the next line's start from
    /// the end of a line.
    pub fn right(self, document: &CodeDocument) -> Self {
        Self::at(document, right(document, self.byte))
    }

    /// The caret moved to the start of the word before it: past any
    /// blanks, then back over a run of word characters or of punctuation.
    /// From the start of a line it moves to the previous line's end.
    pub fn word_left(self, document: &CodeDocument) -> Self {
        let byte = valid(document, self.byte);
        let range = line_range(document, byte);
        if byte == range.start {
            return self.left(document);
        }
        let text = &document.text()[range.start..byte];
        let mut chars = text.char_indices().rev().peekable();
        while chars.next_if(|&(_, ch)| is_blank(ch)).is_some() {}
        let run = chars.peek().map(|&(_, ch)| class(ch));
        // Only blanks before the caret leave it at the line's start.
        let mut start = 0;
        while let Some((at, _)) = chars.next_if(|&(_, ch)| Some(class(ch)) == run) {
            start = at;
        }
        Self::at(document, range.start + start)
    }

    /// The caret moved past the end of the word after it: past any
    /// blanks, then over a run of word characters or of punctuation. From
    /// the end of a line it moves to the next line's start.
    pub fn word_right(self, document: &CodeDocument) -> Self {
        let byte = valid(document, self.byte);
        let range = line_range(document, byte);
        if byte == range.end {
            return self.right(document);
        }
        let text = &document.text()[byte..range.end];
        let mut chars = text.char_indices().peekable();
        while chars.next_if(|&(_, ch)| is_blank(ch)).is_some() {}
        let run = chars.peek().map(|&(_, ch)| class(ch));
        while chars.next_if(|&(_, ch)| Some(class(ch)) == run).is_some() {}
        let end = chars.peek().map_or(text.len(), |&(at, _)| at);
        Self::at(document, byte + end)
    }

    /// The caret moved to its line's first non-blank character, or to the
    /// line's start when it is already there.
    pub fn home(self, document: &CodeDocument) -> Self {
        let byte = valid(document, self.byte);
        let range = line_range(document, byte);
        let text = &document.text()[range.clone()];
        let indent = range.start + text.len() - text.trim_start_matches(is_blank).len();
        Self::at(document, if byte == indent { range.start } else { indent })
    }

    /// The caret moved to its line's end.
    pub fn end(self, document: &CodeDocument) -> Self {
        let byte = valid(document, self.byte);
        Self::at(document, line_range(document, byte).end)
    }

    /// The caret at the start of the first line, after any byte order
    /// mark.
    pub fn document_start(document: &CodeDocument) -> Self {
        Self::at(document, 0)
    }

    /// The caret at the end of the last line.
    pub fn document_end(document: &CodeDocument) -> Self {
        Self::at(document, document.text().len())
    }

    /// The caret moved `rows` rows of the view down, or up when negative,
    /// keeping the column it had when vertical moves began.
    ///
    /// `metrics` must describe `document` as the view lays it out. Moving
    /// up from the first row goes to the document's start, and down from
    /// the last row to its end; a move that would pass either stops on
    /// that row.
    pub fn vertical(self, document: &CodeDocument, rows: isize, metrics: &TextMetrics) -> Self {
        let key = metrics.key();
        let here = spot(document, valid(document, self.byte), key);
        let column = self
            .preferred_column
            .unwrap_or(here.column - here.row_start.column);
        let index = metrics.row_index(here.line, here.row_in_line);
        let last = metrics.rows().saturating_sub(1);
        if rows < 0 && index == 0 {
            return Self::document_start(document);
        }
        if rows > 0 && index >= last {
            return Self::document_end(document);
        }
        let (line, row) = metrics.position(index.saturating_add_signed(rows).min(last));
        let Some(target) = visual_rows(document, line, row, key.wrap_columns, key.tab_size).next()
        else {
            return self;
        };
        Self {
            byte: target.byte_near_column(target.start.column + column, key.tab_size),
            preferred_column: Some(column),
        }
    }
}

/// The nearest valid caret position to `byte`: inside the content of the
/// line holding it, and on a character boundary at or before it.
fn valid(document: &CodeDocument, byte: usize) -> usize {
    let range = line_range(document, byte);
    let text = document.text();
    let mut byte = byte.clamp(range.start, range.end);
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// Content range of the line holding `byte`.
fn line_range(document: &CodeDocument, byte: usize) -> Range<usize> {
    // Every line below the document's line count has a range.
    document.line_range(document.line_of(byte)).unwrap_or(0..0)
}

fn left(document: &CodeDocument, byte: usize) -> usize {
    let byte = valid(document, byte);
    let line = document.line_of(byte);
    let range = line_range(document, byte);
    match document.text()[range.start..byte].chars().next_back() {
        Some(ch) => byte - ch.len_utf8(),
        None if line > 0 => document.line_range(line - 1).map_or(byte, |prev| prev.end),
        None => byte,
    }
}

fn right(document: &CodeDocument, byte: usize) -> usize {
    let byte = valid(document, byte);
    let line = document.line_of(byte);
    let range = line_range(document, byte);
    match document.text()[byte..range.end].chars().next() {
        Some(ch) => byte + ch.len_utf8(),
        None => document
            .line_range(line + 1)
            .map_or(byte, |next| next.start),
    }
}

fn is_blank(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

/// Kinds of character a word move passes over as one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Word,
    Blank,
    Punctuation,
}

fn class(ch: char) -> Class {
    if ch.is_alphanumeric() || ch == '_' {
        Class::Word
    } else if is_blank(ch) {
        Class::Blank
    } else {
        Class::Punctuation
    }
}

#[cfg(test)]
mod tests;
