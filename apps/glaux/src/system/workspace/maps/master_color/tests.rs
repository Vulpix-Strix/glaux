use glaux_input::InputEvent;
use glaux_machine::palette::Rgb;
use pretty_assertions::assert_eq;

use super::*;
use crate::system::map_document::MapDocument;
use crate::system::workspace::maps::bank_slots::{SlotEditor, SlotLayout};
use crate::system::workspace::maps::fixture::{
    at, click, document, editor, input_with, middle, sprites,
};

const MASTER: u8 = 200;

fn bank() -> BgBank {
    BgBank::new(1).unwrap()
}

fn color(sprites: &SpriteDocument) -> Rgb {
    sprites.video().palette.get(MASTER)
}

/// BGP1's slot 1 points at `MASTER`, pending, and the master's color is
/// open over the slots.
fn open() -> (MapEditor, MapDocument, SpriteDocument) {
    let (mut editor, mut map, mut sprites) = (editor(), document(8, 8), sprites());
    let slots = SlotEditor::open(bank(), &mut sprites);
    editor.edit.panels = vec![Panel::Banks, Panel::Slots(slots)];
    let parts = SlotLayout::new(editor.metrics.top, 0..=255);
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        middle(parts.cell(MASTER).unwrap()),
    );
    click(&mut editor, &mut map, &mut sprites, middle(parts.edit_rgb));
    assert!(matches!(editor.edit.panels.last(), Some(Panel::Master(_))));
    (editor, map, sprites)
}

#[test]
fn a_slider_changes_the_color_live_and_cancel_restores_it() {
    let (mut editor, mut map, mut sprites) = open();
    let was = color(&sprites);
    let layout = MasterLayout::new(editor.metrics.top);
    let track = layout.tracks[1];
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        at(track.right() - 1, track.y + 2),
    );
    assert_eq!(color(&sprites), Rgb { g: 255, ..was });
    click(
        &mut editor,
        &mut map,
        &mut sprites,
        at(track.x, track.y + 2),
    );
    assert_eq!(color(&sprites), Rgb { g: 0, ..was });

    click(&mut editor, &mut map, &mut sprites, middle(layout.cancel));
    assert_eq!(color(&sprites), was);
    assert!(matches!(editor.edit.panels.last(), Some(Panel::Slots(_))));
}

#[test]
fn typing_six_hex_digits_sets_the_color() {
    let (mut editor, mut map, mut sprites) = open();
    for key in "f4A24a".chars() {
        input_with(&mut editor, &mut map, &mut sprites, InputEvent::Text(key));
    }
    assert_eq!(color(&sprites), Rgb::from_hex(0xf4a24a));
}

#[test]
fn applying_is_an_undo_step_apart_from_the_slot_change_still_pending() {
    let (mut editor, mut map, mut sprites) = open();
    let was = color(&sprites);
    let slot = |sprites: &SpriteDocument| sprites.bg().palettes().resolve(bank(), 1);
    for key in "102030".chars() {
        input_with(&mut editor, &mut map, &mut sprites, InputEvent::Text(key));
    }
    let layout = MasterLayout::new(editor.metrics.top);
    click(&mut editor, &mut map, &mut sprites, middle(layout.apply));
    assert!(matches!(editor.edit.panels.last(), Some(Panel::Slots(_))));
    assert_eq!(color(&sprites), Rgb::from_hex(0x102030));
    assert_eq!(
        slot(&sprites),
        Some(MASTER),
        "the slot change is still pending"
    );

    // Escape takes the pending slot back and keeps the applied color.
    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Break);
    assert_ne!(slot(&sprites), Some(MASTER));
    assert_eq!(color(&sprites), Rgb::from_hex(0x102030));

    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Undo);
    assert_eq!(color(&sprites), was);
    assert!(!sprites.can_undo(), "the color was the only step");
}

#[test]
fn the_ramps_and_both_swatches_take_scratch_entries() {
    let (mut editor, mut map, mut sprites) = open();
    let was = color(&sprites);
    for key in "804020".chars() {
        input_with(&mut editor, &mut map, &mut sprites, InputEvent::Text(key));
    }
    let colors = editor.scratch_colors().unwrap();
    assert_eq!(colors.len(), CHANNELS * STEPS + 2);
    assert_eq!(
        colors[0],
        Rgb {
            r: 7,
            g: 0x40,
            b: 0x20
        }
    );
    assert_eq!(
        colors[2 * STEPS + 15],
        Rgb {
            r: 0x80,
            g: 0x40,
            b: 247
        }
    );
    assert_eq!(colors[CHANNELS * STEPS..], [was, Rgb::from_hex(0x804020)]);
    assert!(SCRATCH_BASE + colors.len() <= 256);
}
