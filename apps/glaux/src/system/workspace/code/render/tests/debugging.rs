//! How breakpoints and a stopped cartridge show.

use glaux_script::debug::{Frame, Stop, StopReason, Variable};
use pretty_assertions::assert_eq;

use super::*;
use crate::system::workspace::BAR;

fn stop_at(line: u32) -> Stop {
    let frame = |function: &str, line| Frame {
        function: function.into(),
        path: Some("code/a.luau".into()),
        line: Some(line),
        has_variables: true,
    };
    Stop {
        reason: StopReason::Breakpoint,
        frames: vec![frame("tick", line), frame("main", 3)],
        variables: vec![Variable {
            name: "x".into(),
            value: "1".into(),
            expandable: false,
        }],
    }
}

fn shown(fixture: &Fixture) -> (CodeEditor, CodeSession) {
    let mut editor = editor();
    hide_caret(&mut editor);
    let session = showing(fixture, &editor, "one\ntwo\nthree\n");
    (editor, session)
}

/// A pixel of the gutter inside row `row`'s line number, left of its digits.
fn number_ground(
    editor: &CodeEditor,
    session: &CodeSession,
    row: i32,
    framebuffer: &Framebuffer,
) -> u8 {
    let geometry = geometry_of(editor, session);
    let gutter = geometry.gutter.unwrap();
    framebuffer
        .pixel(gutter.x + 2, geometry.viewport.y + row * CELL_H + 1)
        .unwrap()
}

#[test]
fn a_breakpoint_turns_the_ground_of_its_line_number_red() {
    let fixture = Fixture::new("code-render-breakpoint");
    let (editor, mut session) = shown(&fixture);
    let id = session.tabs().active().unwrap();
    session.toggle_breakpoint(id, 1);
    let framebuffer = render(&editor, &session);
    assert_eq!(number_ground(&editor, &session, 1, &framebuffer), ERROR);
    assert_ne!(number_ground(&editor, &session, 0, &framebuffer), ERROR);
    assert_ne!(number_ground(&editor, &session, 2, &framebuffer), ERROR);
}

#[test]
fn the_line_a_cartridge_stopped_on_gets_a_band_and_an_arrow() {
    let fixture = Fixture::new("code-render-stopped-line");
    let (mut editor, session) = shown(&fixture);
    editor.show_stop(stop_at(2));
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    let band = |row: i32| {
        framebuffer
            .pixel(
                geometry.viewport.x + 30,
                geometry.viewport.y + row * CELL_H + 1,
            )
            .unwrap()
    };
    assert_eq!(band(1), BAR);
    assert_eq!(band(0), BACKGROUND);
    assert_eq!(band(2), BACKGROUND);

    let gutter = geometry.gutter.unwrap();
    let arrow_middle = geometry.viewport.y + CELL_H + CELL_H / 2;
    assert_eq!(framebuffer.pixel(gutter.x, arrow_middle), Some(WARN));
}

#[test]
fn a_frame_of_another_script_marks_nothing_here() {
    let fixture = Fixture::new("code-render-stopped-elsewhere");
    let (mut editor, session) = shown(&fixture);
    let mut stop = stop_at(2);
    stop.frames[0].path = Some("code/other.luau".into());
    editor.show_stop(stop);
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    let at = framebuffer.pixel(geometry.viewport.x + 30, geometry.viewport.y + CELL_H + 1);
    assert_eq!(at, Some(BACKGROUND));
}

#[test]
fn the_panel_sits_beside_the_text_with_the_selected_frame_on_a_band() {
    let fixture = Fixture::new("code-render-stop-panel");
    let (mut editor, session) = shown(&fixture);
    editor.show_stop(stop_at(2));
    let layout = editor.debug_layout().unwrap();
    assert_eq!(editor.text_area().right(), layout.area.x);

    let framebuffer = render(&editor, &session);
    assert_eq!(
        framebuffer.pixel(layout.area.x, layout.first_local.y + 2),
        Some(BAR),
        "the rule on the panel's left edge"
    );
    let first = layout.frame_row(0);
    let second = layout.frame_row(1);
    assert_eq!(framebuffer.pixel(first.x + 2, first.y), Some(BAR));
    assert_eq!(framebuffer.pixel(second.x + 2, second.y), Some(BACKGROUND));
}

#[test]
fn the_plates_that_go_on_are_drawn_where_the_run_plates_were() {
    let fixture = Fixture::new("code-render-stop-plates");
    let (mut editor, session) = shown(&fixture);
    let plate = editor.layout().step_buttons()[0];
    let ink = |editor: &CodeEditor| {
        let framebuffer = render(editor, &session);
        (plate.y..plate.bottom())
            .flat_map(|y| (plate.x..plate.right()).map(move |x| (x, y)))
            .filter(|&(x, y)| framebuffer.pixel(x, y) == Some(OK))
            .count()
    };
    let before = ink(&editor);
    editor.show_stop(stop_at(1));
    // Nothing is drawn that far left until a stop makes room for the plate.
    assert_eq!(before, 0);
    assert!(ink(&editor) > 0);
}
