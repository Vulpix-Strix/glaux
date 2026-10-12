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
            format!("{}{}{}", "11100000", "0".repeat(8), "11100000") + "0".repeat(16).as_str(),
            "11100000".to_owned() + "0".repeat(32).as_str(),
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

// Choosing a tile's palette.

use crate::system::sprite_document::ColorScope;
use glaux_machine::obj_palette::ObjBank;

fn centre_of(rect: crate::system::workspace::Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

/// A primary press and release at `at`.
fn tap(editor: &mut SpriteEditor, doc: &mut SpriteDocument, at: Point) -> Option<SpriteRequest> {
    let pressed = editor.handle_input(
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            at,
        },
        Some(doc),
    );
    let released = editor.handle_input(
        InputEvent::PointerUp {
            button: PointerButton::Primary,
            at,
        },
        Some(doc),
    );
    pressed.or(released)
}

fn bank_of(doc: &SpriteDocument, tile: usize) -> u8 {
    doc.obj().attributes().bank(tile).get()
}

fn open_picker(editor: &mut SpriteEditor, doc: &mut SpriteDocument) {
    let button = centre_of(editor.layout().palette_button);
    assert_eq!(tap(editor, doc, button), None);
    assert!(editor.has_dialog());
}

#[test]
fn the_palette_button_opens_the_picker_on_the_tiles_bank() {
    let mut editor = editor();
    let mut doc = document();
    assert!(doc.set_tile_bank(9, ObjBank::new(3).unwrap()));
    editor.selected = 9;
    open_picker(&mut editor, &mut doc);
    assert_eq!(editor.picker().unwrap().pending().get(), 3);
    assert_eq!(editor.picker().unwrap().tile(), 9);

    let mut editor = SpriteEditor::new(METRICS);
    let button = editor.layout().palette_button;
    click(&mut editor, button);
    assert!(!editor.has_dialog(), "without a document there is no tile");
}

#[test]
fn an_advanced_obp_cell_preselects_without_assigning() {
    let mut editor = editor();
    editor.handle_input(InputEvent::Text('a'), None);
    let mut doc = document();
    let cell = centre_of(editor.layout().banks[4]);
    tap(&mut editor, &mut doc, cell);
    assert_eq!(editor.picker().unwrap().pending().get(), 4);
    assert_eq!(bank_of(&doc, 0), 0, "only chosen, not assigned");

    assert_eq!(command(&mut editor, &mut doc, InputEvent::Break), None);
    assert!(!editor.has_dialog());
    assert_eq!(bank_of(&doc, 0), 0);
    assert!(!doc.is_modified() && !doc.can_undo());
}

#[test]
fn the_open_picker_takes_every_event() {
    let mut editor = editor();
    let mut doc = document();
    doc.set_pixel(0, 7, 7, 1);
    open_picker(&mut editor, &mut doc);
    let before = doc.clone();
    let canvas = canvas_point(&editor, 3, 3);
    for event in [
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            at: canvas,
        },
        InputEvent::PointerMove(Some(canvas_point(&editor, 4, 4))),
        InputEvent::PointerUp {
            button: PointerButton::Primary,
            at: canvas,
        },
        InputEvent::Text('e'),
        InputEvent::Text('g'),
        InputEvent::Undo,
        InputEvent::Redo,
        InputEvent::Save,
        InputEvent::Right { select: false },
        InputEvent::Wheel { delta: -1 },
    ] {
        assert_eq!(
            editor.handle_input(event, Some(&mut doc)),
            None,
            "{event:?}"
        );
    }
    assert!(editor.has_dialog());
    assert_eq!(doc, before);
    assert_eq!(
        (editor.tool, editor.grid, editor.selected),
        (ToolSlot::Pencil, true, 0)
    );
}

#[test]
fn applying_a_palette_is_one_undoable_edit() {
    let mut editor = editor();
    let mut doc = document();
    open_picker(&mut editor, &mut doc);
    command(&mut editor, &mut doc, InputEvent::Down);
    command(&mut editor, &mut doc, InputEvent::Down);
    assert_eq!(bank_of(&doc, 0), 0, "nothing changes before Apply");
    assert_eq!(command(&mut editor, &mut doc, InputEvent::Enter), None);
    assert!(!editor.has_dialog());
    assert_eq!(bank_of(&doc, 0), 2);
    assert!(doc.is_modified());

    assert_eq!(command(&mut editor, &mut doc, InputEvent::Undo), None);
    assert_eq!(bank_of(&doc, 0), 0);
    assert!(!doc.is_modified());
    assert_eq!(
        command(&mut editor, &mut doc, InputEvent::Undo),
        Some(SpriteRequest::Notice("Nothing to undo")),
        "one step"
    );
    command(&mut editor, &mut doc, InputEvent::Redo);
    assert_eq!(bank_of(&doc, 0), 2);
}

