use glaux_input::InputEvent;
use pretty_assertions::assert_eq;

use super::*;
use crate::system::workspace::maps::fixture::{
    click, document, editor, input_with, middle, sprites,
};

#[test]
fn a_key_or_a_row_chooses_the_bank_the_brush_paints_with() {
    let (mut editor, mut map, mut sprites) = (editor(), document(8, 8), sprites());
    let chip = middle(editor.frame().bank_chip);
    click(&mut editor, &mut map, &mut sprites, chip);
    assert_eq!(editor.edit.panels, vec![Panel::Banks]);

    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Text('2'));
    assert_eq!(editor.edit.brush.bank().get(), 2);
    assert_eq!(
        editor.edit.layer,
        glaux_machine::tilemap::BgLayer::Bg1,
        "not a layer key here"
    );

    let top = editor.metrics.top;
    let name = crate::system::workspace::maps::fixture::at(row(top, 3).x + 20, row(top, 3).y + 4);
    click(&mut editor, &mut map, &mut sprites, name);
    assert_eq!(editor.edit.brush.bank().get(), 3);
    assert!(!sprites.can_undo(), "choosing a bank edits no palette");

    input_with(&mut editor, &mut map, &mut sprites, InputEvent::Break);
    assert!(!editor.has_panel());
}

#[test]
fn edit_opens_the_slots_of_that_bank_over_the_selector() {
    let (mut editor, mut map, mut sprites) = (editor(), document(8, 8), sprites());
    editor.edit.panels = vec![Panel::Banks];
    let edit = middle(edit_button(editor.metrics.top, 1));
    click(&mut editor, &mut map, &mut sprites, edit);
    let [Panel::Banks, Panel::Slots(slots)] = editor.edit.panels.as_slice() else {
        panic!("the slots are over the selector: {:?}", editor.edit.panels);
    };
    assert_eq!(slots.bank.get(), 1);
    assert_eq!(
        editor.edit.brush.bank().get(),
        0,
        "editing a bank does not choose it"
    );
}
