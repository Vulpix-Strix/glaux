//! Editing text in the Code view with the keyboard.

use super::selection::{
    caret, cell, click, down, geometry, left, open, plain, right, select, selected,
};
use super::*;
use crate::system::code_text::spot;
use pretty_assertions::assert_eq;

fn text(session: &CodeSession) -> &str {
    session.active_document().unwrap().text()
}

fn view(session: &CodeSession) -> DocumentView {
    let id = session.tabs().active().unwrap();
    *session.tabs().view(id).unwrap()
}

fn metrics(session: &CodeSession) -> &TextMetrics {
    let id = session.tabs().active().unwrap();
    session.tabs().metrics(id).unwrap()
}

/// Whether the caret shows in the viewport, with measurements that
/// describe the document as it now is.
fn caret_shown(editor: &CodeEditor, session: &CodeSession) -> bool {
    let document = session.active_document().unwrap();
    let geometry = editor.geometry(document);
    let metrics = metrics(session);
    assert!(metrics.describes(document, geometry.metrics_key()));
    let here = spot(
        document,
        caret(session),
        metrics.key(),
        &editor.layout().font,
    );
    geometry
        .caret_point(view(session), &here, metrics)
        .is_some()
}

#[test]
fn typing_inserts_at_the_caret_and_leaves_it_after_the_text() {
    let fixture = Fixture::new("code-edit-type");
    let (mut editor, _, mut session) = open(&fixture, "ab\ncd", EditorSettings::default());
    feed(&mut editor, &mut session, &[plain(right)]);
    feed(&mut editor, &mut session, &typing("日✅x"));
    assert_eq!(text(&session), "a日✅xb\ncd");
    assert_eq!((selected(&session), caret(&session)), (8..8, 8));
}

#[test]
fn editing_keys_replace_a_selection_made_either_way() {
    let fixture = Fixture::new("code-edit-replace");
    let (mut editor, _, mut session) = open(&fixture, "hello world", EditorSettings::default());
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentEnd { select: false }],
    );
    feed(&mut editor, &mut session, &[select(left); 5]);
    feed(&mut editor, &mut session, &[InputEvent::Text('X')]);
    assert_eq!((text(&session), caret(&session)), ("hello X", 7));

    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentStart { select: false }],
    );
    feed(&mut editor, &mut session, &[select(right); 5]);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Tab { reverse: false }],
    );
    assert_eq!((text(&session), caret(&session)), ("     X", 4));

    feed(&mut editor, &mut session, &[select(left); 2]);
    feed(&mut editor, &mut session, &[InputEvent::Backspace]);
    assert_eq!((text(&session), caret(&session)), ("   X", 2));
    assert_eq!(selected(&session), 2..2);
}

#[test]
fn enter_breaks_lines_in_the_files_own_line_ending() {
    let fixture = Fixture::new("code-edit-crlf");
    let (mut editor, _, mut session) = open(&fixture, "a\r\nb", EditorSettings::default());
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::SelectAll, InputEvent::Enter],
    );
    assert_eq!(text(&session), "\r\n");
    assert_eq!(caret(&session), 2);
    assert_eq!(session.active_document().unwrap().line_count(), 2);
}

#[test]
fn backspace_and_delete_join_lines() {
    let fixture = Fixture::new("code-edit-join");
    let (mut editor, _, mut session) = open(&fixture, "ab\r\ncd\r\nef", EditorSettings::default());
    feed(
        &mut editor,
        &mut session,
        &[plain(down), InputEvent::Backspace],
    );
    assert_eq!((text(&session), caret(&session)), ("abcd\r\nef", 2));
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::End { select: false }, InputEvent::Delete],
    );
    assert_eq!((text(&session), caret(&session)), ("abcdef", 4));
}

