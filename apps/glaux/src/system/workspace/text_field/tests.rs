use super::*;
use pretty_assertions::assert_eq;

fn typed(field: &mut TextField, text: &str) {
    for ch in text.chars() {
        field.handle_input(InputEvent::Text(ch));
    }
}

#[test]
fn select_all_and_clipboard_commands_act_on_the_field() {
    let mut field = TextField::any(32);
    typed(&mut field, "speed");
    assert_eq!(
        field.handle_input(InputEvent::Copy),
        FieldInput::Kept,
        "nothing selected"
    );
    assert_eq!(field.handle_input(InputEvent::SelectAll), FieldInput::Kept);
    assert_eq!(
        field.handle_input(InputEvent::Copy),
        FieldInput::Copy("speed".into())
    );
    assert_eq!(field.text(), "speed");
    assert_eq!(
        field.handle_input(InputEvent::Cut),
        FieldInput::Copy("speed".into())
    );
    assert_eq!(field.text(), "");
    assert_eq!(field.handle_input(InputEvent::Paste), FieldInput::Paste);
    field.paste("max_speed\nmore");
    assert_eq!(field.text(), "max_speed");
}

#[test]
fn undo_and_redo_are_taken_without_effect() {
    let mut field = TextField::any(32);
    typed(&mut field, "ab");
    for event in [InputEvent::Undo, InputEvent::Redo] {
        assert_eq!(field.handle_input(event), FieldInput::Kept);
    }
    assert_eq!(field.text(), "ab");
}

#[test]
fn window_commands_are_left_to_the_window() {
    let mut field = TextField::any(32);
    for event in [
        InputEvent::Enter,
        InputEvent::Break,
        InputEvent::Tab { reverse: false },
        InputEvent::Up { select: false },
        InputEvent::Save,
        InputEvent::Find,
    ] {
        assert_eq!(
            field.handle_input(event),
            FieldInput::Unhandled,
            "{event:?}"
        );
    }
}

#[test]
fn only_accepted_characters_fit_up_to_the_limit() {
    let mut field = TextField::new(3, Accepts::Digits);
    typed(&mut field, "1a2");
    assert_eq!(field.text(), "12");
    field.paste("3456");
    assert_eq!(field.text(), "123");
    assert_eq!(field.handle_input(InputEvent::Text('7')), FieldInput::Kept);
    // Text set from outside is filtered the same way and selected whole.
    field.set_text("9x8");
    assert_eq!(field.text(), "98");
    typed(&mut field, "5");
    assert_eq!(field.text(), "5");
}
