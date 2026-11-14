use super::*;
use crate::cartridge::map::{MapFiles, MapId, MapMeta};
use glaux_machine::font::{BitmapFont, FontMetrics};
use glaux_machine::tilemap::BgBank;
use pretty_assertions::assert_eq;

fn editor() -> MapEditor {
    let layout = super::super::super::tab::Layout::new(FontMetrics::SYSTEM);
    MapEditor::new(layout.map_metrics(), BitmapFont::new(FontMetrics::SYSTEM))
}

fn document(width: usize, height: usize) -> MapDocument {
    let meta = MapMeta::new("Town", width, height).unwrap().to_bytes();
    let files = MapFiles {
        meta,
        layers: [None, None, None],
    };
    MapDocument::from_files(MapId::parse("town").unwrap(), &files).unwrap()
}

fn entry(tile: u16) -> TileEntry {
    TileEntry::new(tile, false, false, false, BgBank::ZERO).unwrap()
}

fn full(tile: u16, flip_x: bool, flip_y: bool, priority: bool, bank: u8) -> TileEntry {
    TileEntry::new(tile, flip_x, flip_y, priority, BgBank::new(bank).unwrap()).unwrap()
}

fn at(x: i32, y: i32) -> Point {
    Point::new(x as u16, y as u16)
}

fn middle(rect: crate::system::workspace::Rect) -> Point {
    at(rect.x + rect.width / 2, rect.y + rect.height / 2)
}

fn cell_point(editor: &MapEditor, document: &MapDocument, cell: (i32, i32)) -> Point {
    let frame = editor.frame();
    let view = editor.viewport(&frame, document);
    middle(view.cell_rect(frame.canvas, cell))
}

fn input(editor: &mut MapEditor, document: &mut MapDocument, event: InputEvent) -> Option<MapOut> {
    editor.handle_input(event, Some(document))
}

fn press(editor: &mut MapEditor, document: &mut MapDocument, at: Point) -> Option<MapOut> {
    input(
        editor,
        document,
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            at,
            select: false,
        },
    )
}

fn press_secondary(editor: &mut MapEditor, document: &mut MapDocument, at: Point) {
    input(
        editor,
        document,
        InputEvent::PointerDown {
            button: PointerButton::Secondary,
            at,
            select: false,
        },
    );
}

fn moved(editor: &mut MapEditor, document: &mut MapDocument, at: Point) {
    input(editor, document, InputEvent::PointerMove(Some(at)));
}

fn release(editor: &mut MapEditor, document: &mut MapDocument, at: Point) {
    input(
        editor,
        document,
        InputEvent::PointerUp {
            button: PointerButton::Primary,
            at,
        },
    );
}

fn key(editor: &mut MapEditor, document: &mut MapDocument, key: char) -> Option<MapOut> {
    input(editor, document, InputEvent::Text(key))
}

fn click_cell(editor: &mut MapEditor, document: &mut MapDocument, cell: (i32, i32)) {
    let at = cell_point(editor, document, cell);
    press(editor, document, at);
    release(editor, document, at);
}

fn cell(document: &MapDocument, layer: BgLayer, x: usize, y: usize) -> TileEntry {
    document.layer(layer).get(x, y).unwrap()
}

fn filled(document: &MapDocument, layer: BgLayer) -> usize {
    let (width, height) = map_size(document);
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter(|&(x, y)| !cell(document, layer, x, y).is_empty())
        .count()
}

#[test]
fn the_pencil_paints_the_brush_on_the_layer_being_edited() {
    let mut editor = editor();
    let mut document = document(30, 20);
    editor.edit.brush = entry(7);
    click_cell(&mut editor, &mut document, (3, 2));
    assert_eq!(cell(&document, BgLayer::Bg1, 3, 2), entry(7));
    assert_eq!(filled(&document, BgLayer::Bg1), 1);
    assert_eq!(filled(&document, BgLayer::Bg0), 0);
    assert_eq!(filled(&document, BgLayer::Bg2), 0);
    assert!(document.is_modified());
}

#[test]
fn a_fast_drag_leaves_no_gaps_and_undoes_as_one_edit() {
    let mut editor = editor();
    let mut document = document(30, 20);
    editor.edit.brush = entry(4);
    let from = cell_point(&editor, &document, (0, 0));
    let to = cell_point(&editor, &document, (9, 4));
    press(&mut editor, &mut document, from);
    moved(&mut editor, &mut document, to);
    release(&mut editor, &mut document, to);
    let painted = filled(&document, BgLayer::Bg1);
    assert_eq!(painted, 10, "one cell for each of the ten columns");
    assert_eq!(cell(&document, BgLayer::Bg1, 0, 0), entry(4));
    assert_eq!(cell(&document, BgLayer::Bg1, 9, 4), entry(4));

    assert!(document.undo());
    assert_eq!(filled(&document, BgLayer::Bg1), 0);
    assert!(!document.can_undo(), "the whole drag was one step");
    assert!(document.redo());
    assert_eq!(filled(&document, BgLayer::Bg1), painted);
}

