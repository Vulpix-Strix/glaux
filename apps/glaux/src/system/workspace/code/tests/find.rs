//! The find bar, and which keys reach it rather than the document.

use super::selection::{caret, cell, click, geometry, open, selected};
use super::*;
use pretty_assertions::assert_eq;

fn text(session: &CodeSession) -> &str {
    session.active_document().unwrap().text()
}

fn find_open(fixture: &Fixture, text: &str, query: &str) -> (CodeEditor, CodeSession) {
    let (mut editor, _, mut session) = open(fixture, text, EditorSettings::default());
    feed(&mut editor, &mut session, &[InputEvent::Find]);
    feed(&mut editor, &mut session, &typing(query));
    (editor, session)
}

#[test]
fn typing_a_query_selects_the_first_match_from_the_caret() {
    let fixture = Fixture::new("code-find-type");
    let (mut editor, _, mut session) = open(&fixture, "aXb aXb aXb", EditorSettings::default());
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Right { select: false }],
    );
    feed(&mut editor, &mut session, &[InputEvent::Find]);
    feed(&mut editor, &mut session, &typing("axb"));
    assert_eq!(
        selected(&session),
        4..7,
        "the match at 0 lies before the caret"
    );
    assert_eq!(
        text(&session),
        "aXb aXb aXb",
        "the query never reaches the text"
    );
}

#[test]
fn next_and_previous_wrap_around() {
    let fixture = Fixture::new("code-find-wrap");
    let (mut editor, mut session) = find_open(&fixture, "x1 x2 x3", "x");
    assert_eq!(selected(&session), 0..1);
    let down = InputEvent::Down { select: false };
    let up = InputEvent::Up { select: false };
    feed(&mut editor, &mut session, &[InputEvent::Enter, down]);
    assert_eq!(selected(&session), 6..7);
    feed(&mut editor, &mut session, &[down]);
    assert_eq!(selected(&session), 0..1, "past the last match");
    feed(&mut editor, &mut session, &[up]);
    assert_eq!(selected(&session), 6..7, "before the first match");
}

#[test]
fn whole_word_and_match_case_narrow_the_matches() {
    let fixture = Fixture::new("code-find-options");
    let (mut editor, mut session) = find_open(&fixture, "max X x", "x");
    let starts = |editor: &CodeEditor, session: &CodeSession| {
        let document = session.active_document().unwrap();
        let matches = editor.find().unwrap().matches_in(document);
        matches.iter().map(|found| found.start).collect::<Vec<_>>()
    };
    assert_eq!(starts(&editor, &session), [2, 4, 6]);
    let tab = InputEvent::Tab { reverse: false };
    // Focus Aa, then W, toggling each.
    feed(&mut editor, &mut session, &[tab, InputEvent::Enter]);
    assert_eq!(starts(&editor, &session), [2, 6]);
    feed(&mut editor, &mut session, &[tab, InputEvent::Enter]);
    assert_eq!(starts(&editor, &session), [6]);
}

#[test]
fn matches_follow_edits_made_while_the_bar_is_open() {
    let fixture = Fixture::new("code-find-live");
    let (mut editor, mut session) = find_open(&fixture, "ab ab", "ab");
    let at = cell(&geometry(&editor, &session), 0, 5);
    // A click in the text gives it the keys; the bar stays open.
    click(&mut editor, &mut session, at);
    assert_eq!(editor.find().unwrap().focus(), None);
    feed(&mut editor, &mut session, &typing(" ab"));
    let document = session.active_document().unwrap();
    assert_eq!(text(&session), "ab ab ab");
    assert_eq!(
        editor.find().unwrap().matches_in(document),
        [0..2, 3..5, 6..8]
    );
}

#[test]
fn replace_swaps_the_selected_match_and_moves_on() {
    let fixture = Fixture::new("code-replace-one");
    let (mut editor, _, mut session) = open(&fixture, "a.x = a.y", EditorSettings::default());
    feed(&mut editor, &mut session, &[InputEvent::Replace]);
    feed(&mut editor, &mut session, &typing("a."));
    let tab = InputEvent::Tab { reverse: false };
    feed(&mut editor, &mut session, &[tab]);
    feed(&mut editor, &mut session, &typing("self."));
    feed(&mut editor, &mut session, &[InputEvent::Enter]);
    assert_eq!(text(&session), "self.x = a.y");
    assert_eq!(selected(&session), 9..11, "the next match is selected");
    feed(&mut editor, &mut session, &[InputEvent::Enter]);
    assert_eq!(text(&session), "self.x = self.y");
    // Undo in a field never reaches the document; once Escape gives the
    // text the keys back, each replacement undoes on its own.
    feed(&mut editor, &mut session, &[InputEvent::Undo]);
    assert_eq!(text(&session), "self.x = self.y");
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Break, InputEvent::Undo],
    );
    assert!(editor.find().is_none());
    assert_eq!(text(&session), "self.x = a.y");
}