#[test]
fn enter_at_the_bottom_scrolls_down_to_the_caret() {
    let fixture = Fixture::new("code-edit-scroll-down");
    let (_, mut session) = session_with(&fixture, &["a.luau"], 1);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    let rows = geometry(&editor, &session).rows;
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentEnd { select: false }],
    );
    feed(
        &mut editor,
        &mut session,
        &vec![InputEvent::Enter; rows + 2],
    );
    let document = session.active_document().unwrap();
    assert_eq!(document.line_count(), rows + 4);
    assert_eq!(view(&session).top_line, 4);
    assert!(caret_shown(&editor, &session));
}

#[test]
fn typing_past_the_right_edge_scrolls_sideways() {
    let fixture = Fixture::new("code-edit-scroll-side");
    let (mut editor, _, mut session) = open(&fixture, "", EditorSettings::default());
    let columns = geometry(&editor, &session).columns;
    feed(&mut editor, &mut session, &typing(&"a".repeat(columns + 5)));
    // The caret after the last character takes a column of its own.
    assert_eq!(view(&session).left_column, 6);
    assert!(caret_shown(&editor, &session));

    // Once every line fits, nothing is left to scroll across.
    feed(
        &mut editor,
        &mut session,
        &vec![InputEvent::Backspace; columns],
    );
    assert_eq!(view(&session).left_column, 0);
    assert!(caret_shown(&editor, &session));
}

#[test]
fn typing_folds_a_wrapped_line_onto_its_next_row() {
    let fixture = Fixture::new("code-edit-wrap");
    let settings = EditorSettings {
        word_wrap: true,
        ..EditorSettings::default()
    };
    let (mut editor, _, mut session) = open(&fixture, "", settings);
    let columns = geometry(&editor, &session).columns;
    feed(&mut editor, &mut session, &typing(&"a".repeat(columns)));
    assert_eq!(metrics(&session).rows(), 1);
    feed(&mut editor, &mut session, &typing("bc"));
    assert_eq!(metrics(&session).rows(), 2);
    let document = session.active_document().unwrap();
    let here = spot(
        document,
        caret(&session),
        metrics(&session).key(),
        &LAYOUT_FONT,
    );
    assert_eq!(
        (here.line, here.row_in_line, here.column),
        (0, 1, columns + 2)
    );
    assert!(caret_shown(&editor, &session));
}

#[test]
fn deleting_everything_brings_the_view_back_to_the_top() {
    let fixture = Fixture::new("code-edit-clear");
    let (_, mut session) = session_with(&fixture, &["a.luau"], 100);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentEnd { select: false }],
    );
    assert!(view(&session).top_line > 0);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::SelectAll, InputEvent::Delete],
    );
    assert_eq!(text(&session), "");
    assert_eq!(view(&session), DocumentView::default());
    assert_eq!(metrics(&session).rows(), 1);
}

#[test]
fn presses_and_vertical_moves_follow_the_edited_text() {
    let fixture = Fixture::new("code-edit-hit");
    let (mut editor, _, mut session) = open(&fixture, "abc", EditorSettings::default());
    // "✅" is two cells wide, so "a" now starts at column 4.
    feed(&mut editor, &mut session, &typing("✅✅"));
    let g = geometry(&editor, &session);
    click(&mut editor, &mut session, cell(&g, 0, 4));
    assert_eq!(caret(&session), 6);

    feed(
        &mut editor,
        &mut session,
        &[InputEvent::End { select: false }, InputEvent::Enter],
    );
    feed(&mut editor, &mut session, &typing("xy"));
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Up { select: false }],
    );
    // Column 2 on the line above is the edge after the first "✅".
    assert_eq!(caret(&session), 3);
    click(&mut editor, &mut session, cell(&g, 1, 1));
    assert_eq!(caret(&session), 11);
}

#[test]
fn a_gutter_that_widens_keeps_the_caret_shown() {
    let fixture = Fixture::new("code-edit-gutter");
    // 98 numbered lines and the empty one after the last newline.
    let (_, mut session) = session_with(&fixture, &["a.luau"], 98);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    let narrow = geometry(&editor, &session);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentEnd { select: false }, InputEvent::Enter],
    );
    let wide = geometry(&editor, &session);
    assert_eq!(session.active_document().unwrap().line_count(), 100);
    assert!(wide.viewport.x > narrow.viewport.x);
    assert!(caret_shown(&editor, &session));
}

