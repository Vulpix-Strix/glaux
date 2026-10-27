//! Breakpoints, and the Code view while a debugged cartridge is stopped.

use glaux_script::debug::{Frame, Resume, Step, Stop, StopReason, Variable};
use pretty_assertions::assert_eq;

use super::selection::{caret, cell, geometry, open};
use super::*;

fn variable(name: &str, value: &str, expandable: bool) -> Variable {
    Variable {
        name: name.into(),
        value: value.into(),
        expandable,
    }
}

fn frame(function: &str, line: u32) -> Frame {
    Frame {
        function: function.into(),
        path: Some("code/a.luau".into()),
        line: Some(line),
        has_variables: true,
    }
}

fn stop() -> Stop {
    Stop {
        reason: StopReason::Breakpoint,
        frames: vec![frame("tick", 2), frame("main", 5)],
        variables: vec![variable("x", "1", false), variable("player", "{..}", true)],
    }
}

fn marked_lines(session: &CodeSession) -> Vec<usize> {
    session
        .active_document()
        .unwrap()
        .breakpoints()
        .lines()
        .collect()
}

fn press_at(editor: &mut CodeEditor, session: &mut CodeSession, at: Point) -> Option<CodeRequest> {
    editor.handle_input(press(PointerButton::Primary, at), Some(session))
}

#[test]
fn f9_sets_and_clears_the_breakpoint_of_the_caret_line() {
    let fixture = Fixture::new("code-breakpoint-key");
    let (mut editor, _, mut session) = open(&fixture, "a\nb\nc", EditorSettings::default());
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Down { select: false }],
    );
    feed(&mut editor, &mut session, &[InputEvent::ToggleBreakpoint]);
    assert_eq!(marked_lines(&session), [1]);
    feed(&mut editor, &mut session, &[InputEvent::ToggleBreakpoint]);
    assert_eq!(marked_lines(&session), Vec::<usize>::new());
}

#[test]
fn pressing_a_line_number_toggles_that_line_without_moving_the_caret() {
    let fixture = Fixture::new("code-breakpoint-gutter");
    let (mut editor, _, mut session) = open(&fixture, "a\nb\nc", EditorSettings::default());
    let geometry = geometry(&editor, &session);
    let gutter = geometry.gutter.expect("line numbers are on by default");
    let at = Point::new(
        (gutter.x + 2) as u16,
        (geometry.viewport.y + 2 * geometry.cell_h + 1) as u16,
    );
    press_at(&mut editor, &mut session, at);
    editor.handle_input(release(at), Some(&mut session));
    assert_eq!(marked_lines(&session), [2]);
    assert_eq!(caret(&session), 0, "the press was not on the text");
}

#[test]
fn a_stop_replaces_the_run_plates_with_the_ones_that_go_on() {
    let fixture = Fixture::new("code-stop-plates");
    let (mut editor, _, mut session) = open(&fixture, "a\nb\nc", EditorSettings::default());
    editor.show_stop(stop());
    let plates = editor.layout().step_buttons();
    let asked: Vec<_> = plates
        .iter()
        .map(|&plate| press_at(&mut editor, &mut session, centre(plate)))
        .collect();
    assert_eq!(
        asked,
        [
            Some(CodeRequest::Debug(DebugAction::Resume(Resume::Continue))),
            Some(CodeRequest::Debug(DebugAction::Resume(Resume::Step(
                Step::Over
            )))),
            Some(CodeRequest::Debug(DebugAction::Resume(Resume::Step(
                Step::Into
            )))),
            Some(CodeRequest::Debug(DebugAction::Resume(Resume::Step(
                Step::Out
            )))),
            Some(CodeRequest::Debug(DebugAction::Stop)),
        ]
    );
}

#[test]
fn without_a_stop_the_run_and_debug_plates_start_the_cartridge() {
    let fixture = Fixture::new("code-run-plates");
    let (mut editor, _, mut session) = open(&fixture, "a", EditorSettings::default());
    let run = centre(editor.layout().run_button());
    let debug = centre(editor.layout().debug_button());
    assert_eq!(
        press_at(&mut editor, &mut session, run),
        Some(CodeRequest::Run)
    );
    assert_eq!(
        press_at(&mut editor, &mut session, debug),
        Some(CodeRequest::StartDebug)
    );
}

#[test]
fn the_panel_asks_to_select_a_frame_and_to_open_a_variable() {
    let fixture = Fixture::new("code-stop-panel");
    let (mut editor, _, mut session) = open(&fixture, "a\nb\nc", EditorSettings::default());
    editor.show_stop(stop());
    let layout = editor.debug_layout().unwrap();
    assert_eq!(
        press_at(&mut editor, &mut session, centre(layout.frame_row(1))),
        Some(CodeRequest::Debug(DebugAction::SelectFrame(1)))
    );
    assert_eq!(
        press_at(&mut editor, &mut session, centre(layout.local_row(1))),
        Some(CodeRequest::Debug(DebugAction::Toggle(vec![1])))
    );
    // A press on the panel's title takes no part in the text.
    let title = centre(layout.locals_title);
    assert_eq!(press_at(&mut editor, &mut session, title), None);
    assert_eq!(caret(&session), 0);
}

