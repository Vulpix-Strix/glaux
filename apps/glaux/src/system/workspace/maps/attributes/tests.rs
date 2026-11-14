use pretty_assertions::assert_eq;

use super::super::fixture::{document, editor, entry, input, key, middle, press};
use super::super::tools::Tool;
use super::*;
use crate::system::map_document::MapDocument;
use crate::system::workspace::Rect;
use glaux_input::InputEvent;
use glaux_machine::tilemap::BgLayer;

fn layout(editor: &MapEditor) -> AttrLayout {
    editor.attr_layout(&editor.frame())
}

fn open_with_cells(editor: &mut MapEditor, doc: &mut MapDocument) {
    for (x, y, tile) in [(1, 1, 5), (2, 1, 6), (1, 2, 7)] {
        doc.set_cell(BgLayer::Bg1, x, y, entry(tile));
    }
    editor.edit.tool = Tool::Select;
    editor.edit.selection = super::super::tools::Selection::between((0, 0), (3, 2), (8, 8));
    key(editor, doc, 'a');
}

#[test]
fn the_panel_fits_below_the_options_bar_and_its_parts_sit_inside_it() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    open_with_cells(&mut editor, &mut doc);
    let frame = editor.frame();
    let layout = layout(&editor);
    assert!(layout.window.y > frame.options.y);
    assert!(layout.window.right() <= frame.drawer.right());
    assert!(layout.window.bottom() < frame.drawer_bar.y);
    let inside = |rect: Rect| {
        rect.x >= layout.window.x
            && rect.right() <= layout.window.right()
            && rect.y >= layout.window.y
            && rect.bottom() <= layout.window.bottom()
    };
    for rect in layout.tabs.iter().chain(&layout.banks).chain(&layout.flips).chain(&layout.priorities) {
        assert!(inside(*rect), "{rect:?}");
    }
    assert!(inside(layout.foot));
}

#[test]
fn it_opens_on_cells_when_something_is_selected_and_the_key_closes_it() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    key(&mut editor, &mut doc, 'a');
    assert_eq!(editor.edit.attrs.unwrap().tab, AttrTab::Brush);
    key(&mut editor, &mut doc, 'a');
    assert!(editor.edit.attrs.is_none());
    open_with_cells(&mut editor, &mut doc);
    assert_eq!(editor.edit.attrs.unwrap().tab, AttrTab::Cells);
    input(&mut editor, &mut doc, InputEvent::Break);
    assert!(editor.edit.attrs.is_none());
}

#[test]
fn the_brush_tab_edits_the_brush_at_once() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    key(&mut editor, &mut doc, 'a');
    let layout = layout(&editor);
    press(&mut editor, &mut doc, middle(layout.banks[3]));
    press(&mut editor, &mut doc, middle(layout.flips[0]));
    press(&mut editor, &mut doc, middle(layout.flips[1]));
    press(&mut editor, &mut doc, middle(layout.priorities[1]));
    let brush = editor.edit.brush;
    assert_eq!((brush.bank().get(), brush.flip_x(), brush.flip_y(), brush.priority()), (3, true, true, true));
    press(&mut editor, &mut doc, middle(layout.flips[0]));
    press(&mut editor, &mut doc, middle(layout.priorities[0]));
    assert!(!editor.edit.brush.flip_x() && !editor.edit.brush.priority());
    assert_eq!(editor.edit.brush.tile(), 1, "the tile stays");
    assert!(doc.layer(BgLayer::Bg1).is_blank(), "the map is not touched");
}

#[test]
fn the_cells_tab_changes_the_tiled_cells_as_one_edit_and_leaves_empty_ones() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    open_with_cells(&mut editor, &mut doc);
    let layout = layout(&editor);
    press(&mut editor, &mut doc, middle(layout.banks[2]));
    let layer = doc.layer(BgLayer::Bg1);
    for (x, y) in [(1, 1), (2, 1), (1, 2)] {
        assert_eq!(layer.get(x, y).unwrap().bank().get(), 2);
    }
    assert_eq!(layer.get(0, 0), Some(TileEntry::EMPTY), "an empty cell stays empty");
    doc.undo();
    assert_eq!(doc.layer(BgLayer::Bg1).get(2, 1), Some(entry(6)));
    assert_eq!(doc.layer(BgLayer::Bg1).get(1, 1), Some(entry(5)));
}

#[test]
fn a_mixed_flip_turns_on_first_and_a_locked_layer_refuses_the_edit() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    open_with_cells(&mut editor, &mut doc);
    doc.set_cell(BgLayer::Bg1, 1, 1, TileEntry::new(5, true, false, false, BgBank::ZERO).unwrap());
    let layout = layout(&editor);
    press(&mut editor, &mut doc, middle(layout.flips[0]));
    let layer = doc.layer(BgLayer::Bg1);
    assert!([(1, 1), (2, 1), (1, 2)].iter().all(|&(x, y)| layer.get(x, y).unwrap().flip_x()));

    editor.edit.locked[1] = true;
    let out = input(
        &mut editor,
        &mut doc,
        InputEvent::PointerDown {
            button: glaux_input::PointerButton::Primary,
            at: middle(layout.banks[1]),
            select: false,
        },
    );
    assert!(matches!(out, Some(MapOut::Notice(_))));
    assert_eq!(doc.layer(BgLayer::Bg1).get(2, 1).unwrap().bank().get(), 0);
}

#[test]
fn a_press_in_the_panel_does_not_reach_the_canvas_but_one_outside_does() {
    let mut editor = editor();
    let mut doc = document(8, 8);
    key(&mut editor, &mut doc, 'a');
    editor.edit.tool = Tool::Pencil;
    let layout = layout(&editor);
    let inside = middle(layout.window);
    press(&mut editor, &mut doc, inside);
    assert!(doc.layer(BgLayer::Bg1).is_blank());

    let frame = editor.frame();
    let view = editor.viewport(&frame, &doc);
    let outside = middle(view.cell_rect(frame.canvas, (0, 0)));
    assert!(!layout.window.contains(outside));
    press(&mut editor, &mut doc, outside);
    assert!(!doc.layer(BgLayer::Bg1).is_blank());
}
