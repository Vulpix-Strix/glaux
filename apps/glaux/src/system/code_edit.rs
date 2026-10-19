//! What the editing keys do to a code document. Each works out the one
//! replacement it makes, which [`CodeDocument::replace`] then applies.
//!
//! Ranges come from selections and caret moves, so their ends are always
//! valid caret positions. See [`code_cursor`](super::code_cursor).

use std::ops::Range;

use glaux_machine::font::BitmapFont;

use super::code_document::{CodeDocument, Indent};
use super::code_selection::Selection;
use super::code_text::{MetricsKey, spot};
use super::editor_settings::IndentStyle;

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

/// One replacement together with the selection it leaves, for edits that
/// leave something other than a caret after the inserted text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reselecting {
    pub edit: Edit,
    /// Where the selection's anchor ends up in the edited text.
    pub anchor: usize,
    /// Where the selection's caret ends up in the edited text.
    pub caret: usize,
}

/// What one indentation step inserts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentUnit {
    Tab,
    /// This many spaces, at least one.
    Spaces(usize),
}

impl IndentUnit {
    /// The unit `style` asks for in `document`. Detecting follows the
    /// style the file was read with and, for spaces, the width of its
    /// steps. `size` columns of spaces stand in where the setting or the
    /// file names no width.
    pub fn of(document: &CodeDocument, style: IndentStyle, size: usize) -> Self {
        let size = size.max(1);
        match style {
            IndentStyle::Tabs => Self::Tab,
            IndentStyle::Spaces => Self::Spaces(size),
            IndentStyle::Detect => match document.indent() {
                Indent::Tabs => Self::Tab,
                Indent::Spaces => Self::Spaces(document.indent_width().unwrap_or(size)),
            },
        }
    }

    /// Columns one step takes, a tab taking `tab_size`.
    fn columns(self, tab_size: usize) -> usize {
        match self {
            Self::Tab => tab_size.max(1),
            Self::Spaces(width) => width.max(1),
        }
    }

    pub fn text(self) -> String {
        match self {
            Self::Tab => "\t".to_owned(),
            Self::Spaces(width) => " ".repeat(width.max(1)),
        }
    }
}

/// Typing `ch` over the selection. Control characters type nothing, since
/// the keys that insert line endings and indentation have their own
/// edits.
pub fn typed(selection: Selection, ch: char) -> Option<Edit> {
    (!ch.is_control()).then(|| Edit::over(selection, ch))
}

/// A line break over the selection, in the document's own line ending,
/// starting the new line with the indentation of the line the selection
/// starts on. Only the indentation before the selection's start carries
/// over, so a break made inside the indentation doesn't double it.
pub fn newline(document: &CodeDocument, selection: Selection) -> Edit {
    let start = selection.range().start;
    let line = document
        .line_range(document.line_of(start))
        .unwrap_or(start..start);
    let before = &document.text()[line.start..start.max(line.start)];
    let indent = &before[..before.len() - before.trim_start_matches(is_blank).len()];
    Edit::over(
        selection,
        format!("{}{indent}", document.line_ending().as_str()),
    )
}

/// The clipboard's `text` pasted over the selection, its line breaks
/// changed to the document's own line ending. `None` for empty text.
pub fn paste(document: &CodeDocument, selection: Selection, text: &str) -> Option<Edit> {
    if text.is_empty() {
        return None;
    }
    let ending = document.line_ending().as_str();
    let mut pasted = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                chars.next_if_eq(&'\n');
                pasted.push_str(ending);
            }
            '\n' => pasted.push_str(ending),
            ch => pasted.push(ch),
        }
    }
    Some(Edit::over(selection, pasted))
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

/// Tab over a selection within one line: `unit`'s tab character, or
/// spaces up to the next multiple of its width in the columns of a view
/// measuring with `key` and `font`.
pub fn tab(
    document: &CodeDocument,
    selection: Selection,
    unit: IndentUnit,
    key: MetricsKey,
    font: &BitmapFont,
) -> Edit {
    match unit {
        IndentUnit::Tab => Edit::over(selection, "\t"),
        IndentUnit::Spaces(width) => {
            let width = width.max(1);
            let column = spot(document, selection.range().start, key, font).column;
            Edit::over(selection, " ".repeat(width - column % width))
        }
    }
}

