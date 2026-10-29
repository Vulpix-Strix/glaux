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