#[test]
fn each_document_is_edited_on_its_own() {
    let fixture = Fixture::new("code-edit-tabs");
    let (_, mut session) = session_with_text(&fixture, &["a.luau", "b.luau"], "x\n");
    let mut editor = editor();
    let (a, b) = (id_of(&session, "a.luau"), id_of(&session, "b.luau"));
    feed(&mut editor, &mut session, &typing("bb"));
    feed(&mut editor, &mut session, &[InputEvent::PreviousDocument]);
    feed(
        &mut editor,
        &mut session,
        &[plain(down), InputEvent::Text('a')],
    );
    feed(&mut editor, &mut session, &[InputEvent::NextDocument]);
    feed(&mut editor, &mut session, &[InputEvent::Text('!')]);

    let text_of = |id| session.document(id).unwrap().text();
    assert_eq!((text_of(a), text_of(b)), ("x\na", "bb!x\n"));
    let caret_of = |id| session.tabs().selection(id).unwrap().caret().byte();
    assert_eq!((caret_of(a), caret_of(b)), (3, 3));
    assert!(session.document(a).unwrap().is_modified());
}

#[test]
fn overlays_keep_editing_keys_from_the_text() {
    let fixture = Fixture::new("code-edit-overlays");
    let files: Vec<String> = (0..12).map(|n| format!("module_{n:02}.luau")).collect();
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    let (cartridge, mut session) = session_with_text(&fixture, &refs, "x");
    let mut editor = editor();
    let keys = [
        InputEvent::Text('z'),
        InputEvent::Backspace,
        InputEvent::Delete,
        InputEvent::Tab { reverse: false },
    ];
    let untouched = |session: &CodeSession| {
        session
            .tabs()
            .ids()
            .all(|id| session.document(id).unwrap().revision() == 0)
    };

    editor.open_picker(cartridge.script_files());
    feed(&mut editor, &mut session, &keys);
    assert!(editor.picker().is_some());
    assert!(untouched(&session));
    feed(&mut editor, &mut session, &[InputEvent::Break]);

    let gear = centre(editor.layout().settings_button());
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Primary, gear)],
    );
    assert!(editor.has_overlay());
    feed(&mut editor, &mut session, &keys);
    assert!(untouched(&session));
    feed(&mut editor, &mut session, &[InputEvent::Break]);

    let more = centre(editor.layout().more_button());
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Primary, more)],
    );
    assert!(editor.has_overlay());
    feed(&mut editor, &mut session, &keys);
    feed(&mut editor, &mut session, &[InputEvent::Enter]);
    assert!(!editor.has_overlay());
    assert!(untouched(&session));
}

#[test]
fn shortcuts_insert_nothing() {
    let fixture = Fixture::new("code-edit-shortcuts");
    let (mut editor, _, mut session) = open(&fixture, "abc", EditorSettings::default());
    feed(
        &mut editor,
        &mut session,
        &[
            InputEvent::Save,
            InputEvent::Undo,
            InputEvent::Redo,
            InputEvent::SelectAll,
            InputEvent::NextView,
        ],
    );
    let document = session.active_document().unwrap();
    assert_eq!((document.text(), document.revision()), ("abc", 0));
}

#[test]
fn typing_during_a_drag_ends_it() {
    let fixture = Fixture::new("code-edit-drag");
    let (mut editor, _, mut session) = open(&fixture, "abcdef", EditorSettings::default());
    let g = geometry(&editor, &session);
    feed(
        &mut editor,
        &mut session,
        &[
            press(PointerButton::Primary, cell(&g, 0, 1)),
            InputEvent::PointerMove(Some(cell(&g, 0, 3))),
            InputEvent::Text('X'),
        ],
    );
    assert_eq!((text(&session), caret(&session)), ("aXdef", 2));
    assert!(!editor.is_dragging());
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PointerMove(Some(cell(&g, 0, 5)))],
    );
    assert_eq!(selected(&session), 2..2);
}

