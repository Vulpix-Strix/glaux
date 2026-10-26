//! How selected text shows.

use std::ops::Range;

use super::*;
use crate::system::editing::cursor::Caret;
use crate::system::editing::selection::Selection;
use pretty_assertions::assert_eq;

fn select(session: &mut CodeSession, range: Range<usize>) {
    let id = session.tabs().active().unwrap();
    let document = session.active_document().unwrap();
    let selection = Selection::between(document, range.start, Caret::at(document, range.end));
    session.tabs_mut().set_selection(id, selection);
}

/// The cells of the first `count` columns of the view's first row: the
/// pixels drawn in `color` in each.
fn row_ink(
    framebuffer: &Framebuffer,
    geometry: &TextGeometry,
    count: i32,
    color: u8,
) -> Vec<Vec<bool>> {
    (0..count)
        .map(|column| {
            let at = cell(geometry, column, 0);
            if color == TEXT {
                // A comment in the text draws in `DIM`.
                ink_where(framebuffer, at, |pixel| is_text_ink(pixel) || pixel == DIM)
            } else {
                ink(framebuffer, at, color)
            }
        })
        .collect()
}

fn inverted(cell: &[bool]) -> Vec<bool> {
    cell.iter().map(|&set| !set).collect()
}

/// Renders the one-line `text` with and without `range` selected, and
/// checks that exactly the cells of the selected characters show their
/// glyph, as shaped with its neighbours, inverted. Returns the glyphs as
/// shaped.
fn assert_selects(name: &str, text: &str, range: Range<usize>) -> Vec<Vec<bool>> {
    let fixture = Fixture::new(name);
    let mut editor = editor();
    hide_caret(&mut editor);
    let mut session = showing(&fixture, &editor, text);
    let geometry = geometry_of(&editor, &session);
    let count = text.chars().count() as i32;
    let shaped = row_ink(&render(&editor, &session), &geometry, count, TEXT);
    select(&mut session, range.clone());
    let selected = row_ink(&render(&editor, &session), &geometry, count, TEXT);
    for ((byte, ch), (shaped, selected)) in text.char_indices().zip(shaped.iter().zip(&selected)) {
        if range.contains(&byte) {
            assert_eq!(
                selected,
                &inverted(shaped),
                "{text:?}: {ch:?} at {byte} is selected"
            );
        } else {
            assert_eq!(
                selected, shaped,
                "{text:?}: {ch:?} at {byte} is not selected"
            );
        }
    }
    shaped
}

#[test]
fn selected_characters_show_inverted_in_their_own_cells() {
    assert_selects("code-render-sel-plain", "local x", 2..5);
    assert_selects("code-render-sel-start", "local x", 0..1);
}

#[test]
fn part_of_a_ligature_selects_only_its_own_characters() {
    // Each glyph keeps the shape the ligature gave it; only the selected
    // characters' cells invert.
    let shaped = assert_selects("code-render-sel-arrow", "a -> b", 3..4);
    assert_ne!(shaped[3], glyph('>'), "\"->\" draws as one arrow");
    assert_selects("code-render-sel-arrow-head", "a -> b", 2..3);
    let shaped = assert_selects("code-render-sel-triple", "x === y", 3..4);
    assert_ne!(shaped[3], glyph('='), "\"===\" draws as a ligature");
    assert_selects("code-render-sel-comment", "<!-- c", 1..3);
}

#[test]
fn part_of_a_contextual_alternate_selects_only_its_own_characters() {
    // In "0xFF" the x between the digits takes its alternate glyph.
    let shaped = assert_selects("code-render-sel-hex", "0xFF", 1..2);
    assert_ne!(shaped[1], glyph('x'), "x takes its contextual alternate");
    assert_selects("code-render-sel-hex-zero", "0xFF", 0..1);
    // In "A:B" the colon between capitals is raised.
    let shaped = assert_selects("code-render-sel-colon", "A:B", 0..2);
    assert_ne!(shaped[1], glyph(':'), "the colon takes its alternate");
}

#[test]
fn a_selected_line_ending_takes_one_cell_after_its_line() {
    let fixture = Fixture::new("code-render-sel-eol");
    let mut editor = editor();
    hide_caret(&mut editor);
    let mut session = showing(&fixture, &editor, "ab\n\ncd");
    let geometry = geometry_of(&editor, &session);
    let full = vec![true; (CELL_W * CELL_H) as usize];
    // From "b" through the empty line to "c".
    select(&mut session, 1..5);
    let framebuffer = render(&editor, &session);
    let at = |column, row| ink(&framebuffer, cell(&geometry, column, row), TEXT);
    assert_eq!(at(2, 0), full, "the first line's ending");
    assert_eq!(at(3, 0), blank());
    assert_eq!(at(0, 1), full, "the empty line's ending");
    assert_eq!(at(1, 1), blank());
    assert_eq!(at(1, 2), glyph('d'), "past the selection");

    // The last line has no ending to show.
    select(&mut session, 0..7);
    let framebuffer = render(&editor, &session);
    assert_eq!(ink(&framebuffer, cell(&geometry, 2, 2), TEXT), blank());
}

#[test]
fn a_selected_tab_fills_every_column_it_takes() {
    let fixture = Fixture::new("code-render-sel-tab");
    let mut editor = editor();
    hide_caret(&mut editor);
    let mut session = showing(&fixture, &editor, "\tx");
    let geometry = geometry_of(&editor, &session);
    select(&mut session, 0..1);
    let framebuffer = render(&editor, &session);
    let full = vec![true; (CELL_W * CELL_H) as usize];
    for column in 0..4 {
        assert_eq!(ink(&framebuffer, cell(&geometry, column, 0), TEXT), full);
    }
    assert_eq!(ink(&framebuffer, cell(&geometry, 4, 0), TEXT), glyph('x'));
}

#[test]
fn the_caret_inverts_where_it_starts_a_selection() {
    let fixture = Fixture::new("code-render-sel-caret");
    let mut editor = editor();
    let mut session = showing(&fixture, &editor, "abc");
    editor.handle_input(InputEvent::End { select: false }, Some(&mut session));
    editor.handle_input(InputEvent::Left { select: true }, Some(&mut session));
    let geometry = geometry_of(&editor, &session);
    let (x, y) = cell(&geometry, 2, 0);
    let edge = |framebuffer: &Framebuffer| -> Vec<Option<u8>> {
        (0..CELL_H).map(|dy| framebuffer.pixel(x, y + dy)).collect()
    };
    let lit = render(&editor, &session);
    assert_eq!(edge(&lit), vec![Some(BACKGROUND); CELL_H as usize]);
    hide_caret(&mut editor);
    let hidden = render(&editor, &session);
    assert!(
        edge(&hidden).contains(&Some(TEXT)),
        "the selection under it"
    );
}
