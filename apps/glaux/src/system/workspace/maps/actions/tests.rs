use pretty_assertions::assert_eq;

use super::super::fixture::{at, document, editor, entry, input, key, middle, press};
use super::*;
use crate::system::workspace::context_menu::MenuEntry;
use glaux_input::InputEvent;
use glaux_machine::tilemap::BgBank;

fn menu_labels(editor: &MapEditor) -> Vec<(String, bool)> {
    let Some(Overlay::LayerMenu(_, menu)) = &editor.overlay else {
        panic!("the layer menu is open");
    };
    menu.entries()
        .iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item { label, enabled, .. } => Some((label.clone(), *enabled)),
            MenuEntry::Header(label) => Some((label.clone(), false)),
            MenuEntry::Separator => None,
        })
        .collect()
}

#[test]
fn the_layer_menu_names_the_layer_and_offers_every_action() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    doc.set_layer_name(BgLayer::Bg2, Some("Decor"));
    editor.open_layer_menu(BgLayer::Bg2, at(100, 100), &doc);
    let labels = menu_labels(&editor);
    assert_eq!(labels[0].0, "BG2  Decor");
    let names: Vec<_> = labels.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(
        names,
        [
            "BG2  Decor",
            "Rename...",
            "Edit this layer",
            "Visible",
            "Locked",
            "Show only BG2",
            "Select all cells",
            "Clear layer..."
        ]
    );
    assert!(!labels[7].1, "an empty layer has nothing to clear");

    doc.set_cell(BgLayer::Bg2, 0, 0, entry(5));
    editor.open_layer_menu(BgLayer::Bg2, at(100, 100), &doc);
    assert!(menu_labels(&editor)[7].1);
    editor.edit.locked[2] = true;
    editor.open_layer_menu(BgLayer::Bg2, at(100, 100), &doc);
    assert!(!menu_labels(&editor)[7].1, "a locked layer is not cleared");
}

#[test]
fn show_only_hides_the_others_and_a_second_use_shows_all_again() {
    let mut editor = editor();
    editor.show_only(BgLayer::Bg1);
    assert_eq!(editor.edit.visible, [false, true, false]);
    editor.show_only(BgLayer::Bg1);
    assert_eq!(editor.edit.visible, [true; 3]);
    editor.show_only(BgLayer::Bg0);
    assert_eq!(editor.edit.visible, [true, false, false]);
}

#[test]
fn the_keys_toggle_visibility_and_lock_of_the_layer_being_edited() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    editor.edit.layer = BgLayer::Bg0;
    key(&mut editor, &mut doc, 'v');
    key(&mut editor, &mut doc, 'l');
    assert_eq!(editor.edit.visible, [false, true, true]);
    assert_eq!(editor.edit.locked, [true, false, false]);
    key(&mut editor, &mut doc, 'V');
    assert_eq!(editor.edit.visible, [true, false, false]);
}

#[test]
fn select_all_takes_the_whole_map() {
    let mut editor = editor();
    let mut doc = document(12, 7);
    input(&mut editor, &mut doc, InputEvent::SelectAll);
    let selection = editor.edit.selection.unwrap();
    assert_eq!((selection.width(), selection.height()), (12, 7));
}

fn answer_no(editor: &mut MapEditor, doc: &mut MapDocument) {
    input(editor, doc, InputEvent::Enter);
}

fn answer_yes(editor: &mut MapEditor, doc: &mut MapDocument) {
    input(editor, doc, InputEvent::Up { select: false });
    input(editor, doc, InputEvent::Enter);
}

#[test]
fn clearing_a_layer_asks_first_and_is_one_undo_step() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    for x in 0..8 {
        doc.set_cell(BgLayer::Bg2, x, 3, entry(4));
    }
    doc.set_cell(BgLayer::Bg1, 0, 0, entry(2));
    editor.run_layer_action(BgLayer::Bg2, LayerAction::Clear, Some(&doc));
    assert!(matches!(editor.overlay, Some(Overlay::Confirm(Confirm::Clear(_), _))));

    answer_no(&mut editor, &mut doc);
    assert!(editor.overlay.is_none());
    assert!(!doc.layer(BgLayer::Bg2).is_blank(), "cancel changes nothing");

    editor.run_layer_action(BgLayer::Bg2, LayerAction::Clear, Some(&doc));
    answer_yes(&mut editor, &mut doc);
    assert!(doc.layer(BgLayer::Bg2).is_blank());
    assert!(!doc.layer(BgLayer::Bg1).is_blank(), "other layers stay");
    doc.undo();
    assert_eq!(doc.layer(BgLayer::Bg2).get(5, 3), Some(entry(4)));
    assert!(doc.layer(BgLayer::Bg2).get(0, 3).is_some_and(|e| e == entry(4)));
}

