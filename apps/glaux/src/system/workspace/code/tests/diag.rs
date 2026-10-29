use super::*;
use crate::system::editing::diagnostics::{EditorDiagnostic, Severity};
use pretty_assertions::assert_eq;

use crate::system::workspace::code::diag::DiagList;

fn finding(start: usize, severity: Severity, message: &str) -> EditorDiagnostic {
    EditorDiagnostic {
        range: start..start + 1,
        severity,
        code: None,
        message: message.to_owned(),
    }
}

fn document(fixture: &Fixture) -> DocumentId {
    let (_, session) = session_with_text(fixture, &["a.luau"], "a b c d e f\n");
    session.active_document().unwrap().id()
}

#[test]
fn the_pick_stays_on_its_finding_when_others_come_and_go() {
    let fixture = Fixture::new("code-diag-stable");
    let id = document(&fixture);
    let mut list = DiagList::default();
    let found = vec![
        finding(2, Severity::Error, "one"),
        finding(6, Severity::Warning, "two"),
    ];
    assert!(list.focus(id, &found, 5));
    assert_eq!(list.index(id, &found), Some(1));
    // A finding appears before it and the text shifts everything by one.
    let found = vec![
        finding(0, Severity::Error, "new"),
        finding(3, Severity::Error, "one"),
        finding(7, Severity::Warning, "two"),
    ];
    list.sync(id, &found, 1);
    assert_eq!(list.index(id, &found), Some(2));
}

#[test]
fn a_vanished_finding_hands_the_pick_to_its_nearest_neighbour() {
    let fixture = Fixture::new("code-diag-vanish");
    let id = document(&fixture);
    let mut list = DiagList::default();
    let found = vec![
        finding(2, Severity::Error, "one"),
        finding(6, Severity::Error, "two"),
        finding(10, Severity::Error, "three"),
    ];
    assert!(list.focus(id, &found, 6));
    let found = vec![
        finding(2, Severity::Error, "one"),
        finding(10, Severity::Error, "three"),
    ];
    list.sync(id, &found, 2);
    assert_eq!(list.index(id, &found), Some(0));
}

#[test]
fn an_empty_list_gives_the_keys_back_and_cannot_be_focused() {
    let fixture = Fixture::new("code-diag-empty");
    let id = document(&fixture);
    let mut list = DiagList::default();
    let found = vec![finding(2, Severity::Error, "one")];
    assert!(list.focus(id, &found, 0));
    list.sync(id, &[], 1);
    assert!(!list.focused());
    assert!(!list.focus(id, &[], 0));
}

#[test]
fn the_view_scrolls_to_keep_the_pick_in_sight() {
    let fixture = Fixture::new("code-diag-scroll");
    let id = document(&fixture);
    let mut list = DiagList::default();
    let found: Vec<_> = (0..5)
        .map(|n| finding(n * 2, Severity::Warning, &n.to_string()))
        .collect();
    assert!(list.focus(id, &found, 0));
    list.step(id, &found, 4);
    list.sync(id, &found, 3);
    assert_eq!((list.index(id, &found), list.top()), (Some(4), 2));
}

/// A session over `a b c d e f` with `items` stored for it.
fn analyzed(fixture: &Fixture, items: Vec<EditorDiagnostic>) -> (CodeEditor, CodeSession) {
    let editor = editor();
    let (_, mut session) = session_with_text(fixture, &["a.luau"], "a b c d e f\n");
    editor.refresh_metrics(&mut session);
    let document = session.active_document().unwrap();
    let (path, revision) = (document.path().clone(), document.revision());
    session.set_diagnostics(&path, revision, items);
    (editor, session)
}

fn caret(session: &CodeSession) -> usize {
    let id = session.active_document().unwrap().id();
    session.tabs().selection(id).unwrap().caret().byte()
}

#[test]
fn focus_picks_a_row_and_enter_goes_to_it_and_gives_the_keys_back() {
    let fixture = Fixture::new("code-diag-enter");
    let items = vec![
        finding(2, Severity::Error, "one"),
        finding(6, Severity::Warning, "two"),
    ];
    let (mut editor, mut session) = analyzed(&fixture, items);
    editor.handle_input(InputEvent::FocusDiagnostics, Some(&mut session));
    assert!(editor.claims_break());
    editor.handle_input(InputEvent::Down { select: false }, Some(&mut session));
    editor.handle_input(InputEvent::Enter, Some(&mut session));
    assert_eq!(caret(&session), 6);
    assert!(!editor.claims_break());
}

#[test]
fn typing_does_not_edit_while_the_list_has_the_keys() {
    let fixture = Fixture::new("code-diag-typing");
    let (mut editor, mut session) = analyzed(&fixture, vec![finding(2, Severity::Error, "one")]);
    editor.handle_input(InputEvent::FocusDiagnostics, Some(&mut session));
    editor.handle_input(InputEvent::Text('x'), Some(&mut session));
    assert_eq!(session.active_document().unwrap().text(), "a b c d e f\n");
    editor.handle_input(InputEvent::Break, Some(&mut session));
    editor.handle_input(InputEvent::Text('x'), Some(&mut session));
    assert_eq!(session.active_document().unwrap().text(), "xa b c d e f\n");
}

#[test]
fn focus_does_nothing_without_findings() {
    let fixture = Fixture::new("code-diag-none");
    let (mut editor, mut session) = analyzed(&fixture, vec![]);
    editor.handle_input(InputEvent::FocusDiagnostics, Some(&mut session));
    assert!(!editor.claims_break());
}

#[test]
fn tab_opens_the_whole_message_which_scrolls_and_closes_with_escape() {
    let fixture = Fixture::new("code-diag-detail");
    let long = "word ".repeat(120);
    let (mut editor, mut session) = analyzed(&fixture, vec![finding(2, Severity::Error, &long)]);
    editor.handle_input(InputEvent::FocusDiagnostics, Some(&mut session));
    editor.handle_input(InputEvent::Tab { reverse: false }, Some(&mut session));
    assert!(editor.has_overlay());
    let before = caret(&session);
    editor.handle_input(InputEvent::Down { select: false }, Some(&mut session));
    assert_eq!(
        caret(&session),
        before,
        "keys scroll the window, not the text"
    );
    editor.handle_input(InputEvent::Break, Some(&mut session));
    assert!(!editor.has_overlay());
}
