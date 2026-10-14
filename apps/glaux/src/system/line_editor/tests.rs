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
        &[
            InputEvent::Up { select: false },
            InputEvent::Down { select: false },
            InputEvent::Tab { reverse: false },
        ],
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
fn select_all_then_cut_empties_the_line_and_returns_it() {
    let mut editor = typed("aé日");
    editor.select_all();
    assert_eq!(state(&editor), ("aé日".into(), 3, Some(0..3)));
    assert_eq!(editor.cut_selection(), Some("aé日".into()));
    assert_eq!(state(&editor), (String::new(), 0, None));
    // Nothing is selected now, so nothing more is cut.
    assert_eq!(editor.cut_selection(), None);
}

#[test]
fn selected_text_leaves_the_line_unchanged() {
    let mut editor = typed("hello");
    apply_all(&mut editor, &[SHIFT_LEFT, SHIFT_LEFT]);
    assert_eq!(editor.selected_text(), Some("lo".into()));
    assert_eq!(state(&editor), ("hello".into(), 3, Some(3..5)));
    apply_all(&mut editor, &[END]);
    assert_eq!(editor.selected_text(), None);
}

#[test]
fn insert_str_replaces_the_selection_and_drops_control_characters() {
    let mut editor = typed("one two");
    apply_all(&mut editor, &[SHIFT_LEFT, SHIFT_LEFT, SHIFT_LEFT]);
    editor.insert_str("3\t4\n");
    assert_eq!(state(&editor), ("one 34".into(), 6, None));
    apply_all(&mut editor, &[HOME]);
    editor.insert_str(">");
    assert_eq!(state(&editor), (">one 34".into(), 1, None));
}