#[test]
fn the_eraser_empties_cells_and_a_drag_of_it_is_one_edit() {
    let mut editor = editor();
    let mut document = document(30, 20);
    for x in 0..6 {
        document.set_cell(BgLayer::Bg1, x, 0, entry(2));
    }
    key(&mut editor, &mut document, 'e');
    let from = cell_point(&editor, &document, (1, 0));
    let to = cell_point(&editor, &document, (4, 0));
    press(&mut editor, &mut document, from);
    moved(&mut editor, &mut document, to);
    release(&mut editor, &mut document, to);
    let row: Vec<bool> = (0..6)
        .map(|x| cell(&document, BgLayer::Bg1, x, 0).is_empty())
        .collect();
    assert_eq!(row, [false, true, true, true, true, false]);
    assert!(document.undo());
    assert_eq!(filled(&document, BgLayer::Bg1), 6);
}

#[test]
fn painting_the_same_entry_again_changes_nothing() {
    let mut editor = editor();
    let mut document = document(10, 10);
    editor.edit.brush = entry(3);
    click_cell(&mut editor, &mut document, (2, 2));
    click_cell(&mut editor, &mut document, (2, 2));
    assert!(document.undo(), "one step for the first press");
    assert_eq!(filled(&document, BgLayer::Bg1), 0);
    assert!(!document.can_undo(), "the second press left no step");
}

#[test]
fn cells_off_the_map_are_ignored_and_never_wrap() {
    let mut editor = editor();
    let mut document = document(4, 3);
    editor.edit.brush = entry(5);
    // The canvas shows more than the map: press on cells past its edges.
    for cell in [(4, 0), (0, 3), (9, 9)] {
        click_cell(&mut editor, &mut document, cell);
    }
    assert!(!document.is_modified());
    // A drag that leaves the map paints only the cells inside it.
    let from = cell_point(&editor, &document, (2, 1));
    let to = cell_point(&editor, &document, (8, 1));
    press(&mut editor, &mut document, from);
    moved(&mut editor, &mut document, to);
    release(&mut editor, &mut document, to);
    assert_eq!(filled(&document, BgLayer::Bg1), 2, "cells 2 and 3 of row 1");
    assert!(cell(&document, BgLayer::Bg1, 0, 1).is_empty(), "no wrap");
}

#[test]
fn painting_a_scrolled_map_changes_the_cell_under_the_pointer() {
    let mut editor = editor();
    let mut document = document(256, 256);
    editor.edit.brush = entry(9);
    editor
        .edit
        .view
        .scroll_tiles(100, 80, (256, 256), (270, 120));
    let frame = editor.frame();
    let view = editor.viewport(&frame, &document);
    let (first, _) = view.visible_cells((256, 256), (frame.canvas.width, frame.canvas.height));
    assert!(first.0 >= 100 && first.1 >= 80, "{first:?}");
    let target = (first.0 + 3, first.1 + 2);
    click_cell(
        &mut editor,
        &mut document,
        (target.0 as i32, target.1 as i32),
    );
    assert_eq!(cell(&document, BgLayer::Bg1, target.0, target.1), entry(9));
    assert_eq!(filled(&document, BgLayer::Bg1), 1);
}

#[test]
fn the_layer_chosen_is_the_one_that_changes() {
    let mut editor = editor();
    let mut document = document(10, 10);
    editor.edit.brush = entry(2);
    for (layer, key_, cell_) in [
        (BgLayer::Bg0, '0', (0, 0)),
        (BgLayer::Bg1, '1', (1, 0)),
        (BgLayer::Bg2, '2', (2, 0)),
    ] {
        key(&mut editor, &mut document, key_);
        assert_eq!(editor.edit.layer, layer);
        click_cell(&mut editor, &mut document, cell_);
    }
    for (layer, x) in [(BgLayer::Bg0, 0), (BgLayer::Bg1, 1), (BgLayer::Bg2, 2)] {
        assert_eq!(filled(&document, layer), 1, "{layer}");
        assert!(!cell(&document, layer, x, 0).is_empty());
    }
}

#[test]
fn a_layer_row_or_chip_chooses_the_layer_to_edit() {
    let mut editor = editor();
    let mut document = document(10, 10);
    let frame = editor.frame();
    let rows = frame.layer_rows.unwrap();
    press(&mut editor, &mut document, middle(rows[0].name));
    assert_eq!(editor.edit.layer, BgLayer::Bg2, "BG2 is the top row");
    press(&mut editor, &mut document, middle(rows[2].name));
    assert_eq!(editor.edit.layer, BgLayer::Bg0);

    editor.edit.drawer = DrawerSize::Collapsed;
    let frame = editor.frame();
    press(&mut editor, &mut document, middle(frame.chips[1]));
    assert_eq!(editor.edit.layer, BgLayer::Bg1);
}

