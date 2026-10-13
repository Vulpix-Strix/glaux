use glaux_input::InputEvent;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::system_rom::SystemRom;
use glaux_video::draw::clear;
use pretty_assertions::assert_eq;

use super::super::tests::{editor, id_of, session_with_text};
use super::*;
use crate::cartridge_fs::fixture::Fixture;
use crate::system::shell::BLINK_TICKS;

mod selection;

const CELL_W: i32 = 6;
const CELL_H: i32 = 12;

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
    TextGeometry::new(editor.layout(), document.line_count(), &editor.settings())
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
        editor.tick(None);
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
fn characters_the_font_lacks_show_as_dim_fallbacks_in_their_own_cells() {
    let fixture = Fixture::new("code-render-unknown");
    let mut editor = editor();
    hide_caret(&mut editor);
    // The byte order mark is not shown. "中" is outside the font, the tab
    // after it reaches column 4, the bell after "x" is a control character,
    // and "✅" takes two cells, so "y" lands on column 8.
    let session = showing(&fixture, &editor, "\u{feff}中\tx\u{7}✅y");
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    let at = |column| cell(&geometry, column, 0);
    let fallback = glyph('\u{FFFD}');

    assert_eq!(ink(&framebuffer, at(0), DIM), fallback);
    assert_eq!(ink(&framebuffer, at(0), TEXT), blank());
    for column in 1..4 {
        assert_eq!(ink(&framebuffer, at(column), TEXT), blank());
        assert_eq!(ink(&framebuffer, at(column), DIM), blank());
    }
    assert_eq!(ink(&framebuffer, at(4), TEXT), glyph('x'));
    assert_eq!(ink(&framebuffer, at(5), DIM), fallback);
    for column in [6, 7] {
        assert!(ink(&framebuffer, at(column), TEXT).contains(&true), "✅");
    }
    assert_eq!(ink(&framebuffer, at(8), TEXT), glyph('y'));
}

