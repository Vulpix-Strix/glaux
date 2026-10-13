//! What the editing keys do to a code document: each works out the one
//! replacement it makes, which [`CodeDocument::replace`] then applies.
//!
//! Ranges come from selections and caret moves, so their ends are always
//! valid caret positions; see [`code_cursor`](super::code_cursor).

use std::ops::Range;

use glaux_machine::font::BitmapFont;

use super::code_document::{CodeDocument, Indent};
use super::code_selection::Selection;
use super::code_text::{MetricsKey, spot};

/// One replacement of a document's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

impl Edit {
    /// The selection replaced with `text`.
    fn over(selection: Selection, text: impl Into<String>) -> Self {
        Self {
            range: selection.range(),
            text: text.into(),
        }
    }
}

/// Typing `ch` over the selection. Control characters type nothing, since
/// the keys that insert line endings and indentation have their own
/// edits.
pub fn typed(selection: Selection, ch: char) -> Option<Edit> {
    (!ch.is_control()).then(|| Edit::over(selection, ch))
}

/// A line break over the selection, in the document's own line ending.
pub fn newline(document: &CodeDocument, selection: Selection) -> Edit {
    Edit::over(selection, document.line_ending().as_str())
}

/// Backspace: deletes the selection, or else the character before the
/// caret, joining the line to the one above from its start. `None` when
/// there is nothing to delete.
///
/// A character is one Unicode scalar value, not what a reader sees as one:
/// `e` followed by a combining accent takes two presses.
pub fn backspace(document: &CodeDocument, selection: Selection) -> Option<Edit> {
    let caret = selection.caret();
    let range = if selection.is_empty() {
        caret.left(document).byte()..caret.byte()
    } else {
        selection.range()
    };
    deletion(range)
}

/// Delete: deletes the selection, or else the character after the caret,
/// joining the next line to this one from its end. `None` when there is
/// nothing to delete. Characters are counted as in [`backspace`].
pub fn delete(document: &CodeDocument, selection: Selection) -> Option<Edit> {
    let caret = selection.caret();
    let range = if selection.is_empty() {
        caret.byte()..caret.right(document).byte()
    } else {
        selection.range()
    };
    deletion(range)
}

fn deletion(range: Range<usize>) -> Option<Edit> {
    (!range.is_empty()).then(|| Edit {
        range,
        text: String::new(),
    })
}

/// Tab over the selection, indenting as the document does: a tab
/// character, or spaces up to the next tab stop of a view measuring with
/// `key` and `font`.
pub fn tab(
    document: &CodeDocument,
    selection: Selection,
    key: MetricsKey,
    font: &BitmapFont,
) -> Edit {
    match document.indent() {
        Indent::Tabs => Edit::over(selection, "\t"),
        Indent::Spaces => {
            let tab_size = key.tab_size.max(1);
            let column = spot(document, selection.range().start, key, font).column;
            Edit::over(selection, " ".repeat(tab_size - column % tab_size))
        }
    }
}

#[cfg(test)]
mod tests;
