use glaux_input::InputEvent;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::system_rom::SystemRom;
use glaux_video::draw::clear;
use pretty_assertions::assert_eq;

use super::super::tests::{editor, id_of, session_with_text};
use super::*;
use crate::cartridge_fs::fixture::Fixture;
use crate::system::shell::BLINK_TICKS;

const CELL_W: i32 = 6;
const CELL_H: i32 = 10;

fn rom() -> SystemRom {
    crate::rom::load().unwrap()
}

fn render(editor: &CodeEditor, session: &CodeSession) -> Framebuffer {
    let rom = rom();
    let mut framebuffer = Framebuffer::new();
    clear(&mut framebuffer, BACKGROUND);
    editor.render(
        &mut framebuffer,
        rom.font(),
        rom.ui(),
        Some(session),
        None,
        None,
    );
    framebuffer
}

/// A session showing `text` in `a.luau`, measured for `editor`.
fn showing(fixture: &Fixture, editor: &CodeEditor, text: &str) -> CodeSession {
    let (_, mut session) = session_with_text(fixture, &["a.luau"], text);
    editor.refresh_metrics(&mut session);
    session
}

fn geometry_of(editor: &CodeEditor, session: &CodeSession) -> TextGeometry {
    let document = session.active_document().unwrap();
    TextGeometry::new(&editor.layout(), document.line_count(), &editor.settings())
}

/// Top-left of the text cell at `column` and `row` of the view.
fn cell(geometry: &TextGeometry, column: i32, row: i32) -> (i32, i32) {
    (
        geometry.viewport.x + column * CELL_W,
        geometry.viewport.y + row * CELL_H,
    )
}

/// Which pixels of the cell whose top-left is `at` are `color`.
fn ink(framebuffer: &Framebuffer, at: (i32, i32), color: u8) -> Vec<bool> {
    (0..CELL_H)
        .flat_map(|dy| (0..CELL_W).map(move |dx| (at.0 + dx, at.1 + dy)))
        .map(|(x, y)| framebuffer.pixel(x, y) == Some(color))
        .collect()
}

/// The pixels the system font sets in a cell for `ch`.
fn glyph(ch: char) -> Vec<bool> {
    let rom = rom();
    let mut framebuffer = Framebuffer::new();
    clear(&mut framebuffer, BACKGROUND);
    draw_text(&mut framebuffer, rom.font(), &ch.to_string(), 0, 0, TEXT);
    ink(&framebuffer, (0, 0), TEXT)
}

fn blank() -> Vec<bool> {
    vec![false; (CELL_W * CELL_H) as usize]
}

/// Ticks `editor`'s caret into the hidden phase of its blink, so it
/// doesn't cover the cells a test looks at.
fn hide_caret(editor: &mut CodeEditor) {
    for _ in 0..BLINK_TICKS {
        editor.tick();
    }
}

fn wrapping(editor: &mut CodeEditor) {
    editor.set_settings(EditorSettings {
        word_wrap: true,
        ..EditorSettings::default()
    });
}

#[test]
fn a_scrolled_view_starts_every_row_at_its_left_column() {
    let fixture = Fixture::new("code-render-left");
    let editor = editor();
    let line: String = ('a'..='z').cycle().take(80).collect();
    let mut session = showing(&fixture, &editor, &format!("{line}\n{line}\n"));
    let id = id_of(&session, "a.luau");
    session.tabs_mut().view_mut(id).unwrap().left_column = 10;

    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    let last = geometry.columns as i32 - 1;
    let at_last = line.chars().nth(10 + last as usize).unwrap();
    assert_eq!(ink(&framebuffer, cell(&geometry, 0, 0), TEXT), glyph('k'));
    assert_eq!(
        ink(&framebuffer, cell(&geometry, last, 1), TEXT),
        glyph(at_last)
    );
    // Line numbers stay put however far the text scrolls.
    let number = (geometry.number_right() - CELL_W, geometry.viewport.y);
    assert_eq!(ink(&framebuffer, number, IDLE_SHADE), glyph('1'));
}

#[test]
fn text_never_draws_outside_its_viewport() {
    let fixture = Fixture::new("code-render-clip");
    let line = "\tlocal x = 1 -- a comment\tthat runs\tfar past the edge of the view";
    let text = format!("{line}\n").repeat(300);
    for wrap in [false, true] {
        let mut editor = editor();
        if wrap {
            wrapping(&mut editor);
        }
        let mut session = showing(&fixture, &editor, &text);
        let id = id_of(&session, "a.luau");
        *session.tabs_mut().view_mut(id).unwrap() = DocumentView {
            top_line: 150,
            top_row: 1,
            left_column: 7,
        };
        editor.refresh_metrics(&mut session);

        let framebuffer = render(&editor, &session);
        let geometry = geometry_of(&editor, &session);
        let area = editor.layout().text;
        let mut inked = 0;
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                if framebuffer.pixel(x, y) == Some(TEXT) {
                    inked += 1;
                    assert!(
                        geometry.viewport.contains_xy(x, y),
                        "text ink at ({x}, {y}) with wrap {wrap}"
                    );
                }
            }
        }
        assert!(inked > 0, "the view shows text with wrap {wrap}");
    }
}

