use super::*;
use crate::system::editing::diagnostics::{EditorDiagnostic, Severity};
use pretty_assertions::assert_eq;

fn finding(start: usize, severity: Severity, message: &str) -> EditorDiagnostic {
    EditorDiagnostic {
        range: start..start + 1,
        severity,
        code: None,
        message: message.to_owned(),
    }
}

/// A session over `a b c d e f` (letters at even bytes) with `items`
/// stored for it.
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

fn jump(editor: &mut CodeEditor, session: &mut CodeSession, reverse: bool) -> usize {
    editor.handle_input(InputEvent::NextProblem { reverse }, Some(session));
    caret(session)
}

#[test]
fn next_problem_walks_the_findings_and_wraps() {
    let fixture = Fixture::new("code-problems-next");
    let items = vec![
        finding(2, Severity::Error, "one"),
        finding(6, Severity::Warning, "two"),
        finding(10, Severity::Error, "three"),
    ];
    let (mut editor, mut session) = analyzed(&fixture, items);
    let walked: Vec<usize> = (0..4)
        .map(|_| jump(&mut editor, &mut session, false))
        .collect();
    assert_eq!(walked, [2, 6, 10, 2]);
}

#[test]
fn previous_problem_walks_backward_and_wraps() {
    let fixture = Fixture::new("code-problems-previous");
    let items = vec![
        finding(2, Severity::Error, "one"),
        finding(6, Severity::Error, "two"),
    ];
    let (mut editor, mut session) = analyzed(&fixture, items);
    let walked: Vec<usize> = (0..3)
        .map(|_| jump(&mut editor, &mut session, true))
        .collect();
    assert_eq!(walked, [6, 2, 6]);
}

#[test]
fn without_findings_the_caret_stays() {
    let fixture = Fixture::new("code-problems-none");
    let (mut editor, mut session) = analyzed(&fixture, vec![]);
    assert_eq!(jump(&mut editor, &mut session, false), 0);
    assert_eq!(jump(&mut editor, &mut session, true), 0);
}

#[test]
fn stale_findings_are_not_walked() {
    let fixture = Fixture::new("code-problems-stale");
    let (mut editor, mut session) = analyzed(&fixture, vec![finding(6, Severity::Error, "old")]);
    let id = session.active_document().unwrap().id();
    session
        .edit(id, 0..0, "x", EditKind::Typing, None)
        .unwrap()
        .unwrap();
    let before = caret(&session);
    assert_eq!(jump(&mut editor, &mut session, false), before);
}

#[test]
fn the_message_under_the_caret_is_the_most_serious_one() {
    let fixture = Fixture::new("code-problems-caret");
    let items = vec![
        finding(4, Severity::Warning, "minor"),
        finding(4, Severity::Error, "major"),
        finding(8, Severity::Error, "elsewhere"),
    ];
    let (mut editor, mut session) = analyzed(&fixture, items);
    assert_eq!(editor.problem_at_caret(&session), None);
    jump(&mut editor, &mut session, false);
    assert_eq!(
        editor
            .problem_at_caret(&session)
            .map(|d| d.message.as_str()),
        Some("major")
    );
}

#[test]
fn the_settings_window_still_fits_the_view_with_its_diagnostics_row() {
    let editor = editor();
    let layout = settings_layout(editor.layout());
    let area = overlay_area(editor.layout());
    assert!(
        layout.window.y >= area.y && layout.window.bottom() <= area.bottom(),
        "{:?} in {area:?}",
        layout.window
    );
}