#[test]
fn replacing_a_tile_keeps_each_cells_attributes_and_asks_first() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    let flipped = TileEntry::new(7, true, false, true, BgBank::new(2).unwrap()).unwrap();
    doc.set_cell(BgLayer::Bg1, 1, 1, flipped);
    doc.set_cell(BgLayer::Bg1, 2, 2, entry(7));
    doc.set_cell(BgLayer::Bg1, 3, 3, entry(8));
    editor.edit.brush = entry(9);

    editor.open_tile_menu(7, at(50, 150), &doc);
    let out = editor.run_tile_action(7, TileAction::Replace, Some(&doc));
    assert_eq!(out, None);
    assert!(matches!(editor.overlay, Some(Overlay::Confirm(Confirm::Replace { from: 7, to: 9 }, _))));
    answer_yes(&mut editor, &mut doc);

    let layer = doc.layer(BgLayer::Bg1);
    let moved = layer.get(1, 1).unwrap();
    assert_eq!((moved.tile(), moved.flip_x(), moved.priority(), moved.bank().get()), (9, true, true, 2));
    assert_eq!(layer.get(2, 2).unwrap().tile(), 9);
    assert_eq!(layer.get(3, 3).unwrap().tile(), 8, "other tiles stay");
    doc.undo();
    assert_eq!(doc.layer(BgLayer::Bg1).get(1, 1), Some(flipped));
    assert_eq!(doc.layer(BgLayer::Bg1).get(2, 2), Some(entry(7)));
}

