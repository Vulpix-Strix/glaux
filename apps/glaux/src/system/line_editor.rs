//! Single-line text editor used by the shell prompt.

use std::ops::Range;

use glaux_input::InputEvent;

/// An editable line with a cursor and an optional selection.
///
/// Text is stored as `char`s, so every position is a character index and
/// any position in `0..=len` is a valid UTF-8 boundary. Each `char` is one
/// logical position; grapheme clusters are not merged.
///
/// Invariants:
/// - `cursor <= chars.len()` and `anchor <= chars.len()`;
/// - `anchor` is never equal to `cursor`: an empty selection is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct LineEditor {
    chars: Vec<char>,
    cursor: usize,
    /// The fixed end of the selection; the cursor is the moving end.
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

    /// Applies `event`. Returns the line when it is submitted with Enter,
    /// leaving the editor empty; an empty line is still submitted.
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
            InputEvent::Up
            | InputEvent::Down
            | InputEvent::Tab
            | InputEvent::Break
            | InputEvent::ButtonDown(_)
            | InputEvent::ButtonUp(_)
            | InputEvent::KeyDown(_)
            | InputEvent::KeyUp(_) => {}
        }
        None
    }

    /// Moves the cursor to `pos`, clamped to the line. With `select`, the
    /// selection is anchored at the old cursor if there was none; without
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
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    use InputEvent::{Backspace, Delete, Enter, Text};

    const LEFT: InputEvent = InputEvent::Left { select: false };
    const RIGHT: InputEvent = InputEvent::Right { select: false };
    const HOME: InputEvent = InputEvent::Home { select: false };
    const END: InputEvent = InputEvent::End { select: false };
    const SHIFT_LEFT: InputEvent = InputEvent::Left { select: true };
    const SHIFT_RIGHT: InputEvent = InputEvent::Right { select: true };
    const SHIFT_HOME: InputEvent = InputEvent::Home { select: true };
    const SHIFT_END: InputEvent = InputEvent::End { select: true };

    fn typed(text: &str) -> LineEditor {
        let mut editor = LineEditor::default();
        type_text(&mut editor, text);
        editor
    }

    fn type_text(editor: &mut LineEditor, text: &str) {
        for ch in text.chars() {
            assert_eq!(editor.apply(Text(ch)), None);
        }
    }

    fn apply_all(editor: &mut LineEditor, events: &[InputEvent]) {
        for &event in events {
            assert_eq!(editor.apply(event), None);
        }
    }

    fn text(editor: &LineEditor) -> String {
        editor.chars().iter().collect()
    }

    /// Text, cursor and selection in one comparable value.
    fn state(editor: &LineEditor) -> (String, usize, Option<Range<usize>>) {
        (text(editor), editor.cursor(), editor.selection())
    }

    #[test]
    fn starts_empty() {
        assert_eq!(state(&LineEditor::default()), (String::new(), 0, None));
    }

    #[test]
    fn inserts_at_end() {
        assert_eq!(state(&typed("abc")), ("abc".into(), 3, None));
    }

    #[test]
    fn inserts_at_beginning() {
        let mut editor = typed("bc");
        apply_all(&mut editor, &[HOME]);
        type_text(&mut editor, "a");
        assert_eq!(state(&editor), ("abc".into(), 1, None));
    }

    #[test]
    fn inserts_in_middle() {
        let mut editor = typed("ac");
        apply_all(&mut editor, &[LEFT]);
        type_text(&mut editor, "b");
        assert_eq!(state(&editor), ("abc".into(), 2, None));
    }

    #[test]
    fn ignores_control_characters() {
        let mut editor = typed("a");
        type_text(&mut editor, "\n\r\t\u{7f}\u{1b}");
        assert_eq!(state(&editor), ("a".into(), 1, None));
    }

    #[test]
    fn left_and_right_stop_at_boundaries() {
        let mut editor = typed("ab");
        apply_all(&mut editor, &[RIGHT]);
        assert_eq!(editor.cursor(), 2);
        apply_all(&mut editor, &[LEFT, LEFT, LEFT, LEFT]);
        assert_eq!(editor.cursor(), 0);
        apply_all(&mut editor, &[RIGHT]);
        assert_eq!(editor.cursor(), 1);
    }

    #[test]
    fn movement_on_empty_line_stays_at_zero() {
        let mut editor = LineEditor::default();
        apply_all(
            &mut editor,
            &[LEFT, RIGHT, HOME, END, SHIFT_LEFT, SHIFT_RIGHT, SHIFT_END],
        );
        assert_eq!(state(&editor), (String::new(), 0, None));
    }

    #[test]
    fn home_and_end_jump_to_line_boundaries() {
        let mut editor = typed("hello");
        apply_all(&mut editor, &[LEFT, LEFT, HOME]);
        assert_eq!(editor.cursor(), 0);
        apply_all(&mut editor, &[END]);
        assert_eq!(editor.cursor(), 5);
    }

    #[test]
    fn backspace_removes_character_before_cursor() {
        let mut editor = typed("abc");
        apply_all(&mut editor, &[LEFT, Backspace]);
        assert_eq!(state(&editor), ("ac".into(), 1, None));
    }

    #[test]
    fn backspace_at_start_does_nothing() {
        let mut editor = typed("abc");
        apply_all(&mut editor, &[HOME, Backspace]);
        assert_eq!(state(&editor), ("abc".into(), 0, None));
    }

    #[test]
    fn delete_removes_character_at_cursor() {
        let mut editor = typed("abc");
        apply_all(&mut editor, &[HOME, Delete]);
        assert_eq!(state(&editor), ("bc".into(), 0, None));
    }

    #[test]
    fn delete_at_end_does_nothing() {
        let mut editor = typed("abc");
        apply_all(&mut editor, &[Delete]);
        assert_eq!(state(&editor), ("abc".into(), 3, None));
    }

    #[test]
    fn shift_left_extends_selection_from_anchor() {
        let mut editor = typed("abcd");
        apply_all(&mut editor, &[SHIFT_LEFT, SHIFT_LEFT]);
        assert_eq!(state(&editor), ("abcd".into(), 2, Some(2..4)));
    }

    #[test]
    fn shift_right_shrinks_selection_back_to_none() {
        let mut editor = typed("abcd");
        apply_all(&mut editor, &[SHIFT_LEFT, SHIFT_LEFT, SHIFT_RIGHT]);
        assert_eq!(editor.selection(), Some(3..4));
        apply_all(&mut editor, &[SHIFT_RIGHT]);
        assert_eq!(state(&editor), ("abcd".into(), 4, None));
    }

    #[test]
    fn selection_passes_through_its_anchor() {
        let mut editor = typed("abcd");
        apply_all(
            &mut editor,
            &[LEFT, LEFT, SHIFT_LEFT, SHIFT_RIGHT, SHIFT_RIGHT],
        );
        assert_eq!(state(&editor), ("abcd".into(), 3, Some(2..3)));
    }

    #[test]
    fn left_collapses_selection_to_its_start() {
        let mut editor = typed("abcd");
        apply_all(&mut editor, &[HOME, RIGHT, SHIFT_RIGHT, SHIFT_RIGHT, LEFT]);
        assert_eq!(state(&editor), ("abcd".into(), 1, None));
    }

    #[test]
    fn right_collapses_selection_to_its_end() {
        let mut editor = typed("abcd");
        apply_all(&mut editor, &[LEFT, SHIFT_LEFT, SHIFT_LEFT, RIGHT]);
        assert_eq!(state(&editor), ("abcd".into(), 3, None));
    }

    #[test]
    fn home_and_end_without_shift_drop_selection() {
        let mut editor = typed("abcd");
        apply_all(&mut editor, &[SHIFT_LEFT, HOME]);
        assert_eq!(state(&editor), ("abcd".into(), 0, None));
        apply_all(&mut editor, &[SHIFT_RIGHT, END]);
        assert_eq!(state(&editor), ("abcd".into(), 4, None));
    }

    #[test]
    fn shift_home_and_end_select_to_line_boundaries() {
        let mut editor = typed("abcd");
        apply_all(&mut editor, &[LEFT, SHIFT_HOME]);
        assert_eq!(state(&editor), ("abcd".into(), 0, Some(0..3)));
        apply_all(&mut editor, &[SHIFT_END]);
        assert_eq!(state(&editor), ("abcd".into(), 4, Some(3..4)));
    }

    #[test]
    fn typing_replaces_selection() {
        let mut editor = typed("abcd");
        apply_all(&mut editor, &[LEFT, SHIFT_LEFT, SHIFT_LEFT]);
        type_text(&mut editor, "X");
        assert_eq!(state(&editor), ("aXd".into(), 2, None));
    }

    #[test]
    fn backspace_deletes_selection_only() {
        let mut editor = typed("abcd");
        apply_all(&mut editor, &[HOME, SHIFT_RIGHT, SHIFT_RIGHT, Backspace]);
        assert_eq!(state(&editor), ("cd".into(), 0, None));
    }

    #[test]
    fn delete_deletes_selection_only() {
        let mut editor = typed("abcd");
        apply_all(&mut editor, &[SHIFT_LEFT, SHIFT_LEFT, Delete]);
        assert_eq!(state(&editor), ("ab".into(), 2, None));
    }

    #[test]
    fn enter_submits_line_and_resets() {
        let mut editor = typed("run");
        apply_all(&mut editor, &[SHIFT_LEFT]);
        assert_eq!(editor.apply(Enter), Some("run".into()));
        assert_eq!(editor, LineEditor::default());
    }

    #[test]
    fn enter_on_empty_line_submits_empty_string() {
        let mut editor = LineEditor::default();
        assert_eq!(editor.apply(Enter), Some(String::new()));
        assert_eq!(editor, LineEditor::default());
    }

    #[test]
    fn set_text_replaces_line_and_moves_cursor_to_end() {
        let mut editor = typed("abcd");
        apply_all(&mut editor, &[HOME, SHIFT_RIGHT]);
        editor.set_text("xy");
        assert_eq!(state(&editor), ("xy".into(), 2, None));
        assert_eq!(editor.text(), "xy");
    }

    #[test]
    fn replace_splices_range_and_places_cursor_after_it() {
        let mut editor = typed("he foo");
        apply_all(&mut editor, &[HOME, SHIFT_RIGHT]);
        editor.replace(0..2, "help");
        assert_eq!(state(&editor), ("help foo".into(), 4, None));

        editor.replace(8..20, "!");
        assert_eq!(state(&editor), ("help foo!".into(), 9, None));
    }

    #[test]
    fn navigation_events_do_not_edit_the_line() {
        let mut editor = typed("abc");
        apply_all(
            &mut editor,
            &[InputEvent::Up, InputEvent::Down, InputEvent::Tab],
        );
        assert_eq!(state(&editor), ("abc".into(), 3, None));
    }

    #[test]
    fn non_ascii_characters_are_single_positions() {
        let mut editor = typed("aé日🦉b");
        assert_eq!(editor.cursor(), 5);

        apply_all(&mut editor, &[LEFT, LEFT, Backspace]);
        assert_eq!(state(&editor), ("aé🦉b".into(), 2, None));

        apply_all(&mut editor, &[Delete]);
        assert_eq!(state(&editor), ("aéb".into(), 2, None));

        type_text(&mut editor, "ñ");
        apply_all(&mut editor, &[SHIFT_LEFT, SHIFT_LEFT, SHIFT_LEFT]);
        assert_eq!(state(&editor), ("aéñb".into(), 0, Some(0..3)));

        type_text(&mut editor, "ü");
        assert_eq!(state(&editor), ("üb".into(), 1, None));
        assert_eq!(editor.apply(Enter), Some("üb".into()));
    }

    #[test]
    fn submitted_line_preserves_unicode() {
        let line = "αβγ — 日本語 🦉 é\u{301}";
        let mut editor = typed(line);
        assert_eq!(editor.cursor(), line.chars().count());
        assert_eq!(editor.apply(Enter).as_deref(), Some(line));
    }
}