#[test]
fn a_locked_layer_takes_no_change_from_any_tool_and_says_so() {
    let mut editor = editor();
    let mut document = document(10, 10);
    document.set_cell(BgLayer::Bg1, 1, 1, entry(2));
    document.set_cell(BgLayer::Bg1, 2, 1, entry(2));
    editor.edit.brush = entry(8);
    let before = document.layer(BgLayer::Bg1).to_bytes();
    let frame = editor.frame();
    let lock = frame.layer_rows.unwrap()[1].lock;
    press(&mut editor, &mut document, middle(lock));
    assert!(editor.edit.locked[BgLayer::Bg1.index()]);

    for tool in ['p', 'e', 'f'] {
        key(&mut editor, &mut document, tool);
        let at = cell_point(&editor, &document, (1, 1));
        let out = press(&mut editor, &mut document, at);
        assert_eq!(
            out,
            Some(MapOut::Notice("BG1 is locked".to_owned())),
            "tool {tool}"
        );
        let far = cell_point(&editor, &document, (5, 5));
        moved(&mut editor, &mut document, far);
        release(&mut editor, &mut document, at);
        assert_eq!(
            document.layer(BgLayer::Bg1).to_bytes(),
            before,
            "tool {tool}"
        );
    }
    assert!(document.is_modified(), "the two cells set above");
    // The lock is the editor's: the map did not notice it.
    let unlocked = document.layer(BgLayer::Bg1).to_bytes();
    press(&mut editor, &mut document, middle(lock));
    key(&mut editor, &mut document, 'p');
    click_cell(&mut editor, &mut document, (5, 5));
    assert_ne!(document.layer(BgLayer::Bg1).to_bytes(), unlocked);
}

#[test]
fn locking_one_layer_leaves_the_others_open() {
    let mut editor = editor();
    let mut document = document(10, 10);
    editor.edit.brush = entry(8);
    editor.edit.locked[BgLayer::Bg1.index()] = true;
    key(&mut editor, &mut document, '2');
    click_cell(&mut editor, &mut document, (1, 1));
    assert_eq!(filled(&document, BgLayer::Bg2), 1);
}

#[test]
fn writes_go_through_one_check_that_refuses_a_locked_layer() {
    let mut editor = editor();
    let mut document = document(4, 4);
    editor.edit.locked[BgLayer::Bg1.index()] = true;
    assert!(!editor.write(&mut document, (1, 1), entry(3)));
    editor.edit.locked[BgLayer::Bg1.index()] = false;
    assert!(editor.write(&mut document, (1, 1), entry(3)));
    assert!(!editor.write(&mut document, (1, 1), entry(3)), "same entry");
    assert!(
        !editor.write(&mut document, (-1, 0), entry(3)),
        "off the map"
    );
    assert!(!editor.write(&mut document, (4, 0), entry(3)));
}

#[test]
fn hiding_a_layer_is_the_editors_alone_and_the_map_stays_clean() {
    let mut editor = editor();
    let mut document = document(10, 10);
    let frame = editor.frame();
    let rows = frame.layer_rows.unwrap();
    press(&mut editor, &mut document, middle(rows[0].eye));
    press(&mut editor, &mut document, middle(rows[1].lock));
    assert_eq!(editor.edit.visible, [true, true, false]);
    assert_eq!(editor.edit.locked, [false, true, false]);
    assert!(!document.is_modified());
    assert!(!document.can_undo());
    press(&mut editor, &mut document, middle(rows[0].eye));
    assert_eq!(editor.edit.visible, [true; 3]);
}

#[test]
fn the_fill_paints_the_connected_cells_with_the_same_entry_as_one_edit() {
    let mut editor = editor();
    let mut document = document(12, 8);
    // A wall of tile 2 splits row 0..8 at column 6.
    for y in 0..8 {
        document.set_cell(BgLayer::Bg1, 6, y, entry(2));
    }
    editor.edit.brush = entry(5);
    key(&mut editor, &mut document, 'f');
    let at = cell_point(&editor, &document, (2, 3));
    press(&mut editor, &mut document, at);
    assert_eq!(filled(&document, BgLayer::Bg1), 6 * 8 + 8);
    assert_eq!(cell(&document, BgLayer::Bg1, 0, 0), entry(5));
    assert_eq!(cell(&document, BgLayer::Bg1, 6, 4), entry(2), "the wall");
    assert!(cell(&document, BgLayer::Bg1, 7, 4).is_empty(), "behind it");
    assert!(document.undo());
    assert_eq!(filled(&document, BgLayer::Bg1), 8);
}

#[test]
fn the_fill_compares_whole_entries_not_just_tile_indices() {
    let mut editor = editor();
    let mut document = document(6, 1);
    let flipped = full(3, true, false, false, 0);
    document.set_cell(BgLayer::Bg1, 0, 0, entry(3));
    document.set_cell(BgLayer::Bg1, 1, 0, entry(3));
    document.set_cell(BgLayer::Bg1, 2, 0, flipped);
    document.set_cell(BgLayer::Bg1, 3, 0, entry(3));
    document.set_cell(BgLayer::Bg1, 4, 0, full(3, false, false, true, 0));
    document.set_cell(BgLayer::Bg1, 5, 0, full(3, false, false, false, 1));
    editor.edit.brush = entry(9);
    key(&mut editor, &mut document, 'f');
    let at = cell_point(&editor, &document, (0, 0));
    press(&mut editor, &mut document, at);
    let row: Vec<u16> = (0..6)
        .map(|x| cell(&document, BgLayer::Bg1, x, 0).tile())
        .collect();
    assert_eq!(row, [9, 9, 3, 3, 3, 3], "only the two plain 3s joined");
    assert_eq!(cell(&document, BgLayer::Bg1, 2, 0), flipped);
}

