//! How the symbol panel shows.

use pretty_assertions::assert_eq;

use super::*;
use crate::system::workspace::BAR;

const TEXT_WITH_SYMBOLS: &str = "local a = 1\nfunction go() end\nlocal function stop() end\n";

fn opened(fixture: &Fixture) -> (CodeEditor, CodeSession) {
    let mut editor = editor();
    hide_caret(&mut editor);
    let mut session = showing(fixture, &editor, TEXT_WITH_SYMBOLS);
    editor.handle_input(InputEvent::GoToSymbol, Some(&mut session));
    (editor, session)
}

#[test]
fn the_panel_is_drawn_beside_the_text_and_takes_its_width() {
    let fixture = Fixture::new("code-render-symbols-width");
    let (editor, session) = opened(&fixture);
    let layout = editor.panel_layout().expect("open");
    assert_eq!(editor.text_area().right(), layout.area.x);
    assert_eq!(layout.area.right(), editor.layout().text.right());

    let framebuffer = render(&editor, &session);
    // The rule on the panel's left edge.
    assert_eq!(
        framebuffer.pixel(layout.area.x, layout.first_row.y + 2),
        Some(BAR)
    );
}

#[test]
fn the_highlighted_declaration_is_set_off_by_a_band() {
    let fixture = Fixture::new("code-render-symbols-band");
    let (mut editor, session) = opened(&fixture);
    let layout = editor.panel_layout().unwrap();
    // The caret starts at `a`, the first declaration, and the panel has the keys.
    let framebuffer = render(&editor, &session);
    let band = |n: usize| {
        let row = layout.row(n);
        // The last column of a row is blank in every glyph, so only a band shows there.
        framebuffer.pixel(layout.area.right() - 1, row.y + 1)
    };
    assert_eq!(band(0), Some(BAR));
    assert_eq!(band(1), Some(BACKGROUND));

    let mut session = session;
    editor.handle_input(InputEvent::Down { select: false }, Some(&mut session));
    let framebuffer = render(&editor, &session);
    let row = |n: usize| framebuffer.pixel(layout.area.right() - 1, layout.row(n).y + 1);
    assert_eq!(row(0), Some(BACKGROUND));
    assert_eq!(row(1), Some(BAR));
}

#[test]
fn each_row_shows_the_name_and_the_line_of_its_declaration() {
    let fixture = Fixture::new("code-render-symbols-rows");
    let (editor, session) = opened(&fixture);
    let layout = editor.panel_layout().unwrap();
    let framebuffer = render(&editor, &session);
    let inked = |x: i32, row: usize| {
        let at = (x, layout.row(row).y);
        ink_where(&framebuffer, at, |pixel| {
            pixel != BACKGROUND && pixel != BAR
        })
        .into_iter()
        .any(|set| set)
    };
    // The kind letter and the name of every row, and the line number at its end.
    for row in 0..3 {
        assert!(inked(layout.area.x + 3, row), "kind of row {row}");
        assert!(
            inked(layout.area.x + 3 + 2 * CELL_W, row),
            "name of row {row}"
        );
        assert!(
            inked(layout.area.right() - 3 - CELL_W, row),
            "line of row {row}"
        );
    }
    // Nothing is listed below the last declaration.
    assert!(!inked(layout.area.x + 3, 3));
}