/// Whether Tab over `selection` indents whole lines rather than inserting
/// indentation: the selection holds a line break.
pub fn spans_lines(document: &CodeDocument, selection: Selection) -> bool {
    document.text()[selection.range()].contains('\n')
}

/// One more `unit` at the start of every non-blank line the selection
/// touches, keeping those lines selected. `None` when every one is blank.
pub fn indent_lines(
    document: &CodeDocument,
    selection: Selection,
    unit: IndentUnit,
) -> Option<Reselecting> {
    let unit = unit.text();
    reindent(document, selection, |line| {
        (!line.trim_matches(is_blank).is_empty()).then(|| (0, unit.clone()))
    })
}

/// One `unit` less at the start of every line the selection touches, or
/// of the caret's line when nothing is selected, keeping the selection on
/// the same text. A line gives up leading blanks up to the unit's columns,
/// with a tab taking `tab_size`. Lines without any are left alone. `None`
/// when no line has any.
pub fn outdent_lines(
    document: &CodeDocument,
    selection: Selection,
    unit: IndentUnit,
    tab_size: usize,
) -> Option<Reselecting> {
    let tab_size = tab_size.max(1);
    let wanted = unit.columns(tab_size);
    reindent(document, selection, |line| {
        let mut columns = 0;
        let mut removed = 0;
        for ch in line.chars() {
            if columns >= wanted {
                break;
            }
            match ch {
                ' ' => columns += 1,
                '\t' => columns += tab_size - columns % tab_size,
                _ => break,
            }
            removed += 1;
        }
        (removed > 0).then(String::new).map(|text| (removed, text))
    })
}

/// Rewrites the start of each line `selection` touches with `change`.
/// `change` gets the line's content and gives how many bytes to remove from
/// its start and what to insert there, or `None` to leave it. The edit
/// spans from the first touched line's start to the last one's end. The
/// selection's ends follow the text they were on, with an end at a changed
/// line's start staying before what is inserted there.
///
/// A selection ending at the very start of a line doesn't touch it.
fn reindent(
    document: &CodeDocument,
    selection: Selection,
    mut change: impl FnMut(&str) -> Option<(usize, String)>,
) -> Option<Reselecting> {
    let text = document.text();
    let range = selection.range();
    let first = document.line_of(range.start);
    let mut last = document.line_of(range.end);
    if last > first
        && document
            .line_range(last)
            .is_some_and(|line| line.start == range.end)
    {
        last -= 1;
    }
    let span_start = document.line_range(first)?.start;
    let span_end = document.line_range(last)?.end;

    // Where each change is, in the original text, and its size.
    let mut changes: Vec<(usize, usize, usize)> = Vec::new();
    let mut rewritten = String::with_capacity(span_end - span_start);
    let mut copied = span_start;
    for line in first..=last {
        let content = document.line_range(line)?;
        let Some((removed, inserted)) = change(&text[content.clone()]) else {
            continue;
        };
        rewritten.push_str(&text[copied..content.start]);
        rewritten.push_str(&inserted);
        copied = content.start + removed;
        changes.push((content.start, removed, inserted.len()));
    }
    if changes.is_empty() {
        return None;
    }
    rewritten.push_str(&text[copied..span_end]);

    let follow = |byte: usize| -> usize {
        let mut moved = byte as isize;
        for &(at, removed, inserted) in &changes {
            if byte <= at {
                continue;
            }
            moved += if byte < at + removed {
                (at + inserted) as isize - byte as isize
            } else {
                inserted as isize - removed as isize
            };
        }
        moved.max(0) as usize
    };
    Some(Reselecting {
        edit: Edit {
            range: span_start..span_end,
            text: rewritten,
        },
        anchor: follow(selection.anchor()),
        caret: follow(selection.caret().byte()),
    })
}

fn is_blank(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

#[cfg(test)]
mod tests;