#[test]
fn only_the_first_row_of_a_wrapped_line_is_numbered() {
    let fixture = Fixture::new("code-render-wrap");
    let mut editor = editor();
    wrapping(&mut editor);
    // Folded at 48 columns, line 1 takes three rows.
    let session = showing(&fixture, &editor, &format!("{}\ny", "x".repeat(100)));
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    let number = |row| {
        let at = (
            geometry.number_right() - CELL_W,
            geometry.viewport.y + row * CELL_H,
        );
        ink(&framebuffer, at, IDLE_SHADE)
    };
    assert_eq!(number(0), glyph('1'));
    assert_eq!(number(1), blank());
    assert_eq!(number(2), blank());
    assert_eq!(number(3), glyph('2'));
    assert_eq!(ink(&framebuffer, cell(&geometry, 0, 3), TEXT), glyph('y'));
}

#[test]
fn characters_the_font_lacks_show_as_boxes_in_their_own_cells() {
    let fixture = Fixture::new("code-render-unknown");
    let mut editor = editor();
    hide_caret(&mut editor);
    // The byte order mark is not shown. The tab after "é" reaches column
    // 4, and the bell character after "x" gets a box of its own.
    let session = showing(&fixture, &editor, "\u{feff}é\tx\u{7}");
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    let at = |column| cell(&geometry, column, 0);

    assert_eq!(ink(&framebuffer, at(0), TEXT), blank());
    assert!(ink(&framebuffer, at(0), DIM).contains(&true), "a box");
    for column in 1..4 {
        assert_eq!(ink(&framebuffer, at(column), TEXT), blank());
        assert_eq!(ink(&framebuffer, at(column), DIM), blank());
    }
    assert_eq!(ink(&framebuffer, at(4), TEXT), glyph('x'));
    assert!(ink(&framebuffer, at(5), DIM).contains(&true), "a box");
}

#[test]
fn indicators_show_only_where_text_overflows() {
    let fixture = Fixture::new("code-render-indicators");
    let has = |framebuffer: &Framebuffer, track: Rect, color| {
        (track.y..track.bottom())
            .flat_map(|y| (track.x..track.right()).map(move |x| (x, y)))
            .any(|(x, y)| framebuffer.pixel(x, y) == Some(color))
    };
    let mut editor = editor();

    let short = showing(&fixture, &editor, "x\ny\n");
    let framebuffer = render(&editor, &short);
    let geometry = geometry_of(&editor, &short);
    assert!(!has(&framebuffer, geometry.vertical, BAR));
    assert!(!has(&framebuffer, geometry.horizontal, BAR));

    let text = format!("{}\n", "x".repeat(100)).repeat(50);
    let long = showing(&fixture, &editor, &text);
    let framebuffer = render(&editor, &long);
    let geometry = geometry_of(&editor, &long);
    for track in [geometry.vertical, geometry.horizontal] {
        assert!(has(&framebuffer, track, BAR), "{track:?} track");
        assert!(has(&framebuffer, track, DIM), "{track:?} thumb");
    }

    // Folded lines have nothing to the side.
    wrapping(&mut editor);
    let mut long = long;
    editor.refresh_metrics(&mut long);
    let framebuffer = render(&editor, &long);
    let geometry = geometry_of(&editor, &long);
    assert!(has(&framebuffer, geometry.vertical, DIM));
    assert!(!has(&framebuffer, geometry.horizontal, BAR));
}

#[test]
fn the_caret_is_a_blinking_line_on_the_left_edge_of_its_cell() {
    let fixture = Fixture::new("code-render-caret");
    let mut editor = editor();
    let mut session = showing(&fixture, &editor, "ab\ncd");
    editor.handle_input(InputEvent::Down, Some(&mut session));
    editor.handle_input(InputEvent::End { select: false }, Some(&mut session));
    let geometry = geometry_of(&editor, &session);
    // After "cd": the empty third cell of the second row.
    let mut caret = blank();
    for row in 0..CELL_H as usize {
        caret[row * CELL_W as usize] = true;
    }
    let at = cell(&geometry, 2, 1);
    assert_eq!(ink(&render(&editor, &session), at, TEXT), caret);

    hide_caret(&mut editor);
    assert_eq!(ink(&render(&editor, &session), at, TEXT), blank());
    for _ in 0..BLINK_TICKS {
        editor.tick();
    }
    assert_eq!(ink(&render(&editor, &session), at, TEXT), caret);
    // Moving shows it again at once.
    hide_caret(&mut editor);
    editor.handle_input(InputEvent::Left { select: false }, Some(&mut session));
    editor.handle_input(InputEvent::Right { select: false }, Some(&mut session));
    assert_eq!(ink(&render(&editor, &session), at, TEXT), caret);
}

#[test]
fn the_caret_hides_while_an_overlay_takes_the_keys() {
    let fixture = Fixture::new("code-render-caret-overlay");
    let mut editor = editor();
    let session = showing(&fixture, &editor, "");
    let geometry = geometry_of(&editor, &session);
    let at = cell(&geometry, 0, 0);
    assert!(ink(&render(&editor, &session), at, TEXT).contains(&true));
    editor.open_picker(Ok(Default::default()));
    // The picker window is centred and does not reach the first cell.
    assert_eq!(ink(&render(&editor, &session), at, TEXT), blank());
}
