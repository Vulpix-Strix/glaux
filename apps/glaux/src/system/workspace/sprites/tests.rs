use super::*;
use glaux_input::Point;
use pretty_assertions::assert_eq;
use sheet::Zoom;

const METRICS: Metrics = Metrics {
    cell_w: 6,
    cell_h: 10,
    top: 16,
};

fn editor() -> SpriteEditor {
    SpriteEditor::new(METRICS)
}

fn click(editor: &mut SpriteEditor, rect: crate::system::workspace::Rect) {
    editor.handle_input(
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            at: Point::new(
                (rect.x + rect.width / 2) as u16,
                (rect.y + rect.height / 2) as u16,
            ),
        },
        None,
    );
}

#[test]
fn clicking_a_visible_tile_selects_it_without_scrolling() {
    let mut editor = editor();
    editor.handle_input(InputEvent::Wheel { delta: -5 }, None);
    let before = editor.sheet;
    let layout = editor.layout();
    click(&mut editor, layout.sheet_cell(4, 21, Zoom::X1));
    assert_eq!(editor.selected(), (10 + 21) * 16 + 4);
    assert_eq!(editor.sheet, before);
}

#[test]
fn the_wheel_scrolls_rows_and_shift_wheel_pans_at_two_x() {
    let mut editor = editor();
    editor.handle_input(InputEvent::Wheel { delta: -3 }, None);
    assert_eq!(editor.sheet.top_row(), 6);
    editor.handle_input(InputEvent::Wheel { delta: 1 }, None);
    assert_eq!(editor.sheet.top_row(), 4);
    editor.handle_input(InputEvent::WheelHorizontal { delta: -2 }, None);
    assert_eq!(editor.sheet.left_column(), 0, "1x shows every column");

    editor.handle_input(InputEvent::Text('+'), None);
    assert_eq!(editor.sheet.zoom(), Zoom::X2);
    editor.handle_input(InputEvent::WheelHorizontal { delta: -2 }, None);
    assert_eq!(editor.sheet.left_column(), 2);
    editor.handle_input(InputEvent::WheelHorizontal { delta: -100 }, None);
    assert_eq!(editor.sheet.left_column(), 5);
    editor.handle_input(InputEvent::Wheel { delta: i8::MIN }, None);
    assert_eq!(editor.sheet.top_row(), 64 - 11);
}

#[test]
fn arrows_move_the_selection_and_reveal_it_minimally() {
    let mut editor = editor();
    editor.handle_input(InputEvent::Left { select: false }, None);
    editor.handle_input(InputEvent::Up, None);
    assert_eq!(
        editor.selected(),
        0,
        "the grid's corner stops the selection"
    );

    for _ in 0..22 {
        editor.handle_input(InputEvent::Down, None);
    }
    assert_eq!(editor.selected(), 22 * 16);
    assert_eq!(editor.sheet.top_row(), 1);

    for _ in 0..20 {
        editor.handle_input(InputEvent::Right { select: false }, None);
    }
    assert_eq!(editor.selected(), 22 * 16 + 15);
    assert_eq!(editor.sheet.top_row(), 1);
}

#[test]
fn zooming_keeps_the_selection_on_screen() {
    let mut editor = editor();
    let layout = editor.layout();
    click(&mut editor, layout.sheet_cell(15, 20, Zoom::X1));
    editor.handle_input(InputEvent::Text('+'), None);
    let visible = editor.layout().visible;
    assert!(editor.sheet.is_visible(editor.selected(), visible));
    editor.handle_input(InputEvent::Text('+'), None);
    assert_eq!(editor.sheet.zoom(), Zoom::X2, "2x is the largest zoom");
    editor.handle_input(InputEvent::Text('-'), None);
    editor.handle_input(InputEvent::Text('-'), None);
    assert_eq!(editor.sheet.zoom(), Zoom::X1);
}

#[test]
fn advanced_mode_keeps_zoom_and_scroll() {
    let mut editor = editor();
    editor.handle_input(InputEvent::Text('+'), None);
    editor.handle_input(InputEvent::Wheel { delta: -10 }, None);
    editor.handle_input(InputEvent::WheelHorizontal { delta: -3 }, None);
    let before = editor.sheet;
    editor.handle_input(InputEvent::Text('a'), None);
    assert!(editor.is_advanced());
    assert_eq!(editor.sheet, before);
    editor.handle_input(InputEvent::Text('a'), None);
    assert_eq!(editor.sheet, before);
}

