//! Indentation and the clipboard in the active document.

use super::selection::{caret, open, selected};
use super::*;
use crate::system::editor_settings::IndentStyle;
use pretty_assertions::assert_eq;

fn text(session: &CodeSession) -> &str {
    session.active_document().unwrap().text()
}

const TAB: InputEvent = InputEvent::Tab { reverse: false };
const SHIFT_TAB: InputEvent = InputEvent::Tab { reverse: true };

#[test]
fn enter_keeps_the_lines_indentation() {
    let fixture = Fixture::new("code-indent-enter");
    let (mut editor, _, mut session) = open(&fixture, "\tif x then", EditorSettings::default());
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::End { select: false }],
    );
    feed(&mut editor, &mut session, &[InputEvent::Enter]);
    feed(&mut editor, &mut session, &typing("y()"));
    assert_eq!(text(&session), "\tif x then\n\ty()");
}

#[test]
fn tab_over_lines_indents_them_and_undo_and_redo_keep_them_selected() {
    let fixture = Fixture::new("code-indent-block");
    let (mut editor, _, mut session) = open(&fixture, "a\nb\nc", EditorSettings::default());
    // Select from "a" into "b".
    feed(
        &mut editor,
        &mut session,
        &[
            InputEvent::Down { select: true },
            InputEvent::Right { select: true },
        ],
    );
    feed(&mut editor, &mut session, &[TAB]);
    assert_eq!(text(&session), "    a\n    b\nc");
    let indented = (selected(&session), caret(&session));
    assert_eq!(indented, (0..11, 11));
    feed(&mut editor, &mut session, &[InputEvent::Undo]);
    assert_eq!((text(&session), selected(&session)), ("a\nb\nc", 0..3));
    feed(&mut editor, &mut session, &[InputEvent::Redo]);
    assert_eq!(text(&session), "    a\n    b\nc");
    assert_eq!((selected(&session), caret(&session)), indented);
    feed(&mut editor, &mut session, &[SHIFT_TAB]);
    assert_eq!(text(&session), "a\nb\nc");
}

#[test]
fn shift_tab_outdents_the_caret_line_by_one_step() {
    let fixture = Fixture::new("code-indent-outdent");
    let settings = EditorSettings {
        indent: IndentStyle::Spaces,
        indent_size: 4,
        ..EditorSettings::default()
    };
    let (mut editor, _, mut session) = open(&fixture, "        x", settings);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::End { select: false }],
    );
    feed(&mut editor, &mut session, &[SHIFT_TAB]);
    assert_eq!((text(&session), caret(&session)), ("    x", 5));
    feed(&mut editor, &mut session, &[SHIFT_TAB, SHIFT_TAB]);
    assert_eq!(text(&session), "x", "nothing more to remove");
}

#[test]
fn the_indent_follows_the_file_unless_the_settings_say_otherwise() {
    let fixture = Fixture::new("code-indent-detect");
    let (mut editor, _, mut session) =
        open(&fixture, "if x then\n  y()\nend", EditorSettings::default());
    feed(&mut editor, &mut session, &[TAB]);
    assert_eq!(
        text(&session),
        "  if x then\n  y()\nend",
        "the file's 2 columns"
    );
    let settings = EditorSettings {
        indent: IndentStyle::Tabs,
        ..EditorSettings::default()
    };
    editor.set_settings(settings);
    feed(&mut editor, &mut session, &[TAB]);
    assert_eq!(text(&session), "  \tif x then\n  y()\nend");
}

#[test]
fn copy_cut_and_paste_go_through_the_clipboard() {
    let fixture = Fixture::new("code-clipboard");
    let (mut editor, _, mut session) = open(&fixture, "one two", EditorSettings::default());
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::Copy]),
        [],
        "nothing selected"
    );
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::WordRight { select: true }],
    );
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::Copy]),
        [CodeRequest::Copy("one".into())]
    );
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::Cut]),
        [CodeRequest::Copy("one".into())]
    );
    assert_eq!(text(&session), " two");
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::Paste]),
        [CodeRequest::Paste]
    );
    editor.paste(Some("1\r\n2".into()), Some(&mut session));
    assert_eq!((text(&session), caret(&session)), ("1\n2 two", 3));
    // An empty clipboard pastes nothing.
    editor.paste(None, Some(&mut session));
    assert_eq!(text(&session), "1\n2 two");
    feed(&mut editor, &mut session, &[InputEvent::Undo]);
    assert_eq!(text(&session), " two", "the paste undoes as one step");
    feed(&mut editor, &mut session, &[InputEvent::Undo]);
    assert_eq!(text(&session), "one two", "and so does the cut");
}
