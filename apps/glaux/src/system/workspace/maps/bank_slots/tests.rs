use glaux_input::InputEvent;
use glaux_machine::palette::Rgb;
use pretty_assertions::assert_eq;

use super::*;
use crate::system::map_document::MapDocument;
use crate::system::workspace::maps::fixture::{
    click, document, editor, input_with, middle, sprites,
};

fn bank(index: u8) -> BgBank {
    BgBank::new(index).unwrap()
}

/// The slots of BGP1 open over the palette selector.
fn open() -> (MapEditor, MapDocument, SpriteDocument) {
    let (mut editor, map, mut sprites) = (editor(), document(8, 8), sprites());
    let slots = SlotEditor::open(bank(1), &mut sprites);
    editor.edit.panels = vec![Panel::Banks, Panel::Slots(slots)];
    (editor, map, sprites)
}

fn layout(editor: &MapEditor, sprites: &SpriteDocument, page: u8) -> SlotLayout {
    let video = sprites.video();
    editor.slot_layout(&video.palette, video.bg.palettes(), page)
}

fn resolve(sprites: &SpriteDocument, local: u8) -> u8 {
    sprites.bg().palettes().resolve(bank(1), local).unwrap()
}

#[test]
fn a_master_is_pointed_at_live_and_escape_takes_it_back_without_a_trace() {
    let (mut editor, mut map, mut sprites) = open();
    let first = resolve(&sprites, 3);
    let parts = layout(&editor, &sprites, 0);
    click(&mut editor, &mut map, &mut sprites, middle(parts.slots[3]));
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(parts.cell(200).unwrap()),
    );
    assert_eq!(
        resolve(&sprites, 3),
        200,
        "the document holds it while it is pending"
    );
    assert!(editor.has_overlay(), "a pending change takes every input");

    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Break);
    assert_eq!(resolve(&sprites, 3), first);
    assert!(!sprites.can_undo() && !sprites.can_redo() && !sprites.is_modified());
    assert_eq!(editor.edit.panels, vec![Panel::Banks]);
}

#[test]
fn applying_leaves_one_undo_step_for_every_slot_changed() {
    let (mut editor, mut map, mut sprites) = open();
    let (third, fifth) = (resolve(&sprites, 3), resolve(&sprites, 5));
    let parts = layout(&editor, &sprites, 0);
    click(&mut editor, &mut map, &mut sprites, middle(parts.slots[3]));
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(parts.cell(200).unwrap()),
    );
    click(&mut editor, &mut map, &mut sprites, middle(parts.slots[5]));
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(parts.cell(201).unwrap()),
    );
    click(&mut editor, &mut map, &mut sprites, middle(parts.apply));
    assert_eq!(editor.edit.panels, vec![Panel::Banks]);
    assert_eq!((resolve(&sprites, 3), resolve(&sprites, 5)), (200, 201));

    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Undo);
    assert_eq!((resolve(&sprites, 3), resolve(&sprites, 5)), (third, fifth));
    assert!(!sprites.can_undo());
}

#[test]
fn revert_and_undo_drop_what_is_pending_and_keep_the_window_open() {
    let (mut editor, mut map, mut sprites) = open();
    let first = resolve(&sprites, 1);
    let parts = layout(&editor, &sprites, 0);
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(parts.cell(90).unwrap()),
    );
    click(&mut editor, &mut map, &mut sprites, middle(parts.revert));
    assert_eq!(resolve(&sprites, 1), first);

    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(parts.cell(91).unwrap()),
    );
    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Undo);
    assert_eq!(resolve(&sprites, 1), first);
    assert!(matches!(editor.edit.panels.last(), Some(Panel::Slots(_))));
    assert!(
        !sprites.can_redo(),
        "taking back a pending change is no undo step"
    );
}

#[test]
fn slot_0_stays_transparent() {
    let (mut editor, mut map, mut sprites) = open();
    let parts = layout(&editor, &sprites, 0);
    click(&mut editor, &mut map, &mut sprites, middle(parts.slots[0]));
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(parts.cell(77).unwrap()),
    );
    assert_eq!(sprites.bg().palettes().resolve(bank(1), 0), None);
    assert_eq!(
        resolve(&sprites, 1),
        77,
        "the click went to the slot selected before"
    );
}

#[test]
fn the_default_master_palette_shows_whole_and_a_distinct_one_by_halves() {
    let (mut editor, mut map, mut sprites) = open();
    let video = sprites.video();
    assert_eq!(
        editor.master_range(&video.palette, video.bg.palettes(), 1),
        0..=255
    );

    // 256 colors that are neither repeated nor the system's.
    for index in 0..=255u8 {
        let color = Rgb {
            r: 1,
            g: index,
            b: 255 - index,
        };
        sprites.set_master_color(index, color);
    }
    let video = sprites.video();
    assert_eq!(
        editor.master_range(&video.palette, video.bg.palettes(), 0),
        0..=127
    );
    assert_eq!(
        editor.master_range(&video.palette, video.bg.palettes(), 1),
        128..=255
    );

    let first = layout(&editor, &sprites, 0);
    assert_eq!(first.cell(127).map(|cell| cell.height), Some(10));
    assert_eq!(first.cell(128), None);
    input_with(
        &mut editor,
        &mut map,
        &mut sprites,
        InputEvent::PageDown { select: false },
    );
    let second = layout(&editor, &sprites, 1);
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(second.cell(130).unwrap()),
    );
    assert_eq!(resolve(&sprites, 1), 130);
    assert_eq!(first.cell(2).map(middle), second.cell(130).map(middle));
}

#[test]
fn the_frame_palette_holds_the_masters_only_while_the_grid_is_on_top() {
    let (mut editor, mut map, mut sprites) = open();
    assert_eq!(editor.shown_masters(&sprites), Some(0..=255));
    assert_eq!(editor.scratch_colors(), None);

    let parts = layout(&editor, &sprites, 0);
    click(&mut editor, &mut map, &mut sprites, middle(parts.edit_rgb));
    assert_eq!(editor.shown_masters(&sprites), None, "the grid is covered");
    assert!(editor.scratch_colors().is_some());

    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Break);
    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Break);
    assert_eq!(editor.shown_masters(&sprites), None);
}