#[test]
fn filling_with_the_entry_already_there_records_nothing() {
    let mut editor = editor();
    let mut document = document(5, 5);
    editor.edit.brush = EMPTY;
    key(&mut editor, &mut document, 'f');
    click_cell(&mut editor, &mut document, (1, 1));
    assert!(!document.is_modified() && !document.can_undo());
}

const EMPTY: TileEntry = TileEntry::EMPTY;

#[test]
fn filling_a_whole_largest_map_is_one_step_within_the_history() {
    let mut editor = editor();
    let mut document = document(256, 256);
    editor.edit.brush = entry(6);
    key(&mut editor, &mut document, 'f');
    click_cell(&mut editor, &mut document, (2, 2));
    assert_eq!(filled(&document, BgLayer::Bg1), 65_536);
    assert!(document.undo(), "the newest edit is always kept");
    assert_eq!(filled(&document, BgLayer::Bg1), 0);
    assert!(document.redo());
    assert_eq!(filled(&document, BgLayer::Bg1), 65_536);
}

#[test]
fn the_eyedropper_takes_the_whole_entry_and_reads_a_locked_layer() {
    let mut editor = editor();
    let mut document = document(10, 10);
    let picked = full(77, true, true, true, 2);
    document.set_cell(BgLayer::Bg1, 4, 4, picked);
    editor.edit.locked[BgLayer::Bg1.index()] = true;
    key(&mut editor, &mut document, 'i');
    click_cell(&mut editor, &mut document, (4, 4));
    assert_eq!(editor.edit.brush, picked);
    assert!(editor.edit.brush.flip_x() && editor.edit.brush.flip_y());
    assert!(editor.edit.brush.priority());
    assert_eq!(editor.edit.brush.bank(), BgBank::new(2).unwrap());
    click_cell(&mut editor, &mut document, (50, 50));
    assert_eq!(editor.edit.brush, picked);
    click_cell(&mut editor, &mut document, (0, 0));
    assert!(editor.edit.brush.is_empty());
}

#[test]
fn the_eyedropper_takes_from_the_layer_being_edited() {
    let mut editor = editor();
    let mut document = document(10, 10);
    document.set_cell(BgLayer::Bg0, 1, 1, entry(11));
    document.set_cell(BgLayer::Bg2, 1, 1, entry(22));
    key(&mut editor, &mut document, 'i');
    key(&mut editor, &mut document, '0');
    click_cell(&mut editor, &mut document, (1, 1));
    assert_eq!(editor.edit.brush.tile(), 11);
    key(&mut editor, &mut document, '2');
    click_cell(&mut editor, &mut document, (1, 1));
    assert_eq!(editor.edit.brush.tile(), 22);
}

#[test]
fn the_hand_scrolls_the_view_by_dragging_and_never_edits() {
    let mut editor = editor();
    let mut document = document(256, 256);
    key(&mut editor, &mut document, 'h');
    let start = at(150, 100);
    press(&mut editor, &mut document, start);
    moved(&mut editor, &mut document, at(130, 70));
    assert_eq!(editor.edit.view.offset(), (20, 30));
    moved(&mut editor, &mut document, at(100, 50));
    assert_eq!(editor.edit.view.offset(), (50, 50));
    release(&mut editor, &mut document, at(100, 50));
    assert!(!document.is_modified());
    // Dragging past the corner stops there.
    press(&mut editor, &mut document, start);
    moved(&mut editor, &mut document, at(300, 300));
    assert_eq!(editor.edit.view.offset(), (0, 0));
    release(&mut editor, &mut document, at(300, 300));
}

#[test]
fn a_drag_that_ends_off_the_canvas_still_ends() {
    let mut editor = editor();
    let mut document = document(30, 20);
    let from = cell_point(&editor, &document, (1, 1));
    press(&mut editor, &mut document, from);
    assert!(editor.edit.stroke.is_some());
    release(&mut editor, &mut document, at(0, 239));
    assert!(editor.edit.stroke.is_none());
    assert!(document.undo(), "its edit was closed and is one step");
}

#[test]
fn a_press_while_a_drag_is_open_closes_it_first() {
    let mut editor = editor();
    let mut document = document(30, 20);
    editor.edit.brush = entry(3);
    let first = cell_point(&editor, &document, (1, 1));
    let second = cell_point(&editor, &document, (5, 5));
    press(&mut editor, &mut document, first);
    press(&mut editor, &mut document, second);
    release(&mut editor, &mut document, second);
    assert!(document.undo());
    assert_eq!(filled(&document, BgLayer::Bg1), 1, "two separate steps");
    assert!(document.undo());
    assert_eq!(filled(&document, BgLayer::Bg1), 0);
}

