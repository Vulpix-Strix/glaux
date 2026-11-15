use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::tilemap::{BgLayer, TileEntry};
use pretty_assertions::assert_eq;

use super::*;
use crate::system::map_document::MapDocument;
use crate::system::workspace::maps::fixture::{
    at, click, document, editor, entry, input_with, middle, sprites,
};
use crate::system::workspace::request::MapRequest;

fn pixel(layout: &TileEditorLayout, x: i32, y: i32) -> Point {
    at(
        layout.canvas.x + x * SCALE + 4,
        layout.canvas.y + y * SCALE + 4,
    )
}

/// An editor on tile 2 of a cartridge whose tile 2 has one pixel, with a
/// map that already has one cell painted.
fn open() -> (MapEditor, MapDocument, SpriteDocument, TileEditorLayout) {
    let (mut editor, mut map, mut sprites) = (editor(), document(8, 8), sprites());
    sprites.set_bg_pixel(2, 0, 0, 1);
    map.set_cell(BgLayer::Bg1, 0, 0, entry(2));
    editor.edit.brush = entry(2);
    let out = input_with(&mut editor, &mut map, &mut sprites, InputEvent::Text('d'));
    assert_eq!(out, Some(MapOut::Request(MapRequest::Survey)));
    let layout = TileEditorLayout::new(editor.metrics.top, false);
    (editor, map, sprites, layout)
}

#[test]
fn a_drag_draws_one_undo_step_of_the_tile_and_none_of_the_map() {
    let (mut editor, mut map, mut sprites, layout) = open();
    let down = InputEvent::PointerDown {
        button: PointerButton::Primary,
        at: pixel(&layout, 1, 1),
        select: false,
    };
    input_with(&mut editor, &mut map, &mut sprites, down);
    let to = pixel(&layout, 4, 1);
    input_with(
        &mut editor,
        &mut map,
        &mut sprites,
        InputEvent::PointerMove(Some(to)),
    );
    let up = InputEvent::PointerUp {
        button: PointerButton::Primary,
        at: to,
    };
    input_with(&mut editor, &mut map, &mut sprites, up);
    for x in 1..=4 {
        assert_eq!(sprites.bg_pixel(2, x, 1), Some(1), "pixel {x}");
    }

    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Undo);
    for x in 1..=4 {
        assert_eq!(sprites.bg_pixel(2, x, 1), Some(0), "pixel {x}");
    }
    assert_eq!(map.layer(BgLayer::Bg1).get(0, 0), Some(entry(2)));
    assert!(map.can_undo(), "the map's history is its own");
}

#[test]
fn undo_goes_back_to_the_map_once_the_editor_is_closed() {
    let (mut editor, mut map, mut sprites, layout) = open();
    click(&mut editor, &mut map, &mut sprites, pixel(&layout, 3, 3));
    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Text('d'));
    assert!(!editor.has_panel());

    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Undo);
    assert_eq!(map.layer(BgLayer::Bg1).get(0, 0), Some(TileEntry::EMPTY));
    assert_eq!(
        sprites.bg_pixel(2, 3, 3),
        Some(1),
        "the tile keeps its pixel"
    );
}

#[test]
fn the_toolbar_undo_button_acts_on_the_tile_while_the_editor_is_open() {
    let (mut editor, mut map, mut sprites, layout) = open();
    click(&mut editor, &mut map, &mut sprites, pixel(&layout, 3, 3));
    let undo = middle(editor.frame().undo);
    click(&mut editor, &mut map, &mut sprites, undo);
    assert_eq!(sprites.bg_pixel(2, 3, 3), Some(0));
    assert!(editor.has_panel(), "the editor stays open");
    assert!(map.can_undo());
}

#[test]
fn the_tools_and_swatches_work_as_in_the_sprite_editor() {
    let (mut editor, mut map, mut sprites, layout) = open();
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(layout.swatches[7]),
    );
    click(&mut editor, &mut map, &mut sprites, pixel(&layout, 5, 5));
    assert_eq!(sprites.bg_pixel(2, 5, 5), Some(7));

    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Text('f'));
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(layout.swatches[9]),
    );
    click(&mut editor, &mut map, &mut sprites, pixel(&layout, 7, 7));
    assert_eq!(
        sprites.bg_pixel(2, 7, 0),
        Some(9),
        "the fill reaches the tile's edge"
    );
    assert_eq!(sprites.bg_pixel(2, 5, 5), Some(7));

    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Text('i'));
    click(&mut editor, &mut map, &mut sprites, pixel(&layout, 5, 5));
    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Text('p'));
    click(&mut editor, &mut map, &mut sprites, pixel(&layout, 6, 6));
    assert_eq!(
        sprites.bg_pixel(2, 6, 6),
        Some(7),
        "the eyedropper picked 7"
    );

    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(layout.swatches[0]),
    );
    click(&mut editor, &mut map, &mut sprites, pixel(&layout, 6, 5));
    assert_eq!(
        sprites.bg_pixel(2, 6, 5),
        Some(7),
        "slot 0 cannot be picked"
    );
}

#[test]
fn placing_a_new_tile_closes_the_editor_with_the_pencil_on_it() {
    let (mut editor, mut map, mut sprites) = (editor(), document(8, 8), sprites());
    editor.edit.tool = Tool::Hand;
    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Text('n'));
    let layout = TileEditorLayout::new(editor.metrics.top, true);
    click(&mut editor, &mut map, &mut sprites, pixel(&layout, 0, 0));
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(layout.place.unwrap()),
    );
    assert!(!editor.has_panel());
    assert_eq!(
        (editor.edit.tool, editor.edit.brush.tile()),
        (Tool::Pencil, 1)
    );

    let canvas = editor.frame().canvas;
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        at(canvas.x + 1, canvas.y + 1),
    );
    assert_eq!(
        map.layer(BgLayer::Bg1).get(0, 0).map(|cell| cell.tile()),
        Some(1)
    );
}

#[test]
fn a_press_outside_closes_the_editor_without_painting_the_map() {
    let (mut editor, mut map, mut sprites, _) = open();
    let canvas = editor.frame().canvas;
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        at(canvas.x + 3, canvas.bottom() - 3),
    );
    assert!(!editor.has_panel());
    assert_eq!(map.layer(BgLayer::Bg1).get(0, 7), Some(TileEntry::EMPTY));
}