#[test]
fn ctrl_s_asks_to_save_the_active_document_only_when_nothing_covers_it() {
    let fixture = Fixture::new("code-edit-save");
    let (cartridge, mut session) = session_with_text(&fixture, &["a.luau", "b.luau"], "x");
    let mut editor = editor();
    let active = id_of(&session, "b.luau");
    assert_eq!(
        editor.handle_input(InputEvent::Save, Some(&mut session)),
        Some(CodeRequest::Save(active))
    );

    editor.open_picker(cartridge.script_files());
    assert_eq!(
        editor.handle_input(InputEvent::Save, Some(&mut session)),
        None
    );
    editor.handle_input(InputEvent::Break, Some(&mut session));
    assert!(editor.picker().is_none());

    for id in [id_of(&session, "a.luau"), active] {
        session.close(id);
    }
    assert_eq!(
        editor.handle_input(InputEvent::Save, Some(&mut session)),
        None
    );
}

#[test]
fn undo_and_redo_restore_the_text_and_selection_and_show_the_caret() {
    let fixture = Fixture::new("code-edit-undo");
    let text_before = "alpha beta\n".repeat(60);
    let (mut editor, _, mut session) = open(&fixture, &text_before, EditorSettings::default());
    feed(
        &mut editor,
        &mut session,
        &[plain(right), InputEvent::WordRight { select: true }],
    );
    assert_eq!(selected(&session), 1..5);
    feed(&mut editor, &mut session, &typing("XY"));
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentEnd { select: false }],
    );
    assert_ne!(view(&session), DocumentView::default());

    feed(&mut editor, &mut session, &[InputEvent::Undo]);
    assert_eq!(text(&session), text_before);
    assert_eq!((selected(&session), caret(&session)), (1..5, 5));
    assert!(caret_shown(&editor, &session));

    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentEnd { select: false }, InputEvent::Redo],
    );
    assert!(text(&session).starts_with("aXY beta\n"));
    assert_eq!((selected(&session), caret(&session)), (3..3, 3));
    assert!(caret_shown(&editor, &session));
}

#[test]
fn a_pause_in_typing_starts_a_new_undo_step() {
    let fixture = Fixture::new("code-edit-undo-idle");
    for (pause, after_undo) in [(UNDO_IDLE_TICKS - 1, ""), (UNDO_IDLE_TICKS, "ab")] {
        let (mut editor, _, mut session) = open(&fixture, "", EditorSettings::default());
        feed(&mut editor, &mut session, &typing("ab"));
        for _ in 0..pause {
            editor.tick(Some(&mut session));
        }
        feed(&mut editor, &mut session, &typing("cd"));
        feed(&mut editor, &mut session, &[InputEvent::Undo]);
        assert_eq!(text(&session), after_undo, "after a {pause}-tick pause");
    }
}

#[test]
fn each_document_undoes_its_own_edits_across_tab_switches() {
    let fixture = Fixture::new("code-edit-undo-tabs");
    let (_, mut session) = session_with_text(&fixture, &["a.luau", "b.luau"], "");
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    feed(&mut editor, &mut session, &typing("in b"));
    feed(&mut editor, &mut session, &[InputEvent::NextDocument]);
    feed(&mut editor, &mut session, &typing("in a"));

    feed(
        &mut editor,
        &mut session,
        &[InputEvent::NextDocument, InputEvent::Undo],
    );
    assert_eq!(text(&session), "");
    feed(&mut editor, &mut session, &[InputEvent::NextDocument]);
    assert_eq!(text(&session), "in a");
    feed(&mut editor, &mut session, &[InputEvent::Undo]);
    assert_eq!(text(&session), "");
    feed(&mut editor, &mut session, &[InputEvent::Undo]);
    assert_eq!(text(&session), "");
}