#[test]
fn the_tile_menu_disables_what_cannot_apply() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    doc.set_cell(BgLayer::Bg1, 0, 0, entry(7));
    editor.edit.brush = entry(9);
    let enabled = |editor: &MapEditor| {
        let Some(Overlay::TileMenu(_, menu)) = &editor.overlay else {
            panic!("the tile menu is open");
        };
        menu.entries()
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Item { label, enabled, .. } => Some((label.clone(), *enabled)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    editor.open_tile_menu(7, at(50, 150), &doc);
    assert!(enabled(&editor).iter().all(|(_, on)| *on));

    editor.open_tile_menu(12, at(50, 150), &doc);
    let unused = enabled(&editor);
    assert_eq!(
        unused.iter().map(|(_, on)| *on).collect::<Vec<_>>(),
        [true, true, false, false],
        "an unused tile has nothing to select or replace"
    );

    editor.edit.locked[1] = true;
    editor.open_tile_menu(7, at(50, 150), &doc);
    assert_eq!(enabled(&editor)[3].1, false, "a locked layer is not replaced into");

    editor.open_tile_menu(0, at(50, 150), &doc);
    assert!(!enabled(&editor)[0].1, "tile 0 draws nothing");
}

#[test]
fn selecting_a_tile_on_the_layer_takes_the_rectangle_around_its_cells() {
    let mut editor = editor();
    let mut doc = document(10, 10);
    doc.set_cell(BgLayer::Bg1, 2, 3, entry(7));
    doc.set_cell(BgLayer::Bg1, 6, 8, entry(7));
    editor.run_tile_action(7, TileAction::SelectOnLayer, Some(&doc));
    let selection = editor.edit.selection.unwrap();
    assert_eq!(
        (selection.left, selection.top, selection.right, selection.bottom),
        (2, 3, 6, 8)
    );
}

#[test]
fn editing_a_tile_asks_for_it_with_the_brushs_bank() {
    let mut editor = editor();
    editor.edit.brush = TileEntry::new(3, false, false, false, BgBank::new(2).unwrap()).unwrap();
    assert_eq!(
        editor.run_tile_action(40, TileAction::EditInSprites, None),
        Some(MapOut::EditTile {
            tile: 40,
            bank: BgBank::new(2).unwrap()
        })
    );
}

fn canvas_cell(editor: &MapEditor, doc: &MapDocument, cell: (i32, i32)) -> glaux_input::Point {
    let frame = editor.frame();
    let view = editor.viewport(&frame, doc);
    middle(view.cell_rect(frame.canvas, cell))
}

fn select(editor: &mut MapEditor, doc: &mut MapDocument, from: (i32, i32), to: (i32, i32)) {
    editor.edit.tool = Tool::Select;
    let start = canvas_cell(editor, doc, from);
    let end = canvas_cell(editor, doc, to);
    press(editor, doc, start);
    input(editor, doc, InputEvent::PointerMove(Some(end)));
    super::super::fixture::release(editor, doc, end);
}

use super::super::tools::Tool;

#[test]
fn copy_and_paste_move_a_block_as_one_edit_cut_at_the_maps_edge() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    doc.set_cell(BgLayer::Bg1, 1, 1, entry(5));
    doc.set_cell(BgLayer::Bg1, 2, 1, entry(6));
    doc.set_cell(BgLayer::Bg1, 1, 2, entry(7));
    doc.set_cell(BgLayer::Bg1, 2, 2, entry(8));
    select(&mut editor, &mut doc, (1, 1), (2, 2));
    assert!(matches!(input(&mut editor, &mut doc, InputEvent::Copy), Some(MapOut::Notice(_))));

    let target = canvas_cell(&editor, &doc, (7, 7));
    input(&mut editor, &mut doc, InputEvent::PointerMove(Some(target)));
    input(&mut editor, &mut doc, InputEvent::Paste);
    let layer = doc.layer(BgLayer::Bg1);
    assert_eq!(layer.get(7, 7), Some(entry(5)));
    let pasted = editor.edit.selection.unwrap();
    assert_eq!((pasted.left, pasted.top, pasted.width(), pasted.height()), (7, 7, 1, 1));
    doc.undo();
    assert_eq!(doc.layer(BgLayer::Bg1).get(7, 7), Some(TileEntry::EMPTY));
    assert_eq!(doc.layer(BgLayer::Bg1).get(1, 1), Some(entry(5)), "the source stays");
}

#[test]
fn cut_clears_the_selection_and_pastes_back_elsewhere() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    doc.set_cell(BgLayer::Bg1, 0, 0, entry(5));
    doc.set_cell(BgLayer::Bg1, 1, 0, entry(6));
    select(&mut editor, &mut doc, (0, 0), (1, 0));
    input(&mut editor, &mut doc, InputEvent::Cut);
    assert!(doc.layer(BgLayer::Bg1).is_blank());

    let target = canvas_cell(&editor, &doc, (4, 4));
    input(&mut editor, &mut doc, InputEvent::PointerMove(Some(target)));
    input(&mut editor, &mut doc, InputEvent::Paste);
    assert_eq!(doc.layer(BgLayer::Bg1).get(4, 4), Some(entry(5)));
    assert_eq!(doc.layer(BgLayer::Bg1).get(5, 4), Some(entry(6)));
}

#[test]
fn a_locked_layer_refuses_cut_and_paste_and_an_empty_clipboard_says_so() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    assert!(matches!(
        input(&mut editor, &mut doc, InputEvent::Paste),
        Some(MapOut::Notice(_))
    ));
    doc.set_cell(BgLayer::Bg1, 0, 0, entry(5));
    select(&mut editor, &mut doc, (0, 0), (0, 0));
    input(&mut editor, &mut doc, InputEvent::Copy);
    editor.edit.locked[1] = true;

    assert!(matches!(input(&mut editor, &mut doc, InputEvent::Cut), Some(MapOut::Notice(_))));
    assert_eq!(doc.layer(BgLayer::Bg1).get(0, 0), Some(entry(5)));
    let target = canvas_cell(&editor, &doc, (3, 3));
    input(&mut editor, &mut doc, InputEvent::PointerMove(Some(target)));
    assert!(matches!(input(&mut editor, &mut doc, InputEvent::Paste), Some(MapOut::Notice(_))));
    assert_eq!(doc.layer(BgLayer::Bg1).get(3, 3), Some(TileEntry::EMPTY));
}
