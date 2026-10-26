//! How the suggestion list and its panel show.

use glaux_syntax::SyntaxKind;
use pretty_assertions::assert_eq;

use super::*;
use crate::system::screen::{DIM, PALE};
use crate::system::workspace::BAR;
use crate::system::workspace::code::completion::DocPanel;
use crate::system::workspace::code::tests::completion::list_with_panel;

fn panel() -> DocPanel {
    DocPanel {
        signature: vec![("audio".to_owned(), SyntaxKind::Type)],
        summary: Some("Plays sound.".to_owned()),
        params: None,
    }
}

#[test]
fn the_list_draws_a_bordered_box_with_the_pick_in_a_pale_band() {
    let fixture = Fixture::new("code-render-complete");
    let mut editor = editor();
    let mut session = showing(&fixture, &editor, "au");
    list_with_panel(&mut editor, &mut session, &["audio", "auto"], 0..2, None);
    let layout = editor.completion_layout(&session).unwrap();

    let framebuffer = render(&editor, &session);
    let (first, second) = (layout.rows[0].1, layout.rows[1].1);
    // The pick is a band across its row, behind its text.
    let corner = |rect: Rect| framebuffer.pixel(rect.right() - 1, rect.y);
    assert_eq!(corner(first), Some(PALE));
    assert_eq!(corner(second), Some(BACKGROUND));
    assert_eq!(
        framebuffer.pixel(layout.list.x, layout.list.y + 4),
        Some(DIM)
    );
    // A hard shadow, not blurred, follows the box on its right and below.
    assert_eq!(
        framebuffer.pixel(layout.list.right() + 1, layout.list.y + 6),
        Some(BAR)
    );
}

#[test]
fn the_pick_moves_the_band_and_the_panel_follows_it() {
    let fixture = Fixture::new("code-render-complete-panel");
    let mut editor = editor();
    let mut session = showing(&fixture, &editor, "au");
    list_with_panel(
        &mut editor,
        &mut session,
        &["audio", "auto"],
        0..2,
        Some(panel()),
    );
    let before = editor.completion_layout(&session).unwrap();
    editor.handle_input(InputEvent::Down { select: false }, Some(&mut session));
    let after = editor.completion_layout(&session).unwrap();

    let panel_before = before.panel.as_ref().unwrap().window;
    let panel_after = after.panel.as_ref().unwrap().window;
    assert_eq!(
        panel_after.y - panel_before.y,
        after.rows[1].1.y - before.rows[0].1.y
    );
    let framebuffer = render(&editor, &session);
    assert_eq!(
        framebuffer.pixel(after.rows[1].1.right() - 1, after.rows[1].1.y),
        Some(PALE)
    );
    let foot = after.panel.unwrap().foot;
    assert_eq!(framebuffer.pixel(foot.right() - 1, foot.y), Some(BAR));
}

#[test]
fn an_overlay_hides_the_list() {
    let fixture = Fixture::new("code-render-complete-overlay");
    let mut editor = editor();
    let mut session = showing(&fixture, &editor, "au");
    list_with_panel(&mut editor, &mut session, &["audio"], 0..2, None);
    let layout = editor.completion_layout(&session).unwrap();
    let with_list = render(&editor, &session);
    editor.open_new_file(crate::cartridge::Language::Luau);
    let covered = render(&editor, &session);
    let at = (layout.list.x, layout.list.y + 4);
    assert_eq!(with_list.pixel(at.0, at.1), Some(DIM));
    assert_ne!(covered.pixel(at.0, at.1), Some(DIM));
}