#[test]
fn options_buttons_and_swatches_respond_to_clicks() {
    let mut editor = editor();
    let layout = editor.layout();
    click(&mut editor, layout.grid_button);
    assert!(!editor.grid);
    click(&mut editor, layout.zoom_in);
    assert_eq!(editor.sheet.zoom(), Zoom::X2);
    click(&mut editor, layout.swatches[9]);
    assert_eq!(editor.color(), 9);
    click(&mut editor, layout.advanced_button);
    assert!(editor.is_advanced());
}

// Drawing.

use crate::system::video::{CartridgeVideoDefaults, VideoFiles};

/// A document whose tile 0 is solid local 5 and every other tile empty.
fn document() -> SpriteDocument {
    let files = VideoFiles {
        obj_tiles: Some(vec![0x55; glaux_machine::sprite::OBJ_TILE_BYTES]),
        ..VideoFiles::default()
    };
    let defaults = CartridgeVideoDefaults::from_rom(&crate::rom::load().unwrap());
    SpriteDocument::from_files(&files, &defaults).unwrap()
}

/// The screen point in the middle of tile pixel `(x, y)`, which may lie
/// off the canvas.
fn canvas_point(editor: &SpriteEditor, x: i32, y: i32) -> Point {
    let canvas = editor.canvas_rect();
    let half = layout::CANVAS_ZOOM / 2;
    Point::new(
        (canvas.x + x * layout::CANVAS_ZOOM + half) as u16,
        (canvas.y + y * layout::CANVAS_ZOOM + half) as u16,
    )
}

fn down(editor: &mut SpriteEditor, doc: &mut SpriteDocument, x: i32, y: i32) {
    let at = canvas_point(editor, x, y);
    editor.handle_input(
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            at,
        },
        Some(doc),
    );
}

fn drag(editor: &mut SpriteEditor, doc: &mut SpriteDocument, x: i32, y: i32) {
    let at = canvas_point(editor, x, y);
    editor.handle_input(InputEvent::PointerMove(Some(at)), Some(doc));
}

fn up(editor: &mut SpriteEditor, doc: &mut SpriteDocument, x: i32, y: i32) {
    let at = canvas_point(editor, x, y);
    editor.handle_input(
        InputEvent::PointerUp {
            button: PointerButton::Primary,
            at,
        },
        Some(doc),
    );
}

/// Tile `tile` as one string of hex digits per pixel row.
fn rows(doc: &SpriteDocument, tile: usize) -> Vec<String> {
    (0..8)
        .map(|y| {
            (0..8)
                .map(|x| format!("{:X}", doc.pixel(tile, x, y).unwrap()))
                .collect()
        })
        .collect()
}

#[test]
fn a_fast_diagonal_drag_paints_every_pixel_between() {
    let mut editor = editor();
    let mut doc = document();
    editor.select(1, None);
    down(&mut editor, &mut doc, 0, 0);
    drag(&mut editor, &mut doc, 7, 7);
    up(&mut editor, &mut doc, 7, 7);
    for n in 0..8 {
        assert_eq!(doc.pixel(1, n, n), Some(1), "pixel ({n}, {n})");
    }
    assert_eq!(rows(&doc, 1).concat().matches('1').count(), 8);
}

#[test]
fn a_fast_horizontal_drag_leaves_no_gaps() {
    let mut editor = editor();
    let mut doc = document();
    editor.handle_input(InputEvent::Text('e'), None);
    down(&mut editor, &mut doc, 0, 3);
    drag(&mut editor, &mut doc, 5, 3);
    drag(&mut editor, &mut doc, 7, 3);
    up(&mut editor, &mut doc, 7, 3);
    assert_eq!(rows(&doc, 0)[3], "00000000");
    assert_eq!(rows(&doc, 0)[2], "55555555");
}

#[test]
fn a_drag_off_the_canvas_paints_to_the_edge_and_resumes_on_return() {
    let mut editor = editor();
    let mut doc = document();
    editor.select(1, None);
    down(&mut editor, &mut doc, 5, 1);
    drag(&mut editor, &mut doc, 11, 1);
    drag(&mut editor, &mut doc, 11, 5);
    drag(&mut editor, &mut doc, 5, 5);
    up(&mut editor, &mut doc, 5, 5);
    assert_eq!(
        rows(&doc, 1),
        [
            "00000000", //
            "00000111", //
            "00000000", //
            "00000000", //
            "00000000", //
            "00000111", //
            "00000000", //
            "00000000", //
        ]
    );
    assert_eq!(
        rows(&doc, 2).concat(),
        "0".repeat(64),
        "never the next tile"
    );
}