#[test]
fn undo_during_a_drag_ends_it_and_undoes_the_edit() {
    let mut editor = editor();
    let mut document = document(30, 20);
    let from = cell_point(&editor, &document, (1, 1));
    press(&mut editor, &mut document, from);
    let next = cell_point(&editor, &document, (4, 1));
    moved(&mut editor, &mut document, next);
    input(&mut editor, &mut document, InputEvent::Undo);
    assert!(editor.edit.stroke.is_none());
    assert_eq!(filled(&document, BgLayer::Bg1), 0);
}

#[test]
fn a_cancelled_drag_stops_acting() {
    let mut editor = editor();
    let mut document = document(30, 20);
    let from = cell_point(&editor, &document, (1, 1));
    press(&mut editor, &mut document, from);
    input(&mut editor, &mut document, InputEvent::PointerCancel);
    let later = cell_point(&editor, &document, (6, 1));
    moved(&mut editor, &mut document, later);
    assert_eq!(filled(&document, BgLayer::Bg1), 1);
}

#[test]
fn the_select_tool_marks_the_rectangle_between_its_corners() {
    let mut editor = editor();
    let mut document = document(30, 20);
    key(&mut editor, &mut document, 's');
    let from = cell_point(&editor, &document, (6, 4));
    let to = cell_point(&editor, &document, (3, 2));
    press(&mut editor, &mut document, from);
    assert_eq!(
        editor.edit.selection.map(|s| (s.width(), s.height())),
        Some((1, 1))
    );
    moved(&mut editor, &mut document, to);
    release(&mut editor, &mut document, to);
    let selection = editor.edit.selection.unwrap();
    assert_eq!(
        (
            selection.left,
            selection.top,
            selection.right,
            selection.bottom
        ),
        (3, 2, 6, 4)
    );
    assert!(!document.is_modified(), "selecting edits nothing");
    assert!(editor.claims_break());
}

#[test]
fn escape_clears_the_selection_before_leaving_the_view() {
    let mut editor = editor();
    let mut document = document(30, 20);
    key(&mut editor, &mut document, 's');
    click_cell(&mut editor, &mut document, (3, 3));
    assert!(editor.claims_break());
    assert_eq!(input(&mut editor, &mut document, InputEvent::Break), None);
    assert!(editor.edit.selection.is_none());
    assert!(!editor.claims_break(), "the next Escape leaves");
}

#[test]
fn a_selection_is_cut_at_the_map_edge() {
    let mut editor = editor();
    let mut document = document(5, 4);
    key(&mut editor, &mut document, 's');
    let from = cell_point(&editor, &document, (3, 2));
    let to = cell_point(&editor, &document, (12, 9));
    press(&mut editor, &mut document, from);
    moved(&mut editor, &mut document, to);
    release(&mut editor, &mut document, to);
    let selection = editor.edit.selection.unwrap();
    assert_eq!((selection.right, selection.bottom), (4, 3));
}

#[test]
fn the_keys_pick_the_tools_and_toggle_the_grid_and_rulers() {
    let mut editor = editor();
    let mut document = document(10, 10);
    for tool in Tool::ALL {
        key(&mut editor, &mut document, tool.key());
        assert_eq!(editor.edit.tool, tool);
    }
    key(&mut editor, &mut document, 'P');
    assert_eq!(editor.edit.tool, Tool::Pencil);
    assert!(editor.edit.grid);
    key(&mut editor, &mut document, 'g');
    assert!(!editor.edit.grid);
    assert!(!editor.edit.rulers);
    key(&mut editor, &mut document, 'x');
    assert!(editor.edit.rulers);
    assert_eq!(key(&mut editor, &mut document, 'q'), None);
}

#[test]
fn the_tool_plates_and_option_buttons_answer_clicks() {
    let mut editor = editor();
    let mut document = document(10, 10);
    let frame = editor.frame();
    for (tool, rect) in frame.tools {
        press(&mut editor, &mut document, middle(rect));
        assert_eq!(editor.edit.tool, tool);
    }
    press(&mut editor, &mut document, middle(frame.grid_button));
    assert!(!editor.edit.grid);
    press(&mut editor, &mut document, middle(frame.rulers_button));
    assert!(editor.edit.rulers);
    let frame = editor.frame();
    press(&mut editor, &mut document, middle(frame.zoom_in));
    assert_eq!(editor.zoom_level(), super::super::view::Zoom::X4);
}

#[test]
fn zoom_steps_in_and_out_and_stops_at_the_ends() {
    let mut editor = editor();
    let mut document = document(64, 64);
    key(&mut editor, &mut document, '+');
    key(&mut editor, &mut document, '+');
    assert_eq!(editor.zoom_level(), super::super::view::Zoom::X4);
    for _ in 0..4 {
        key(&mut editor, &mut document, '-');
    }
    assert_eq!(editor.zoom_level(), super::super::view::Zoom::X1);
    key(&mut editor, &mut document, '=');
    assert_eq!(editor.zoom_level(), super::super::view::Zoom::X2);
}

