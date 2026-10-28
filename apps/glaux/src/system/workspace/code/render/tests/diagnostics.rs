use super::*;
use crate::system::editing::diagnostics::{EditorDiagnostic, Severity};
use pretty_assertions::assert_eq;

fn finding(range: Range<usize>, severity: Severity, message: &str) -> EditorDiagnostic {
    EditorDiagnostic {
        range,
        severity,
        code: None,
        message: message.to_owned(),
    }
}

/// `items` stored for the active document as it is now.
fn analyzed(session: &mut CodeSession, items: Vec<EditorDiagnostic>) {
    let document = session.active_document().unwrap();
    let (path, revision) = (document.path().clone(), document.revision());
    assert!(session.set_diagnostics(&path, revision, items));
}

fn typed(session: &mut CodeSession, text: &str) {
    let id = session.active_document().unwrap().id();
    session
        .edit(id, 0..0, text, EditKind::Typing, None)
        .unwrap()
        .unwrap();
}

/// The x offsets within the cell at `at` where the bottom pixel row is
/// `color`.
fn bottom_row(framebuffer: &Framebuffer, at: (i32, i32), color: u8) -> Vec<i32> {
    (0..CELL_W)
        .filter(|dx| framebuffer.pixel(at.0 + dx, at.1 + CELL_H - 1) == Some(color))
        .collect()
}

#[test]
fn a_finding_is_underlined_with_a_dotted_line_over_its_cells_only() {
    let fixture = Fixture::new("code-diag-underline");
    let editor = editor();
    let mut session = showing(&fixture, &editor, "local x = 1\n");
    analyzed(&mut session, vec![finding(6..7, Severity::Error, "bad")]);
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    assert_eq!(
        bottom_row(&framebuffer, cell(&geometry, 6, 0), ERROR),
        [0, 2, 4]
    );
    assert_eq!(
        bottom_row(&framebuffer, cell(&geometry, 5, 0), ERROR),
        Vec::<i32>::new()
    );
    assert_eq!(
        bottom_row(&framebuffer, cell(&geometry, 7, 0), ERROR),
        Vec::<i32>::new()
    );
}

#[test]
fn warnings_are_yellow_and_information_is_not_drawn_in_the_text() {
    let fixture = Fixture::new("code-diag-severity");
    let editor = editor();
    let mut session = showing(&fixture, &editor, "local x = 1\n");
    analyzed(
        &mut session,
        vec![
            finding(1..2, Severity::Warning, "w"),
            finding(6..7, Severity::Info, "i"),
        ],
    );
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    assert_eq!(
        bottom_row(&framebuffer, cell(&geometry, 1, 0), WARN),
        [0, 2, 4]
    );
    for color in [ERROR, WARN, ACCENT] {
        assert_eq!(
            bottom_row(&framebuffer, cell(&geometry, 6, 0), color),
            Vec::<i32>::new()
        );
    }
}

#[test]
fn an_edit_removes_every_mark_until_the_next_answer() {
    let fixture = Fixture::new("code-diag-stale");
    let editor = editor();
    let mut session = showing(&fixture, &editor, "local x = 1\n");
    analyzed(&mut session, vec![finding(6..7, Severity::Error, "bad")]);
    typed(&mut session, "--");
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    // The text moved two bytes; nothing may be drawn at either place.
    for column in [6, 8] {
        assert_eq!(
            bottom_row(&framebuffer, cell(&geometry, column, 0), ERROR),
            Vec::<i32>::new(),
            "column {column}"
        );
    }
    assert!(
        session
            .diagnostics(session.active_document().unwrap().id())
            .is_empty()
    );
}

#[test]
fn the_margin_marks_the_line_but_a_breakpoint_keeps_the_margin() {
    let fixture = Fixture::new("code-diag-gutter");
    let editor = editor();
    let mut session = showing(&fixture, &editor, "a\nb\nc\n");
    analyzed(
        &mut session,
        vec![
            finding(0..1, Severity::Warning, "first"),
            finding(2..3, Severity::Warning, "second"),
        ],
    );
    let id = session.active_document().unwrap().id();
    session.toggle_breakpoint(id, 1);
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    let gutter = geometry.gutter.unwrap();
    let marker =
        |row: i32| framebuffer.pixel(gutter.right() - 4, geometry.viewport.y + row * CELL_H + 3);
    assert_eq!(marker(0), Some(WARN));
    assert_eq!(marker(1), Some(ERROR), "the breakpoint holds the margin");
    assert_ne!(marker(2), Some(WARN), "line 3 has no finding");
}

#[test]
fn the_status_bar_counts_findings_and_dims_the_counts_after_an_edit() {
    let fixture = Fixture::new("code-diag-summary");
    let editor = editor();
    let mut session = showing(&fixture, &editor, "a\nb\nc\n");
    analyzed(
        &mut session,
        vec![
            finding(0..1, Severity::Error, "e"),
            finding(2..3, Severity::Warning, "w"),
        ],
    );
    let bar = editor.layout().status;
    let count = |framebuffer: &Framebuffer, color: u8| {
        (bar.x + bar.width / 2..bar.right())
            .flat_map(|x| (bar.y..bar.bottom()).map(move |y| (x, y)))
            .filter(|&(x, y)| framebuffer.pixel(x, y) == Some(color))
            .count()
    };
    let fresh = render(&editor, &session);
    assert!(count(&fresh, ERROR) > 0 && count(&fresh, WARN) > 0);

    typed(&mut session, "x");
    let stale = render(&editor, &session);
    assert_eq!(count(&stale, ERROR) + count(&stale, WARN), 0);
    assert!(count(&stale, DIM) > 0, "the counts stay, dimmed");
}

#[test]
fn the_scroll_rail_shows_where_the_findings_are() {
    let fixture = Fixture::new("code-diag-rail");
    let editor = editor();
    let text: String = (0..120).map(|n| format!("line {n}\n")).collect();
    let mut session = showing(&fixture, &editor, &text);
    let at = text.match_indices('\n').nth(89).unwrap().0;
    analyzed(
        &mut session,
        vec![finding(at..at + 1, Severity::Error, "far below")],
    );
    let framebuffer = render(&editor, &session);
    let track = geometry_of(&editor, &session).vertical;
    let ticks: Vec<i32> = (track.y..track.bottom())
        .filter(|&y| framebuffer.pixel(track.x, y) == Some(ERROR))
        .collect();
    assert_eq!(ticks.len(), 1, "{ticks:?}");
    // Line 90 is the 90th of 121 rows, the last being the empty one after
    // the final newline.
    let expected = track.y + track.height * 89 / 121;
    assert!(
        (ticks[0] - expected).abs() <= 1,
        "{} vs {expected}",
        ticks[0]
    );
}