#[test]
fn moves_after_a_release_or_cancel_do_not_paint() {
    let mut editor = editor();
    let mut doc = document();
    editor.select(1, None);
    down(&mut editor, &mut doc, 0, 0);
    up(&mut editor, &mut doc, 0, 0);
    drag(&mut editor, &mut doc, 3, 0);
    assert_eq!(rows(&doc, 1)[0], "10000000");

    down(&mut editor, &mut doc, 0, 2);
    drag(&mut editor, &mut doc, 2, 2);
    editor.handle_input(InputEvent::PointerCancel, Some(&mut doc));
    drag(&mut editor, &mut doc, 6, 2);
    up(&mut editor, &mut doc, 6, 2);
    assert_eq!(rows(&doc, 1)[2], "11100000", "what was drawn stays");
}

#[test]
fn a_press_without_a_release_starts_a_new_stroke() {
    let mut editor = editor();
    let mut doc = document();
    editor.select(1, None);
    down(&mut editor, &mut doc, 0, 0);
    // The release was lost; the next press must not join the two.
    down(&mut editor, &mut doc, 7, 7);
    drag(&mut editor, &mut doc, 7, 6);
    assert_eq!(rows(&doc, 1).concat().matches('1').count(), 3);
    assert_eq!(doc.pixel(1, 3, 3), Some(0));
}

#[test]
fn changing_tool_or_tile_ends_the_stroke() {
    let mut editor = editor();
    let mut doc = document();
    editor.select(1, None);
    down(&mut editor, &mut doc, 0, 0);
    editor.handle_input(InputEvent::Text('e'), Some(&mut doc));
    drag(&mut editor, &mut doc, 4, 0);
    assert_eq!(rows(&doc, 1)[0], "10000000");

    editor.handle_input(InputEvent::Text('p'), Some(&mut doc));
    down(&mut editor, &mut doc, 0, 1);
    editor.handle_input(InputEvent::Down, Some(&mut doc));
    drag(&mut editor, &mut doc, 4, 1);
    assert_eq!(rows(&doc, 1)[1], "10000000");
    assert_eq!(rows(&doc, 17)[1], "00000000");
}

#[test]
fn the_eyedropper_picks_colors_including_transparent() {
    let mut editor = editor();
    let mut doc = document();
    editor.handle_input(InputEvent::Text('I'), None);
    assert_eq!(editor.tool, ToolSlot::Eyedropper);
    down(&mut editor, &mut doc, 2, 2);
    assert_eq!(editor.color(), 5);
    editor.select(1, None);
    down(&mut editor, &mut doc, 2, 2);
    assert_eq!(editor.color(), 0);
    assert!(!doc.is_modified());
}

#[test]
fn fill_acts_once_per_press() {
    let mut editor = editor();
    let mut doc = document();
    editor.handle_input(InputEvent::Text('f'), None);
    editor.color = 9;
    down(&mut editor, &mut doc, 4, 4);
    assert!(rows(&doc, 0).iter().all(|row| row == "99999999"));
    assert!(rows(&doc, 1).iter().all(|row| row == "00000000"));
    assert_eq!(editor.stroke, None);
}

#[test]
fn keys_and_tool_plates_select_tools() {
    let mut editor = editor();
    assert_eq!(editor.tool, ToolSlot::Pencil);
    for (key, tool) in [
        ('e', ToolSlot::Eraser),
        ('i', ToolSlot::Eyedropper),
        ('F', ToolSlot::Fill),
        ('p', ToolSlot::Pencil),
    ] {
        editor.handle_input(InputEvent::Text(key), None);
        assert_eq!(editor.tool, tool, "{key}");
    }
    let layout = editor.layout();
    for (tool, rect) in layout.tools {
        click(&mut editor, rect);
        assert_eq!(editor.tool, tool);
    }
}

#[test]
fn without_a_document_the_canvas_does_nothing() {
    let mut editor = editor();
    let before = editor.clone();
    let canvas = editor.layout().canvas;
    click(&mut editor, canvas);
    assert_eq!(editor, before);
}

// Undo, redo and save.

fn command(
    editor: &mut SpriteEditor,
    doc: &mut SpriteDocument,
    event: InputEvent,
) -> Option<SpriteRequest> {
    editor.handle_input(event, Some(doc))
}