#[test]
fn painting_keeps_working_after_a_zoom() {
    let mut editor = editor();
    let mut document = document(64, 64);
    editor.edit.brush = entry(3);
    for zoom_key in ['+', '-', '-'] {
        key(&mut editor, &mut document, zoom_key);
        // Zooming keeps the middle of the canvas in place, so pick a cell
        // that shows now.
        let frame = editor.frame();
        let view = editor.viewport(&frame, &document);
        let (first, _) = view.visible_cells((64, 64), (frame.canvas.width, frame.canvas.height));
        let target = (first.0 + 1, first.1 + 1);
        click_cell(
            &mut editor,
            &mut document,
            (target.0 as i32, target.1 as i32),
        );
        assert_eq!(cell(&document, BgLayer::Bg1, target.0, target.1), entry(3));
        document.set_cell(BgLayer::Bg1, target.0, target.1, EMPTY);
    }
}

#[test]
fn the_wheel_scrolls_the_canvas_and_stops_at_the_map() {
    let mut editor = editor();
    let mut document = document(64, 64);
    input(&mut editor, &mut document, InputEvent::Wheel { delta: -3 });
    assert_eq!(
        editor.edit.view.offset(),
        (0, 3 * 2 * 16),
        "toward the bottom"
    );
    input(&mut editor, &mut document, InputEvent::Wheel { delta: 10 });
    assert_eq!(editor.edit.view.offset(), (0, 0));
    input(
        &mut editor,
        &mut document,
        InputEvent::WheelHorizontal { delta: -2 },
    );
    assert_eq!(editor.edit.view.offset(), (2 * 2 * 16, 0));
    input(
        &mut editor,
        &mut document,
        InputEvent::WheelHorizontal { delta: 100 },
    );
    assert_eq!(editor.edit.view.offset(), (0, 0));
}

#[test]
fn the_wheel_scrolls_the_strip_while_the_pointer_is_over_it() {
    let mut editor = editor();
    let mut document = document(64, 64);
    let frame = editor.frame();
    moved(&mut editor, &mut document, middle(frame.strip.unwrap()));
    input(&mut editor, &mut document, InputEvent::Wheel { delta: -3 });
    assert_eq!(editor.edit.strip_top, 6);
    assert_eq!(editor.edit.view.offset(), (0, 0), "the canvas stayed");
    input(
        &mut editor,
        &mut document,
        InputEvent::Wheel { delta: i8::MAX },
    );
    assert_eq!(editor.edit.strip_top, 0);
    for _ in 0..40 {
        input(&mut editor, &mut document, InputEvent::Wheel { delta: -10 });
    }
    assert_eq!(editor.edit.strip_top, 64 - frame.strip_rows);
}

#[test]
fn the_arrows_and_pages_scroll_the_canvas_by_tiles_and_pages() {
    let mut editor = editor();
    let mut document = document(64, 64);
    input(
        &mut editor,
        &mut document,
        InputEvent::Right { select: false },
    );
    input(
        &mut editor,
        &mut document,
        InputEvent::Down { select: false },
    );
    input(
        &mut editor,
        &mut document,
        InputEvent::Down { select: false },
    );
    assert_eq!(editor.edit.view.offset(), (16, 32));
    input(
        &mut editor,
        &mut document,
        InputEvent::Left { select: false },
    );
    input(&mut editor, &mut document, InputEvent::Up { select: false });
    assert_eq!(editor.edit.view.offset(), (0, 16));
    let height = editor.frame().canvas.height;
    input(
        &mut editor,
        &mut document,
        InputEvent::PageDown { select: false },
    );
    assert_eq!(editor.edit.view.offset().1, 16 + height - 16);
    input(
        &mut editor,
        &mut document,
        InputEvent::PageUp { select: false },
    );
    input(
        &mut editor,
        &mut document,
        InputEvent::PageUp { select: false },
    );
    assert_eq!(editor.edit.view.offset().1, 0);
}

#[test]
fn the_down_arrow_scrolls_while_a_map_is_open_and_enter_opens_the_list() {
    let mut editor = editor();
    let mut document = document(64, 64);
    editor.listed(Ok(vec![crate::cartridge::map::MapSummary {
        id: MapId::parse("town").unwrap(),
        meta: MapMeta::new("Town", 64, 64),
    }]));
    input(
        &mut editor,
        &mut document,
        InputEvent::Down { select: false },
    );
    assert!(!editor.list_is_open());
    input(&mut editor, &mut document, InputEvent::Enter);
    assert!(editor.list_is_open());
}

#[test]
fn tab_cycles_the_drawer_and_the_view_stays_inside_the_map() {
    let mut editor = editor();
    let mut document = document(64, 64);
    editor
        .edit
        .view
        .scroll_by(100_000, 100_000, (64, 64), (270, 120));
    let mut heights = Vec::new();
    for _ in 0..3 {
        input(
            &mut editor,
            &mut document,
            InputEvent::Tab { reverse: false },
        );
        let frame = editor.frame();
        let (width, height) = editor.edit.view.content((64, 64));
        let (x, y) = editor.edit.view.offset();
        assert!(x + frame.canvas.width <= width && y + frame.canvas.height <= height);
        heights.push((editor.edit.drawer, frame.canvas.height));
    }
    assert_eq!(heights[0].0, DrawerSize::Expanded);
    assert_eq!(heights[1].0, DrawerSize::Collapsed);
    assert_eq!(heights[2].0, DrawerSize::Compact);
    assert!(heights[1].1 > heights[2].1 && heights[2].1 > heights[0].1);
    input(
        &mut editor,
        &mut document,
        InputEvent::Tab { reverse: true },
    );
    assert_eq!(editor.edit.drawer, DrawerSize::Collapsed);
}

