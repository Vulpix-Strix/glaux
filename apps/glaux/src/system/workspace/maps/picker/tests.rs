use pretty_assertions::assert_eq;

use super::super::fixture::{at, document, editor, entry, input, key, middle, press, release};
use super::*;
use crate::system::workspace::Rect;
use glaux_input::InputEvent;
use glaux_machine::tilemap::{BgBank, BgLayer};
use super::super::Overlay;

fn open(editor: &mut MapEditor, document: &mut crate::system::map_document::MapDocument) -> PickerLayout {
    key(editor, document, 't');
    PickerLayout::new(&editor.frame())
}

fn tile_point(layout: &PickerLayout, tile: usize, page: usize) -> glaux_input::Point {
    middle(layout.tile_rect(tile, page).unwrap())
}

#[test]
fn a_rectangle_between_two_tiles_starts_at_its_top_left_whichever_way_it_is_dragged() {
    assert_eq!(between(20, 22), (20, (3, 1)));
    assert_eq!(between(22, 20), (20, (3, 1)));
    assert_eq!(between(52, 17), (17, (4, 3)));
    assert_eq!(between(33, 33), (33, (1, 1)));
}

#[test]
fn the_window_fits_the_screen_above_the_drawers_bar_and_the_grid_holds_nine_rows() {
    let editor = editor();
    let mut editor = editor;
    let mut doc = document(8, 8);
    let layout = open(&mut editor, &mut doc);
    let frame = editor.frame();
    let screen = Rect { x: 0, y: 0, width: 320, height: 240 };
    assert!(layout.window.right() <= screen.right());
    assert!(layout.window.y >= frame.options.bottom());
    assert!(layout.window.bottom() <= frame.drawer_bar.y, "the Picker button stays visible");
    assert_eq!((layout.grid.width, layout.grid.height), (256, 144));
    for tile in [0, 15, 16, 143] {
        let rect = layout.tile_rect(tile, 0).unwrap();
        assert_eq!(layout.tile_at(middle(rect), 0), Some(tile));
    }
    assert_eq!(layout.tile_rect(144, 0), None, "the tenth row is on the next page");
    assert_eq!(layout.tile_at(middle(layout.tile_rect(144, 1).unwrap()), 1), Some(144));
}

#[test]
fn opening_collapses_the_drawer_and_closing_brings_it_back() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    editor.edit.drawer = DrawerSize::Expanded;
    open(&mut editor, &mut doc);
    assert_eq!(editor.edit.drawer, DrawerSize::Collapsed);
    assert!(editor.claims_break());
    input(&mut editor, &mut doc, InputEvent::Break);
    assert!(editor.edit.picker.is_none());
    assert_eq!(editor.edit.drawer, DrawerSize::Expanded);

    open(&mut editor, &mut doc);
    key(&mut editor, &mut doc, 't');
    assert_eq!(editor.edit.drawer, DrawerSize::Expanded);
}

#[test]
fn dragging_chooses_a_pattern_and_keeps_the_brushs_attributes() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    editor.edit.brush = TileEntry::new(1, true, false, true, BgBank::new(2).unwrap()).unwrap();
    let layout = open(&mut editor, &mut doc);
    press(&mut editor, &mut doc, tile_point(&layout, 20, 0));
    let end = tile_point(&layout, 38, 0);
    input(&mut editor, &mut doc, InputEvent::PointerMove(Some(end)));
    release(&mut editor, &mut doc, end);

    assert_eq!(editor.edit.pattern, (3, 2));
    let brush = editor.edit.brush;
    assert_eq!((brush.tile(), brush.flip_x(), brush.priority(), brush.bank().get()), (20, true, true, 2));
    assert!(editor.edit.picker.is_some(), "the picker stays open");
}

#[test]
fn tile_zero_alone_is_refused_and_a_pattern_may_include_it() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    let layout = open(&mut editor, &mut doc);
    let zero = tile_point(&layout, 0, 0);
    press(&mut editor, &mut doc, zero);
    let out = release(&mut editor, &mut doc, zero);
    assert!(matches!(out, Some(MapOut::Notice(_))));
    assert_eq!(editor.edit.brush.tile(), 1);

    press(&mut editor, &mut doc, zero);
    let end = tile_point(&layout, 1, 0);
    release(&mut editor, &mut doc, end);
    assert_eq!((editor.edit.brush.tile(), editor.edit.pattern), (0, (2, 1)));
}

#[test]
fn the_wheel_turns_pages_within_the_sheet() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    open(&mut editor, &mut doc);
    input(&mut editor, &mut doc, InputEvent::Wheel { delta: 1 });
    assert_eq!(editor.edit.picker.unwrap().page, 0);
    for _ in 0..20 {
        input(&mut editor, &mut doc, InputEvent::Wheel { delta: -1 });
    }
    assert_eq!(editor.edit.picker.unwrap().page, PAGES - 1);
    assert_eq!(PAGES, 8);
}

#[test]
fn a_press_in_the_window_does_not_paint_the_map_behind_it() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    let layout = open(&mut editor, &mut doc);
    press(&mut editor, &mut doc, tile_point(&layout, 5, 0));
    assert!(doc.layer(BgLayer::Bg1).is_blank());
}

#[test]
fn a_secondary_press_opens_the_tile_menu_and_f2_edits_the_tile_under_the_pointer() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    let layout = open(&mut editor, &mut doc);
    let point = tile_point(&layout, 70, 0);
    input(&mut editor, &mut doc, InputEvent::PointerMove(Some(point)));
    assert_eq!(
        input(&mut editor, &mut doc, InputEvent::Function(2)),
        Some(MapOut::EditTile { tile: 70, bank: BgBank::ZERO })
    );
    super::super::fixture::pointer(&mut editor, &mut doc, glaux_input::PointerButton::Secondary, point);
    assert!(matches!(editor.overlay, Some(Overlay::TileMenu(70, _))));
}

#[test]
fn the_status_bar_names_the_pattern_and_its_tile_range() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    editor.edit.brush = entry(20);
    editor.edit.pattern = (3, 1);
    open(&mut editor, &mut doc);
    let picker = editor.edit.picker.unwrap();
    assert_eq!(editor.picker_status(&picker), "Pattern 3x1  0020-0022");
    editor.edit.pattern = (1, 1);
    assert_eq!(editor.picker_status(&picker), "Tile 0020");
    let _ = at(0, 0);
}