#[test]
fn a_pencil_or_eraser_drag_undoes_as_one() {
    for key in ['p', 'e'] {
        let mut editor = editor();
        let mut doc = document();
        editor.handle_input(InputEvent::Text(key), None);
        down(&mut editor, &mut doc, 0, 0);
        drag(&mut editor, &mut doc, 4, 0);
        drag(&mut editor, &mut doc, 4, 3);
        up(&mut editor, &mut doc, 4, 3);
        assert!(doc.is_modified(), "{key}");
        assert_eq!(command(&mut editor, &mut doc, InputEvent::Undo), None);
        assert_eq!(rows(&doc, 0).concat(), "5".repeat(64), "{key}");
        assert!(!doc.can_undo(), "{key}: one drag, one edit");
        command(&mut editor, &mut doc, InputEvent::Redo);
        assert!(doc.is_modified(), "{key}");
    }
}

#[test]
fn a_fill_undoes_as_one_and_the_eyedropper_records_nothing() {
    let mut editor = editor();
    let mut doc = document();
    editor.handle_input(InputEvent::Text('i'), None);
    down(&mut editor, &mut doc, 1, 1);
    up(&mut editor, &mut doc, 1, 1);
    assert!(!doc.can_undo());

    editor.handle_input(InputEvent::Text('f'), None);
    editor.color = 9;
    down(&mut editor, &mut doc, 1, 1);
    up(&mut editor, &mut doc, 1, 1);
    command(&mut editor, &mut doc, InputEvent::Undo);
    assert_eq!(rows(&doc, 0).concat(), "5".repeat(64));
    assert!(!doc.can_undo());
}

#[test]
fn interrupted_strokes_stay_separate_edits() {
    let mut editor = editor();
    let mut doc = document();
    editor.select(1, None);
    down(&mut editor, &mut doc, 0, 0);
    drag(&mut editor, &mut doc, 2, 0);
    editor.handle_input(InputEvent::PointerCancel, Some(&mut doc));
    // A release that never arrived: the next press starts anew.
    down(&mut editor, &mut doc, 0, 2);
    drag(&mut editor, &mut doc, 2, 2);
    down(&mut editor, &mut doc, 0, 4);
    up(&mut editor, &mut doc, 0, 4);

    let mut steps = Vec::new();
    while doc.undo() {
        steps.push(rows(&doc, 1)[..5].concat());
    }
    assert_eq!(
        steps,
        [
            format!("{}{}{}", "11100000", "0".repeat(8), "11100000") + &"0".repeat(16),
            "11100000".to_owned() + &"0".repeat(32),
            "0".repeat(40),
        ]
    );
}

#[test]
fn undo_during_a_drag_undoes_the_whole_drag_so_far() {
    let mut editor = editor();
    let mut doc = document();
    editor.select(1, None);
    down(&mut editor, &mut doc, 0, 0);
    drag(&mut editor, &mut doc, 3, 0);
    command(&mut editor, &mut doc, InputEvent::Undo);
    assert_eq!(rows(&doc, 1)[0], "00000000");
    drag(&mut editor, &mut doc, 6, 0);
    assert_eq!(rows(&doc, 1)[0], "00000000", "the drag ended");
}

#[test]
fn save_is_asked_for_only_with_unsaved_edits() {
    let mut editor = editor();
    let mut doc = document();
    assert_eq!(
        command(&mut editor, &mut doc, InputEvent::Save),
        Some(SpriteRequest::Notice("Nothing to save"))
    );
    assert_eq!(
        command(&mut editor, &mut doc, InputEvent::Undo),
        Some(SpriteRequest::Notice("Nothing to undo"))
    );
    assert_eq!(
        command(&mut editor, &mut doc, InputEvent::Redo),
        Some(SpriteRequest::Notice("Nothing to redo"))
    );
    down(&mut editor, &mut doc, 0, 0);
    // Saving mid-drag ends the drag first, so it is saved whole.
    assert_eq!(
        command(&mut editor, &mut doc, InputEvent::Save),
        Some(SpriteRequest::Save)
    );
    drag(&mut editor, &mut doc, 4, 0);
    assert_eq!(rows(&doc, 0)[0], "15555555");
    assert!(doc.can_undo());

    let button = editor.layout().save_button;
    let at = Point::new(
        (button.x + button.width / 2) as u16,
        (button.y + button.height / 2) as u16,
    );
    let press = InputEvent::PointerDown {
        button: PointerButton::Primary,
        at,
    };
    assert_eq!(
        editor.handle_input(press, Some(&mut doc)),
        Some(SpriteRequest::Save)
    );
    assert_eq!(editor.handle_input(press, None), None);
}