#[test]
fn applying_the_current_palette_or_cancelling_changes_nothing() {
    let mut editor = editor();
    let mut doc = document();
    open_picker(&mut editor, &mut doc);
    assert_eq!(command(&mut editor, &mut doc, InputEvent::Enter), None);
    assert!(!editor.has_dialog());
    assert!(!doc.is_modified() && !doc.can_undo());

    open_picker(&mut editor, &mut doc);
    command(&mut editor, &mut doc, InputEvent::Text('6'));
    let cancel = centre_of(editor.picker_layout().cancel);
    assert_eq!(tap(&mut editor, &mut doc, cancel), None);
    assert!(!editor.has_dialog());
    assert_eq!(bank_of(&doc, 0), 0);
    assert!(!doc.is_modified() && !doc.can_undo());
}

#[test]
fn choosing_and_applying_by_pointer_never_draws() {
    let mut editor = editor();
    let mut doc = document();
    let before = rows(&doc, 0);
    open_picker(&mut editor, &mut doc);
    let layout = editor.picker_layout();
    // A row lying over the canvas is chosen, not painted on.
    let row = layout
        .rows
        .iter()
        .rposition(|row| row.overlaps(editor.layout().canvas))
        .expect("a row over the canvas");
    assert_ne!(row, 0, "a bank the tile does not have");
    tap(&mut editor, &mut doc, centre_of(layout.rows[row]));
    tap(&mut editor, &mut doc, centre_of(layout.apply));
    assert!(!editor.has_dialog());
    assert_eq!(bank_of(&doc, 0), row as u8);

    // What follows the closing release reaches the editor, which holds no
    // stroke to continue.
    let canvas = canvas_point(&editor, 2, 2);
    editor.handle_input(InputEvent::PointerMove(Some(canvas)), Some(&mut doc));
    editor.handle_input(
        InputEvent::PointerUp {
            button: PointerButton::Primary,
            at: canvas,
        },
        Some(&mut doc),
    );
    assert_eq!(rows(&doc, 0), before);
}

// Editing a color.

/// Taps swatch `local` twice, `gap` ticks apart.
fn tap_swatch_twice(
    editor: &mut SpriteEditor,
    doc: &mut SpriteDocument,
    local: u8,
    gap: u32,
) -> Option<SpriteRequest> {
    let at = centre_of(editor.swatch_rect(local));
    tap(editor, doc, at);
    for _ in 0..gap {
        editor.tick();
    }
    tap(editor, doc, at)
}

#[test]
fn a_double_click_on_a_swatch_opens_edit_color_on_that_color() {
    let mut editor = editor();
    let mut doc = document();
    assert!(doc.set_tile_bank(0, ObjBank::new(2).unwrap()));
    assert_eq!(
        tap_swatch_twice(&mut editor, &mut doc, 6, DOUBLE_CLICK_TICKS),
        None
    );
    let open = editor.color_editor().expect("EDIT COLOR is open");
    assert_eq!(
        open.target(),
        ColorTarget {
            bank: ObjBank::new(2).unwrap(),
            local: 6
        }
    );
    assert_eq!(editor.color, 6, "the first click selected the color");
}

#[test]
fn slow_or_split_clicks_only_select() {
    let mut editor = editor();
    let mut doc = document();
    tap_swatch_twice(&mut editor, &mut doc, 6, DOUBLE_CLICK_TICKS + 1);
    assert!(!editor.has_dialog());

    let mut editor = SpriteEditor::new(METRICS);
    let first = centre_of(editor.swatch_rect(6));
    let second = centre_of(editor.swatch_rect(7));
    tap(&mut editor, &mut doc, first);
    tap(&mut editor, &mut doc, second);
    assert!(!editor.has_dialog());
    assert_eq!(editor.color, 7);
    // A third click on the same swatch completes a new double click.
    tap(&mut editor, &mut doc, second);
    assert!(editor.color_editor().is_some());
}

#[test]
fn c_opens_edit_color_but_transparency_has_no_color_to_edit() {
    let mut editor = editor();
    let mut doc = document();
    editor.color = 9;
    assert_eq!(command(&mut editor, &mut doc, InputEvent::Text('c')), None);
    assert_eq!(editor.color_editor().unwrap().target().local, 9);

    let mut editor = SpriteEditor::new(METRICS);
    editor.color = 0;
    assert_eq!(
        command(&mut editor, &mut doc, InputEvent::Text('c')),
        Some(SpriteRequest::Notice("Color 0 is transparent"))
    );
    assert_eq!(
        tap_swatch_twice(&mut editor, &mut doc, 0, 1),
        Some(SpriteRequest::Notice("Color 0 is transparent"))
    );
    assert!(!editor.has_dialog());

    let mut editor = SpriteEditor::new(METRICS);
    editor.handle_input(InputEvent::Text('c'), None);
    assert!(!editor.has_dialog(), "without a document there is no color");
}