#[test]
fn the_drawer_button_cycles_its_size() {
    let mut editor = editor();
    let mut document = document(10, 10);
    let button = editor.frame().size_button;
    press(&mut editor, &mut document, middle(button));
    assert_eq!(editor.edit.drawer, DrawerSize::Expanded);
}

#[test]
fn a_strip_tile_becomes_the_brush_tile_and_keeps_the_brush_attributes() {
    let mut editor = editor();
    let mut document = document(10, 10);
    editor.edit.brush = full(5, true, false, true, 2);
    let strip = editor.frame().strip.unwrap();
    press(
        &mut editor,
        &mut document,
        at(strip.x + 2 * 8 + 3, strip.y + 8 + 3),
    );
    assert_eq!(editor.edit.brush, full(18, true, false, true, 2));
    editor.edit.strip_top = 10;
    press(&mut editor, &mut document, at(strip.x + 3, strip.y + 3));
    assert_eq!(editor.edit.brush.tile(), 160);
}

#[test]
fn tile_zero_is_not_offered_as_a_brush() {
    let mut editor = editor();
    let mut document = document(10, 10);
    editor.edit.brush = entry(5);
    let strip = editor.frame().strip.unwrap();
    let out = press(&mut editor, &mut document, at(strip.x + 3, strip.y + 3));
    assert_eq!(
        out,
        Some(MapOut::Notice("Tile 0 is always transparent".to_owned()))
    );
    assert_eq!(editor.edit.brush, entry(5));
}

#[test]
fn clicking_the_strip_changes_no_map_and_no_layer() {
    let mut editor = editor();
    let mut document = document(10, 10);
    let strip = editor.frame().strip.unwrap();
    press(&mut editor, &mut document, at(strip.x + 20, strip.y + 20));
    assert!(!document.is_modified());
    assert_eq!(editor.edit.layer, BgLayer::Bg1);
}

#[test]
fn f2_renames_the_layer_being_edited_and_an_empty_name_removes_it() {
    let mut editor = editor();
    let mut document = document(10, 10);
    key(&mut editor, &mut document, '2');
    input(&mut editor, &mut document, InputEvent::Function(2));
    assert!(editor.has_overlay());
    for c in "Sky".chars() {
        input(&mut editor, &mut document, InputEvent::Text(c));
    }
    input(&mut editor, &mut document, InputEvent::Enter);
    assert!(!editor.has_overlay());
    assert_eq!(
        document.meta().layer_names[BgLayer::Bg2.index()].as_deref(),
        Some("Sky")
    );
    assert!(document.undo(), "the name is part of the history");
    assert_eq!(document.meta().layer_names[BgLayer::Bg2.index()], None);
    assert!(document.redo());

    input(&mut editor, &mut document, InputEvent::Function(2));
    for _ in 0..3 {
        input(&mut editor, &mut document, InputEvent::Backspace);
    }
    input(&mut editor, &mut document, InputEvent::Enter);
    assert_eq!(document.meta().layer_names[BgLayer::Bg2.index()], None);
}

#[test]
fn escape_closes_the_layer_rename_without_a_change() {
    let mut editor = editor();
    let mut document = document(10, 10);
    input(&mut editor, &mut document, InputEvent::Function(2));
    input(&mut editor, &mut document, InputEvent::Text('A'));
    input(&mut editor, &mut document, InputEvent::Break);
    assert!(!editor.has_overlay());
    assert!(!document.is_modified());
}

#[test]
fn a_secondary_press_on_a_layer_opens_its_menu_and_choosing_runs_the_action() {
    let mut editor = editor();
    let mut document = document(10, 10);
    let rows = editor.frame().layer_rows.unwrap();
    press_secondary(&mut editor, &mut document, middle(rows[0].name));
    assert!(editor.has_overlay());
    assert!(editor.claims_break());
    // Edit, Rename, a separator, Hide, Lock: the third item hides.
    for _ in 0..3 {
        input(
            &mut editor,
            &mut document,
            InputEvent::Down { select: false },
        );
    }
    input(&mut editor, &mut document, InputEvent::Enter);
    assert!(!editor.has_overlay());
    assert_eq!(editor.edit.visible, [true, true, false], "BG2 hidden");
    assert!(!document.is_modified());
}