#[test]
fn ligatures_redraw_only_the_cells_of_their_characters() {
    let fixture = Fixture::new("code-render-ligatures");
    let mut editor = editor();
    hide_caret(&mut editor);
    let session = showing(&fixture, &editor, "a -> b");
    let geometry = geometry_of(&editor, &session);
    let cells = |framebuffer: &Framebuffer| -> Vec<Vec<bool>> {
        (0..6)
            .map(|column| ink(framebuffer, cell(&geometry, column, 0), TEXT))
            .collect()
    };
    let plain: Vec<_> = "a -> b".chars().map(glyph).collect();

    assert!(editor.settings().ligatures, "ligatures are on by default");
    let joined = cells(&render(&editor, &session));
    assert_ne!(joined[2..4], plain[2..4], "\"->\" draws as one arrow");
    assert_eq!(joined[..2], plain[..2]);
    assert_eq!(joined[4..], plain[4..]);

    editor.set_settings(EditorSettings {
        ligatures: false,
        ..editor.settings()
    });
    assert_eq!(cells(&render(&editor, &session)), plain);
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
    editor.handle_input(InputEvent::Down { select: false }, Some(&mut session));
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
        editor.tick(None);
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

fn render_at(editor: &CodeEditor, session: &CodeSession, pointer: Option<Point>) -> Framebuffer {
    let rom = rom();
    let mut framebuffer = Framebuffer::new();
    clear(&mut framebuffer, BACKGROUND);
    editor.render(
        &mut framebuffer,
        rom.font(),
        rom.ui(),
        Some(session),
        pointer,
        None,
    );
    framebuffer
}

/// The colors of the close glyph's cell, row by row.
fn close_cell(framebuffer: &Framebuffer, close: Rect) -> Vec<u8> {
    (0..close.height)
        .flat_map(|dy| (0..close.width).map(move |dx| (close.x + dx, close.y + dy)))
        .map(|(x, y)| framebuffer.pixel(x, y).unwrap())
        .collect()
}

/// A close cross in `ink` on the tab's ground.
fn cross(ink: u8) -> Vec<u8> {
    (0..CLOSE_SIZE)
        .flat_map(|y| (0..CLOSE_SIZE).map(move |x| (x, y)))
        .map(|(x, y)| {
            if x == y || x == CLOSE_SIZE - 1 - y {
                ink
            } else {
                BAR
            }
        })
        .collect()
}

/// The unsaved-edits dot: three by three in the cell's middle.
fn dot() -> Vec<u8> {
    (0..CLOSE_SIZE)
        .flat_map(|y| (0..CLOSE_SIZE).map(move |x| (x, y)))
        .map(|(x, y)| {
            let inside = (1..4).contains(&x) && (1..4).contains(&y);
            if inside { ACCENT } else { BAR }
        })
        .collect()
}

#[test]
fn modified_tabs_show_a_dot_where_close_is_until_hovered() {
    let fixture = Fixture::new("code-render-dirty");
    let editor = editor();
    let (cartridge, mut session) = session_with_text(&fixture, &["a.luau", "b.luau"], "x");
    let (a, b) = (id_of(&session, "a.luau"), id_of(&session, "b.luau"));
    session.edit(a, 0..0, "y").unwrap().unwrap();
    let labels = labels(&editor.layout().font, &session);
    let tabs = editor.layout().strip_tabs(&labels, 0);
    let (tab_a, tab_b) = (tabs[0], tabs[1]);

    // Idle: the modified tab shows the dot, the clean active one its cross.
    let framebuffer = render_at(&editor, &session, None);
    assert_eq!(close_cell(&framebuffer, tab_a.close), dot());
    assert_eq!(close_cell(&framebuffer, tab_b.close), cross(DIM));

    // Over the tab the cross comes back, lit as on any hovered tab, and
    // over the cross itself it warns as always.
    let label = Point::new((tab_a.rect.x + 2) as u16, (tab_a.rect.y + 2) as u16);
    let framebuffer = render_at(&editor, &session, Some(label));
    assert_eq!(close_cell(&framebuffer, tab_a.close), cross(DIM));
    let close = Point::new((tab_a.close.x + 2) as u16, (tab_a.close.y + 2) as u16);
    let framebuffer = render_at(&editor, &session, Some(close));
    assert_eq!(close_cell(&framebuffer, tab_a.close), cross(ERROR));

    // The active tab takes the dot too, and each tab follows its own
    // document: saving one leaves the other marked.
    session.edit(b, 0..0, "y").unwrap().unwrap();
    session.save(&cartridge, a).unwrap();
    let framebuffer = render_at(&editor, &session, None);
    assert_eq!(close_cell(&framebuffer, tab_a.close), cross(IDLE_SHADE));
    assert_eq!(close_cell(&framebuffer, tab_b.close), dot());

    // Text edited back to what was saved is clean again.
    session.edit(b, 0..1, "").unwrap().unwrap();
    let framebuffer = render_at(&editor, &session, None);
    assert_eq!(close_cell(&framebuffer, tab_b.close), cross(DIM));
}

#[test]
fn the_options_bar_ends_with_the_caret_position_beside_the_settings_button() {
    let fixture = Fixture::new("code-render-options");
    let editor = editor();
    let session = showing(&fixture, &editor, "x");
    let framebuffer = render(&editor, &session);

    let layout = editor.layout();
    let bar = layout.options;
    let gear = layout.settings_button();
    let y = bar.y + (bar.height - 1 - CELL_H) / 2 + 1;
    // "Ln 1  Col 1" ends one cell left of the settings button.
    assert_eq!(
        ink(&framebuffer, (gear.x - 2 * CELL_W, y), PALE),
        glyph('1')
    );
    assert_eq!(ink(&framebuffer, (gear.x - CELL_W, y), PALE), blank());
    // Nothing dim is written left of the button: no Read-only label.
    for y in bar.y..bar.bottom() - 1 {
        for x in 0..gear.x - SETTINGS_MARGIN {
            assert_ne!(framebuffer.pixel(x, y), Some(DIM), "({x}, {y})");
        }
    }
}