#[test]
fn cancelling_edit_color_leaves_the_document_exactly_as_it_was() {
    let mut editor = editor();
    let mut doc = document();
    doc.set_pixel(0, 0, 0, 3);
    let before = doc.clone();
    command(&mut editor, &mut doc, InputEvent::Text('c'));
    let layout = editor.color_editor_layout();
    // Drag across the field, then type a color, then cancel.
    tap(&mut editor, &mut doc, centre_of(layout.sat_val));
    tap(&mut editor, &mut doc, centre_of(layout.fields[0]));
    for c in "123456".chars() {
        command(&mut editor, &mut doc, InputEvent::Text(c));
    }
    assert_eq!(
        editor.color_editor().unwrap().color(),
        Rgb::from_hex(0x123456)
    );
    tap(&mut editor, &mut doc, centre_of(layout.cancel));
    assert!(!editor.has_dialog());
    assert_eq!(doc, before);
}

#[test]
fn cancelling_after_choosing_a_scope_changes_nothing() {
    let mut editor = editor();
    let mut doc = document();
    share_local_5(&mut doc);
    let before = doc.clone();
    type_color_5(&mut editor, &mut doc, "010203");
    let layout = editor.color_editor_layout();
    for row in layout.scope_rows() {
        tap(&mut editor, &mut doc, centre_of(row));
        assert_eq!(doc, before);
    }
    tap(&mut editor, &mut doc, centre_of(layout.cancel));
    assert!(!editor.has_dialog());
    assert_eq!(doc, before, "no palette, master or history change");
}

/// Makes OBP1 local 5 show OBP0 local 5's master, so that color is shared
/// across palettes. Returns that master.
fn share_local_5(doc: &mut SpriteDocument) -> u8 {
    let master = doc.obj().palettes().resolve(ObjBank::ZERO, 5).unwrap();
    doc.set_obj_color(ObjBank::new(1).unwrap(), 5, master);
    doc.end_edit();
    master
}

/// Opens EDIT COLOR on swatch 5 and types `hex`, confirmed.
fn type_color_5(editor: &mut SpriteEditor, doc: &mut SpriteDocument, hex: &str) {
    tap_swatch_twice(editor, doc, 5, 0);
    let layout = editor.color_editor_layout();
    tap(editor, doc, centre_of(layout.fields[0]));
    for c in hex.chars() {
        command(editor, doc, InputEvent::Text(c));
    }
    command(editor, doc, InputEvent::Enter);
}

#[test]
fn applying_only_this_palette_is_one_undoable_edit() {
    let mut editor = editor();
    let mut doc = document();
    let master = share_local_5(&mut doc);
    let before = doc.clone();
    let old = doc.video().palette.get(master);
    // No default master holds this color, so a spare one takes it.
    let new = Rgb::from_hex(0x010203);
    type_color_5(&mut editor, &mut doc, "010203");
    assert_eq!(doc, before, "nothing applied yet");
    assert_eq!(
        editor.color_editor().unwrap().scope(),
        Some(ColorScope::Palette)
    );
    command(&mut editor, &mut doc, InputEvent::Enter);
    assert!(!editor.has_dialog());

    let palettes = doc.obj().palettes();
    let moved = palettes.resolve(ObjBank::ZERO, 5).unwrap();
    assert_eq!(doc.video().palette.get(moved), new);
    assert_eq!(palettes.resolve(ObjBank::new(1).unwrap(), 5), Some(master));
    assert_eq!(doc.video().palette.get(master), old, "OBP1 keeps its color");

    assert_eq!(command(&mut editor, &mut doc, InputEvent::Undo), None);
    assert_eq!(doc.video(), before.video(), "one undo restores everything");
    assert_eq!(command(&mut editor, &mut doc, InputEvent::Redo), None);
    assert_eq!(doc.obj().palettes().resolve(ObjBank::ZERO, 5), Some(moved));
}

#[test]
fn applying_to_all_changes_the_shared_master_as_one_edit() {
    let mut editor = editor();
    let mut doc = document();
    let master = share_local_5(&mut doc);
    let before = doc.clone();
    let red = Rgb::from_hex(0xe6482e);
    type_color_5(&mut editor, &mut doc, "E6482E");
    let [all, _] = editor.color_editor_layout().scope_rows();
    tap(&mut editor, &mut doc, centre_of(all));
    command(&mut editor, &mut doc, InputEvent::Enter);
    assert!(!editor.has_dialog());
    assert_eq!(doc.video().palette.get(master), red);
    assert_eq!(doc.obj().palettes(), before.obj().palettes());

    assert_eq!(command(&mut editor, &mut doc, InputEvent::Undo), None);
    assert_eq!(doc.video(), before.video());
}

#[test]
fn edit_color_takes_every_event() {
    let mut editor = editor();
    let mut doc = document();
    command(&mut editor, &mut doc, InputEvent::Text('c'));
    let before = doc.clone();
    let canvas = canvas_point(&editor, 3, 3);
    for event in [
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            at: canvas,
        },
        InputEvent::PointerUp {
            button: PointerButton::Primary,
            at: canvas,
        },
        InputEvent::Text('e'),
        InputEvent::Text('a'),
        InputEvent::Undo,
        InputEvent::Save,
    ] {
        assert_eq!(command(&mut editor, &mut doc, event), None);
    }
    assert_eq!(doc, before);
    assert!(editor.color_editor().is_some());
    assert!(!editor.is_advanced());
    assert_eq!(editor.tool, ToolSlot::Pencil);
}