#[test]
fn replace_all_is_one_undo_step_and_reports_the_count() {
    let fixture = Fixture::new("code-replace-all");
    let (mut editor, _, mut session) = open(&fixture, "v v\nv", EditorSettings::default());
    feed(&mut editor, &mut session, &[InputEvent::Replace]);
    feed(&mut editor, &mut session, &typing("v"));
    let tab = InputEvent::Tab { reverse: false };
    feed(&mut editor, &mut session, &[tab]);
    feed(&mut editor, &mut session, &typing("speed"));
    // Replace, then All.
    let all = requests(&mut editor, &mut session, &[tab, tab, InputEvent::Enter]);
    assert_eq!(all, [CodeRequest::Notice("Replaced 3".into())]);
    assert_eq!(text(&session), "speed speed\nspeed");
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Break, InputEvent::Undo],
    );
    assert_eq!(text(&session), "v v\nv");
}

#[test]
fn a_single_line_selection_becomes_the_query() {
    let fixture = Fixture::new("code-find-seed");
    let (mut editor, _, mut session) = open(&fixture, "speed = speed", EditorSettings::default());
    feed(&mut editor, &mut session, &[InputEvent::SelectAll]);
    feed(&mut editor, &mut session, &[InputEvent::Find]);
    assert_eq!(editor.find().unwrap().query(), "speed = speed");
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentStart { select: false }],
    );
    for _ in 0..5 {
        feed(
            &mut editor,
            &mut session,
            &[InputEvent::Right { select: true }],
        );
    }
    feed(&mut editor, &mut session, &[InputEvent::Find]);
    assert_eq!(editor.find().unwrap().query(), "speed");
}

// Which keys reach the bar's fields and which the document.

#[test]
fn select_all_and_clipboard_keys_in_a_field_leave_the_document_alone() {
    let fixture = Fixture::new("code-find-focus-keys");
    let (mut editor, mut session) = find_open(&fixture, "one two", "two");
    let before = (selected(&session), caret(&session));
    let asked = requests(
        &mut editor,
        &mut session,
        &[InputEvent::SelectAll, InputEvent::Copy, InputEvent::Undo],
    );
    assert_eq!(
        asked,
        [CodeRequest::Copy("two".into())],
        "the query is copied"
    );
    assert_eq!((selected(&session), caret(&session)), before);
    assert_eq!(text(&session), "one two");
    // A paste goes to the query too.
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::Paste]),
        [CodeRequest::Paste]
    );
    editor.paste(Some("one".into()), Some(&mut session));
    assert_eq!(editor.find().unwrap().query(), "one");
    assert_eq!(text(&session), "one two");
    assert_eq!(selected(&session), 0..3, "the pasted query was searched");
}

#[test]
fn document_commands_still_work_from_a_field() {
    let fixture = Fixture::new("code-find-focus-commands");
    let (mut editor, mut session) = find_open(&fixture, "a", "a");
    let id = session.tabs().active().unwrap();
    assert_eq!(
        requests(
            &mut editor,
            &mut session,
            &[InputEvent::Save, InputEvent::OpenFile, InputEvent::NewFile]
        ),
        [
            CodeRequest::Save(id),
            CodeRequest::ListScripts,
            CodeRequest::NewFile
        ]
    );
    feed(&mut editor, &mut session, &[InputEvent::GoToLine]);
    assert!(editor.go_to().is_some());
}

#[test]
fn escape_closes_the_bar_whichever_part_has_the_keys() {
    let fixture = Fixture::new("code-find-escape");
    let (mut editor, mut session) = find_open(&fixture, "abc", "b");
    assert!(editor.claims_break() && !editor.has_overlay());
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    assert!(editor.find().is_none() && !editor.claims_break());
    feed(&mut editor, &mut session, &[InputEvent::Find]);
    let at = cell(&geometry(&editor, &session), 0, 0);
    click(&mut editor, &mut session, at);
    assert!(editor.claims_break(), "open while the text has the keys");
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    assert!(editor.find().is_none());
}

#[test]
fn the_bar_pushes_the_text_down_rather_than_covering_it() {
    let fixture = Fixture::new("code-find-dock");
    let (mut editor, _, mut session) = open(&fixture, "a", EditorSettings::default());
    let closed = geometry(&editor, &session).viewport;
    feed(&mut editor, &mut session, &[InputEvent::Find]);
    let open = geometry(&editor, &session).viewport;
    let panel = editor.find_layout().unwrap().panel;
    assert_eq!(open.y, panel.bottom() + 1);
    assert!(open.y > closed.y);
    assert!(open.bottom() <= editor.layout().text.bottom());
}
