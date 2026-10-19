//! How the signature popup shows.

use pretty_assertions::assert_eq;

use super::*;
use crate::system::workspace::BAR;
use crate::system::workspace::code::signature::SignatureLayout;

/// An editor and session with `typed` entered after `text` in `a.luau`.
fn typing(fixture: &Fixture, text: &str, typed: &str) -> (CodeEditor, CodeSession) {
    let mut editor = editor();
    let mut session = showing(fixture, &editor, text);
    editor.handle_input(
        InputEvent::DocumentEnd { select: false },
        Some(&mut session),
    );
    for ch in typed.chars() {
        editor.handle_input(InputEvent::Text(ch), Some(&mut session));
    }
    (editor, session)
}

#[test]
fn the_popup_draws_a_bordered_box_with_a_hard_shadow() {
    let fixture = Fixture::new("code-render-signature");
    let (editor, session) = typing(&fixture, "", "spr(1, ");
    let layout = editor.signature_layout(&session).expect("showing");

    let framebuffer = render(&editor, &session);
    let window = layout.window;
    assert_eq!(framebuffer.pixel(window.x, window.y + 4), Some(DIM));
    assert_eq!(framebuffer.pixel(window.x + 4, window.y), Some(DIM));
    assert_eq!(
        framebuffer.pixel(window.right() + 1, window.y + 6),
        Some(BAR)
    );
    assert_eq!(
        framebuffer.pixel(window.x + 4, window.bottom() + 1),
        Some(BAR)
    );
}

#[test]
fn only_the_active_parameter_is_set_off_by_a_band() {
    let fixture = Fixture::new("code-render-signature-chip");
    let (mut editor, mut session) = typing(&fixture, "", "spr(1, ");
    let rom = rom();
    let band_x = |layout: &SignatureLayout, word: &str| {
        let mut x = layout.text_x;
        for segment in &layout.segments {
            if segment.text == word {
                return x;
            }
            x += rom.font().text_cells(&segment.text) as i32 * CELL_W;
        }
        panic!("{word} is not in the signature");
    };

    let layout = editor.signature_layout(&session).unwrap();
    let framebuffer = render(&editor, &session);
    // The top row of a cell is blank, so a band shows there.
    let (x, y) = (band_x(&layout, "x"), layout.line_y);
    assert_eq!(framebuffer.pixel(x, y), Some(BAR));
    assert_eq!(
        framebuffer.pixel(band_x(&layout, "tile"), y),
        Some(BACKGROUND)
    );
    assert_eq!(framebuffer.pixel(band_x(&layout, "y"), y), Some(BACKGROUND));

    // Another comma moves the band.
    editor.handle_input(InputEvent::Text('2'), Some(&mut session));
    editor.handle_input(InputEvent::Text(','), Some(&mut session));
    let layout = editor.signature_layout(&session).unwrap();
    let framebuffer = render(&editor, &session);
    assert_eq!(framebuffer.pixel(band_x(&layout, "y"), y), Some(BAR));
    assert_eq!(framebuffer.pixel(band_x(&layout, "x"), y), Some(BACKGROUND));
}

#[test]
fn the_popup_sits_above_the_line_being_typed() {
    let fixture = Fixture::new("code-render-signature-place");
    let (editor, session) = typing(&fixture, &"a = 1\n".repeat(8), "spr(");
    let layout = editor.signature_layout(&session).unwrap();
    let document = session.active_document().unwrap();
    let geometry = geometry_of(&editor, &session);
    let caret_row = document.line_of(document.text().len()) as i32;
    let line_top = geometry.viewport.y + caret_row * CELL_H;
    assert!(layout.window.bottom() + 2 <= line_top, "{layout:?}");
}

#[test]
fn near_the_top_of_the_view_the_popup_goes_below_the_line() {
    let fixture = Fixture::new("code-render-signature-below");
    let (editor, session) = typing(&fixture, "", "spr(");
    let layout = editor.signature_layout(&session).unwrap();
    let geometry = geometry_of(&editor, &session);
    assert_eq!(layout.window.y, geometry.viewport.y + CELL_H);
}

#[test]
fn an_overlay_hides_the_popup() {
    let fixture = Fixture::new("code-render-signature-overlay");
    let (mut editor, mut session) = typing(&fixture, "", "spr(1, ");
    let layout = editor.signature_layout(&session).unwrap();
    let probe = (layout.window.x, layout.window.y + 4);

    assert_eq!(render(&editor, &session).pixel(probe.0, probe.1), Some(DIM));
    editor.handle_input(InputEvent::GoToLine, Some(&mut session));
    assert!(editor.has_overlay());
    assert_ne!(render(&editor, &session).pixel(probe.0, probe.1), Some(DIM));
}

#[test]
fn the_popup_gives_way_to_the_list_it_would_cover() {
    let fixture = Fixture::new("code-render-signature-list");
    // Close to the top there is no room above the line, so the popup would
    // go below it, where the suggestions are.
    let mut editor = editor();
    let mut session = showing(&fixture, &editor, "");
    for ch in "spr(re".chars() {
        editor.handle_input(InputEvent::Text(ch), Some(&mut session));
    }
    assert!(editor.completion().is_some() && editor.signature().is_some());
    let list = editor.completion_layout(&session).unwrap().list;
    match editor.signature_layout(&session) {
        None => {}
        Some(popup) => {
            let a = popup.window;
            let apart = a.right() + 2 <= list.x
                || list.right() + 2 <= a.x
                || a.bottom() + 2 <= list.y
                || list.bottom() + 2 <= a.y;
            assert!(apart, "{a:?} covers {list:?}");
        }
    }
}

#[test]
fn a_caret_scrolled_out_of_sight_leaves_no_popup() {
    let fixture = Fixture::new("code-render-signature-scroll");
    let text = "line\n".repeat(60);
    let (editor, mut session) = typing(&fixture, &text, "spr(");
    assert!(editor.signature_layout(&session).is_some());
    let id = session.active_document().unwrap().id();
    session.tabs_mut().view_mut(id).unwrap().top_line = 0;
    assert!(editor.signature_layout(&session).is_none());
}
