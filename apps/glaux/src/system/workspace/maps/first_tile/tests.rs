use glaux_input::InputEvent;
use glaux_machine::tilemap::BgLayer;
use pretty_assertions::assert_eq;

use super::*;
use crate::system::workspace::maps::fixture::{
    at, click, document, editor, entry, input_with, middle, sprites,
};
use crate::system::workspace::maps::panels::Panel;

#[test]
fn tile_0_is_neither_drawn_nor_free() {
    let mut sprites = sprites();
    assert_eq!(first_drawn(sprites.bg_tiles()), None);
    assert_eq!(first_free(sprites.bg_tiles()), Some(1));
    sprites.set_bg_pixel(0, 0, 0, 5);
    sprites.set_bg_pixel(1, 7, 7, 5);
    sprites.set_bg_pixel(3, 0, 0, 1);
    assert_eq!(first_drawn(sprites.bg_tiles()), Some(1));
    assert_eq!(first_free(sprites.bg_tiles()), Some(2));
}

#[test]
fn n_opens_the_editor_on_the_first_free_tile_and_makes_it_the_brush() {
    let (mut editor, mut map, mut sprites) = (editor(), document(8, 8), sprites());
    sprites.set_bg_pixel(1, 0, 0, 1);
    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Text('n'));
    let Some(Panel::Tile(tile)) = editor.edit.panels.last() else {
        panic!("the tile editor is open");
    };
    assert_eq!((tile.tile, tile.created), (2, true));
    assert_eq!(editor.edit.brush.tile(), 2);
}

#[test]
fn the_guide_shows_only_on_a_blank_map_of_a_cartridge_without_tiles() {
    let (mut editor, mut map, mut sprites) = (editor(), document(8, 8), sprites());
    assert!(editor.shows_guide(&map, sprites.bg_tiles()));
    let frame = editor.frame();
    let button = middle(MapEditor::guide_button(&frame));
    click(&mut editor, &mut map, &mut sprites, button);
    assert!(editor.shows_tile_editor().is_some_and(|tile| tile.created));

    let mut editor = super::super::fixture::editor();
    sprites.set_bg_pixel(4, 0, 0, 1);
    assert!(!editor.shows_guide(&map, sprites.bg_tiles()));
    input_with(
        &mut editor,
        &mut map,
        &mut sprites,
        InputEvent::PointerMove(None),
    );
    assert_eq!(
        editor.edit.brush.tile(),
        4,
        "the brush moves to a drawn tile"
    );
    map.set_cell(BgLayer::Bg1, 0, 0, entry(4));
    sprites.undo();
    assert!(
        !editor.shows_guide(&map, sprites.bg_tiles()),
        "the map is not blank"
    );
}

#[test]
fn the_pencil_does_not_paint_while_no_tile_is_drawn() {
    let (mut editor, mut map, mut sprites) = (editor(), document(8, 8), sprites());
    let canvas = editor.frame().canvas;
    let corner = at(canvas.x + 1, canvas.y + 1);
    let out = click(&mut editor, &mut map, &mut sprites, corner);
    assert_eq!(
        out,
        Some(MapOut::Notice(ui_text::get("maps-draw-a-tile-first")))
    );
    assert!(map_is_blank(&map));
}