#[test]
fn the_layer_menu_locks_edits_and_renames_the_layer_it_was_opened_on() {
    let mut editor = editor();
    let mut document = document(10, 10);
    let rows = editor.frame().layer_rows.unwrap();

    press_secondary(&mut editor, &mut document, middle(rows[2].lock));
    for _ in 0..4 {
        input(
            &mut editor,
            &mut document,
            InputEvent::Down { select: false },
        );
    }
    input(&mut editor, &mut document, InputEvent::Enter);
    assert_eq!(editor.edit.locked, [true, false, false]);
    assert_eq!(editor.edit.layer, BgLayer::Bg1, "locking does not edit");

    press_secondary(&mut editor, &mut document, middle(rows[2].eye));
    input(
        &mut editor,
        &mut document,
        InputEvent::Down { select: false },
    );
    input(&mut editor, &mut document, InputEvent::Enter);
    assert_eq!(editor.edit.layer, BgLayer::Bg0);

    press_secondary(&mut editor, &mut document, middle(rows[0].name));
    for _ in 0..2 {
        input(
            &mut editor,
            &mut document,
            InputEvent::Down { select: false },
        );
    }
    input(&mut editor, &mut document, InputEvent::Enter);
    assert!(editor.has_overlay(), "the rename window");
    assert_eq!(editor.edit.layer, BgLayer::Bg2);
}

#[test]
fn escape_closes_the_layer_menu_and_a_press_outside_closes_it_too() {
    let mut editor = editor();
    let mut document = document(10, 10);
    let rows = editor.frame().layer_rows.unwrap();
    press_secondary(&mut editor, &mut document, middle(rows[1].name));
    input(&mut editor, &mut document, InputEvent::Break);
    assert!(!editor.has_overlay());
    press_secondary(&mut editor, &mut document, middle(rows[1].name));
    press(&mut editor, &mut document, at(2, 232));
    assert!(!editor.has_overlay());
    assert_eq!(editor.edit.visible, [true; 3]);
}

#[test]
fn a_secondary_press_elsewhere_opens_nothing() {
    let mut editor = editor();
    let mut document = document(10, 10);
    let canvas = editor.frame().canvas;
    press_secondary(&mut editor, &mut document, middle(canvas));
    assert!(!editor.has_overlay());
}

#[test]
fn opening_another_map_resets_the_view_but_keeps_the_tool_and_layer_states() {
    let mut editor = editor();
    let mut document = document(64, 64);
    editor.edit.view.scroll_by(300, 300, (64, 64), (270, 120));
    editor.edit.tool = Tool::Fill;
    editor.edit.locked[BgLayer::Bg0.index()] = true;
    editor.edit.selection = Selection::between((1, 1), (2, 2), (64, 64));
    key(&mut editor, &mut document, '+');
    let town = MapId::parse("town").unwrap();
    editor.opened(&town);
    assert_eq!(editor.edit.view.offset(), (0, 0));
    assert!(editor.edit.selection.is_none());
    assert_eq!(editor.edit.tool, Tool::Fill);
    assert!(editor.edit.locked[0]);

    editor.edit.view.scroll_by(100, 100, (64, 64), (270, 120));
    editor.opened(&town);
    assert_ne!(editor.edit.view.offset(), (0, 0));
    editor.opened(&MapId::parse("cave").unwrap());
    assert_eq!(editor.edit.view.offset(), (0, 0));
}

#[test]
fn forgetting_the_cartridge_resets_the_editor() {
    let mut editor = editor();
    let mut document = document(64, 64);
    key(&mut editor, &mut document, 'f');
    editor.edit.locked[1] = true;
    editor.forget();
    assert_eq!(editor.edit, Edit::default());
}

#[test]
fn leaving_the_view_drops_the_drag_and_leaves_the_edit_to_close_later() {
    let mut editor = editor();
    let mut document = document(30, 20);
    let from = cell_point(&editor, &document, (1, 1));
    press(&mut editor, &mut document, from);
    editor.close_overlays();
    assert!(editor.edit.stroke.is_none());
    assert!(document.undo(), "undo closes the open edit and reverses it");
    assert_eq!(filled(&document, BgLayer::Bg1), 0);
}

#[test]
fn nothing_reaches_the_editor_without_a_map() {
    let mut editor = editor();
    assert_eq!(editor.handle_input(InputEvent::Text('f'), None), None);
    assert_eq!(
        editor.handle_input(InputEvent::Tab { reverse: false }, None),
        None
    );
    assert_eq!(editor.edit, Edit::default());
}

#[test]
fn saving_and_undoing_still_reach_the_document() {
    let mut editor = editor();
    let mut document = document(10, 10);
    click_cell(&mut editor, &mut document, (1, 1));
    assert_eq!(
        input(&mut editor, &mut document, InputEvent::Save),
        Some(MapOut::Request(super::super::MapRequest::Save))
    );
    input(&mut editor, &mut document, InputEvent::Undo);
    assert_eq!(filled(&document, BgLayer::Bg1), 0);
    input(&mut editor, &mut document, InputEvent::Redo);
    assert_eq!(filled(&document, BgLayer::Bg1), 1);
}

#[test]
fn the_brush_text_names_the_tile_bank_and_flags() {
    use super::super::drawer::brush_text;
    assert_eq!(brush_text(entry(36)), "0036  BGP0");
    assert_eq!(brush_text(full(36, true, false, false, 1)), "0036  BGP1 X");
    assert_eq!(brush_text(full(7, true, true, true, 3)), "0007  BGP3 XYP");
    assert_eq!(
        brush_text(full(1023, false, false, true, 0)),
        "1023  BGP0 P"
    );
    assert_eq!(brush_text(EMPTY), "0000  BGP0");
}