#[test]
fn the_view_opens_a_variable_once_its_entries_are_known() {
    let mut view = DebugView::new(stop());
    assert!(!view.toggle(&[0]), "a number has nothing to open");
    assert!(view.toggle(&[1]), "the entries must be fetched first");
    view.set_children(
        &[1],
        vec![variable("hp", "3", false), variable("pos", "{..}", true)],
    );
    let rows: Vec<_> = view
        .rows()
        .into_iter()
        .map(|row| (row.depth, row.name, row.open))
        .collect();
    assert_eq!(
        rows,
        [
            (0, "x".to_owned(), false),
            (0, "player".to_owned(), true),
            (1, "hp".to_owned(), false),
            (1, "pos".to_owned(), false),
        ]
    );
    assert!(!view.toggle(&[1]), "known entries need no fetching");
    assert_eq!(view.rows().len(), 2, "closed again");
    assert!(!view.toggle(&[1]));
    assert_eq!(view.rows().len(), 4, "and opened again");
}

#[test]
fn selecting_a_frame_waits_for_its_variables_and_moves_the_place() {
    let mut view = DebugView::new(stop());
    assert_eq!(view.place(), Some(("code/a.luau", 2)));
    view.select(1);
    assert_eq!(view.place(), Some(("code/a.luau", 5)));
    assert!(view.rows().is_empty());
    view.set_unavailable();
    assert!(view.unavailable());
    view.set_variables(vec![variable("y", "2", false)]);
    assert!(!view.unavailable());
    assert_eq!(view.rows().len(), 1);
    view.select(9);
    assert_eq!(view.selected(), 1, "there is no such frame");
}

#[test]
fn a_frame_that_is_not_script_code_has_no_place() {
    let mut stop = stop();
    stop.frames[0].path = None;
    stop.frames[0].line = None;
    assert_eq!(DebugView::new(stop).place(), None);
}

#[test]
fn scripts_cannot_be_edited_while_the_session_is_read_only() {
    let fixture = Fixture::new("code-read-only");
    let (mut editor, _, mut session) = open(&fixture, "abc", EditorSettings::default());
    session.set_read_only(true);
    for event in [
        InputEvent::Text('x'),
        InputEvent::Enter,
        InputEvent::Backspace,
        InputEvent::Delete,
        InputEvent::Tab { reverse: false },
        InputEvent::Undo,
    ] {
        let asked = editor.handle_input(event, Some(&mut session));
        assert!(
            matches!(asked, Some(CodeRequest::Notice(_))),
            "{event:?} gave {asked:?}"
        );
    }
    assert_eq!(session.active_document().unwrap().text(), "abc");

    // Reading still works.
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Right { select: false }],
    );
    assert_eq!(caret(&session), 1);

    session.set_read_only(false);
    feed(&mut editor, &mut session, &[InputEvent::Text('x')]);
    assert_eq!(session.active_document().unwrap().text(), "axbc");
}

#[test]
fn a_stop_closes_what_would_get_in_its_way_and_ending_it_restores_the_view() {
    let fixture = Fixture::new("code-stop-end");
    let (mut editor, _, mut session) = open(&fixture, "a\nb", EditorSettings::default());
    feed(&mut editor, &mut session, &[InputEvent::GoToSymbol]);
    assert!(editor.panel_layout().is_some());
    let width = editor.text_area().width;
    editor.show_stop(stop());
    assert!(
        editor.panel_layout().is_none(),
        "the call stack takes its place"
    );
    assert_eq!(editor.text_area().width, width);
    assert_eq!(
        editor.hint(),
        crate::ui_text::get("code-f5-go-f10-step-shift-f5-stop")
    );
    editor.end_stop();
    assert!(editor.debug_view().is_none());
    assert_eq!(editor.text_area().width, width + symbols::WIDTH);
}

#[test]
fn the_wheel_scrolls_the_variables_only_over_the_panel() {
    let fixture = Fixture::new("code-stop-wheel");
    let (mut editor, _, mut session) = open(&fixture, "a\nb", EditorSettings::default());
    let many = (0..40)
        .map(|n| variable(&format!("v{n}"), "1", false))
        .collect();
    editor.show_stop(Stop {
        variables: many,
        ..stop()
    });
    let layout = editor.debug_layout().unwrap();
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PointerMove(Some(centre(layout.local_row(0))))],
    );
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Wheel { delta: -2 }],
    );
    assert_eq!(editor.debug_view().unwrap().scroll(), 2);
    feed(&mut editor, &mut session, &[InputEvent::Wheel { delta: 9 }]);
    assert_eq!(editor.debug_view().unwrap().scroll(), 0);
}

#[test]
fn a_press_in_the_text_still_moves_the_caret_during_a_stop() {
    let fixture = Fixture::new("code-stop-caret");
    let (mut editor, _, mut session) = open(&fixture, "abcdef", EditorSettings::default());
    editor.show_stop(stop());
    let at = cell(&geometry(&editor, &session), 0, 3);
    press_at(&mut editor, &mut session, at);
    editor.handle_input(release(at), Some(&mut session));
    assert_eq!(caret(&session), 3);
}
