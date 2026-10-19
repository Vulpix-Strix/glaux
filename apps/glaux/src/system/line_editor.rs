//! Single-line text editor used by the shell prompt.

use std::ops::Range;

use glaux_input::InputEvent;

/// An editable line with a cursor and an optional selection.
///
/// Text is stored as `char`s, so every position is a character index and
/// any position in `0..=len` is a valid UTF-8 boundary. Each `char` is one
/// logical position. Grapheme clusters are not merged.
///
/// Invariants:
/// - `cursor <= chars.len()` and `anchor <= chars.len()`;
/// - `anchor` is never equal to `cursor`: an empty selection is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct LineEditor {
    chars: Vec<char>,
    cursor: usize,
    /// The fixed end of the selection. The cursor is the moving end.
    anchor: Option<usize>,
}

impl LineEditor {
    pub(super) fn chars(&self) -> &[char] {
        &self.chars
    }

    pub(super) fn text(&self) -> String {
        self.chars.iter().collect()
    }

    /// Replaces the whole line with `text`, leaving the cursor at its end
    /// and nothing selected.
    pub(super) fn set_text(&mut self, text: &str) {
        self.chars = text.chars().collect();
        self.set_cursor(self.chars.len(), false);
    }

    /// Replaces the characters in `range` with `text`, leaving the cursor
    /// just after the inserted text and nothing selected. `range` is
    /// clamped to the line.
    pub(super) fn replace(&mut self, range: Range<usize>, text: &str) {
        let end = range.end.min(self.chars.len());
        let start = range.start.min(end);
        let inserted = text.chars().count();
        self.chars.splice(start..end, text.chars());
        // Drop any anchor first: it may point past the new end of the line.
        self.anchor = None;
        self.set_cursor(start + inserted, false);
    }

    /// Character index where the next typed character is inserted.
    pub(super) fn cursor(&self) -> usize {
        self.cursor
    }

    /// The selected character range, if any. Never empty.
    pub(super) fn selection(&self) -> Option<Range<usize>> {
        self.anchor
            .map(|anchor| anchor.min(self.cursor)..anchor.max(self.cursor))
    }

    /// Selects the whole line, leaving the cursor at its end.
    pub(super) fn select_all(&mut self) {
        self.anchor = None;
        self.cursor = 0;
        self.set_cursor(self.chars.len(), true);
    }

    /// The selected text, `None` when nothing is selected.
    pub(super) fn selected_text(&self) -> Option<String> {
        self.selection()
            .map(|range| self.chars[range].iter().collect())
    }

    /// Removes the selected text and returns it, `None` when nothing is
    /// selected.
    pub(super) fn cut_selection(&mut self) -> Option<String> {
        let text = self.selected_text()?;
        self.delete_selection();
        Some(text)
    }

    /// Replaces the selection, if any, with `text`, leaving the cursor
    /// after it. Control characters are dropped, as when typed.
    pub(super) fn insert_str(&mut self, text: &str) {
        self.delete_selection();
        let inserted: Vec<char> = text.chars().filter(|ch| !ch.is_control()).collect();
        let at = self.cursor;
        let count = inserted.len();
        self.chars.splice(at..at, inserted);
        self.set_cursor(at + count, false);
    }

    /// Applies `event`. Returns the line when it is submitted with Enter,
    /// leaving the editor empty. An empty line is still submitted.
    pub(super) fn apply(&mut self, event: InputEvent) -> Option<String> {
        match event {
            InputEvent::Text(ch) => self.insert(ch),
            InputEvent::Left { select } => match self.selection() {
                Some(range) if !select => self.set_cursor(range.start, false),
                _ => self.set_cursor(self.cursor.saturating_sub(1), select),
            },
            InputEvent::Right { select } => match self.selection() {
                Some(range) if !select => self.set_cursor(range.end, false),
                _ => self.set_cursor(self.cursor + 1, select),
            },
            InputEvent::Home { select } => self.set_cursor(0, select),
            InputEvent::End { select } => self.set_cursor(self.chars.len(), select),
            InputEvent::Backspace => {
                if !self.delete_selection() && self.cursor > 0 {
                    self.chars.remove(self.cursor - 1);
                    self.set_cursor(self.cursor - 1, false);
                }
            }
            InputEvent::Delete => {
                if !self.delete_selection() && self.cursor < self.chars.len() {
                    self.chars.remove(self.cursor);
                }
            }
            InputEvent::Enter => {
                let line = std::mem::take(&mut self.chars).into_iter().collect();
                self.set_cursor(0, false);
                return Some(line);
            }
            // Navigation, completion and break act on the whole system, not
            // the line.
            InputEvent::Up { .. }
            | InputEvent::Down { .. }
            | InputEvent::WordLeft { .. }
            | InputEvent::WordRight { .. }
            | InputEvent::DocumentStart { .. }
            | InputEvent::DocumentEnd { .. }
            | InputEvent::PageUp { .. }
            | InputEvent::PageDown { .. }
            | InputEvent::Tab { .. }
            | InputEvent::Break
            | InputEvent::ButtonDown(_)
            | InputEvent::ButtonUp(_)
            | InputEvent::KeyDown(_)
            | InputEvent::KeyUp(_)
            | InputEvent::NextView
            | InputEvent::PreviousView
            | InputEvent::Undo
            | InputEvent::Redo
            | InputEvent::Save
            | InputEvent::SelectAll
            | InputEvent::OpenFile
            | InputEvent::CloseDocument
            | InputEvent::NextDocument
            | InputEvent::PreviousDocument
            | InputEvent::NewFile
            | InputEvent::Find
            | InputEvent::Replace
            | InputEvent::GoToLine
            | InputEvent::Complete
            | InputEvent::Copy
            | InputEvent::Cut
            | InputEvent::Paste
            | InputEvent::PointerMove(_)
            | InputEvent::PointerDown { .. }
            | InputEvent::PointerUp { .. }
            | InputEvent::PointerCancel
            | InputEvent::Wheel { .. }
            | InputEvent::WheelHorizontal { .. } => {}
        }
        None
    }

    /// Moves the cursor to `pos`, clamped to the line. With `select`, the
    /// selection is anchored at the old cursor if there was none. Without
    /// it, any selection is dropped.
    ///
    /// Every cursor change goes through here so the anchor invariant holds.
    fn set_cursor(&mut self, pos: usize, select: bool) {
        let pos = pos.min(self.chars.len());
        self.anchor = if select {
            Some(self.anchor.unwrap_or(self.cursor)).filter(|&anchor| anchor != pos)
        } else {
            None
        };
        self.cursor = pos;
    }

    /// Removes the selected text, if any, leaving the cursor where it
    /// started. Returns whether there was a selection.
    fn delete_selection(&mut self) -> bool {
        let Some(range) = self.selection() else {
            return false;
        };
        let start = range.start;
        self.chars.drain(range);
        self.set_cursor(start, false);
        true
    }

    /// Replaces the selection (if any) with `ch`. Control characters are
    /// ignored because they have no meaning inside a single line.
    fn insert(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        self.delete_selection();
        self.chars.insert(self.cursor, ch);
        self.set_cursor(self.cursor + 1, false);
    }
}

#[cfg(test)]
mod tests;
