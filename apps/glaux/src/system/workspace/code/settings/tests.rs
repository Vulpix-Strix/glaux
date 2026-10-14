use super::*;
use crate::system::editor_settings::IndentStyle;
use pretty_assertions::assert_eq;

fn layout() -> SettingsLayout {
    let code = super::super::layout::CodeLayout::new(
        crate::system::workspace::tests_support::CODE_METRICS,
        crate::system::test_font::LAYOUT_FONT.clone(),
    );
    super::super::settings_layout(&code)
}

fn centre(rect: Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

fn press(at: Point) -> InputEvent {
    InputEvent::PointerDown {
        button: PointerButton::Primary,
        at,
        select: false,
    }
}

fn release(at: Point) -> InputEvent {
    InputEvent::PointerUp {
        button: PointerButton::Primary,
        at,
    }
}

#[test]
fn keys_change_the_focused_setting_at_once() {
    let layout = layout();
    let mut settings = EditorSettings::default();
    let mut dialog = SettingsDialog::new(settings);
    let mut send =
        |dialog: &mut SettingsDialog, event| dialog.handle_input(event, &mut settings, &layout);

    // Left and Right only step values on rows that have one.
    send(&mut dialog, InputEvent::Right { select: false });
    send(&mut dialog, InputEvent::Enter);
    send(&mut dialog, InputEvent::Down { select: false });
    send(&mut dialog, InputEvent::Right { select: false });
    send(&mut dialog, InputEvent::Down { select: false });
    send(&mut dialog, InputEvent::Text(' '));
    send(&mut dialog, InputEvent::Down { select: false });
    send(&mut dialog, InputEvent::Enter);
    assert_eq!(
        settings,
        EditorSettings {
            word_wrap: true,
            tab_size: 5,
            line_numbers: false,
            ligatures: false,
            ..EditorSettings::default()
        }
    );
    assert_eq!(dialog.opened_with(), EditorSettings::default());
}

#[test]
fn the_tab_size_never_leaves_its_range() {
    let layout = layout();
    let mut settings = EditorSettings::default();
    let mut dialog = SettingsDialog::new(settings);
    dialog.handle_input(InputEvent::Down { select: false }, &mut settings, &layout);
    for _ in 0..10 {
        dialog.handle_input(InputEvent::Left { select: false }, &mut settings, &layout);
    }
    assert_eq!(settings.tab_size, 1);
    for _ in 0..10 {
        dialog.handle_input(InputEvent::Right { select: false }, &mut settings, &layout);
    }
    assert_eq!(settings.tab_size, 8);
}

#[test]
fn escape_and_done_both_close() {
    let layout = layout();
    let mut settings = EditorSettings::default();
    let mut dialog = SettingsDialog::new(settings);
    assert_eq!(
        dialog.handle_input(InputEvent::Break, &mut settings, &layout),
        SettingsAction::Close
    );
    for _ in 0..Row::ALL.len() {
        dialog.handle_input(InputEvent::Down { select: false }, &mut settings, &layout);
    }
    assert_eq!(dialog.focus(), Row::Done);
    assert_eq!(
        dialog.handle_input(InputEvent::Enter, &mut settings, &layout),
        SettingsAction::Close
    );
    assert_eq!(settings, EditorSettings::default(), "Done changes nothing");
}

#[test]
fn clicks_toggle_rows_and_step_the_arrows() {
    let layout = layout();
    let mut settings = EditorSettings::default();
    let mut dialog = SettingsDialog::new(settings);
    let mut click = |at: Point| {
        let pressed = dialog.handle_input(press(at), &mut settings, &layout);
        let released = dialog.handle_input(release(at), &mut settings, &layout);
        (pressed, released)
    };
    let stays = (SettingsAction::None, SettingsAction::None);
    assert_eq!(click(centre(layout.rows[0])), stays);
    assert_eq!(click(centre(layout.rows[2])), stays);
    assert_eq!(click(centre(layout.rows[3])), stays);
    let (minus, plus) = layout.arrows(Row::TabSize).unwrap();
    assert_eq!(click(centre(minus)), stays);
    assert_eq!(click(centre(minus)), stays);
    assert_eq!(click(centre(plus)), stays);
    let (_, plus) = layout.arrows(Row::IndentWith).unwrap();
    assert_eq!(click(centre(plus)), stays);
    let (minus, _) = layout.arrows(Row::IndentSize).unwrap();
    assert_eq!(click(centre(minus)), stays);
    assert_eq!(
        settings,
        EditorSettings {
            word_wrap: true,
            tab_size: 3,
            line_numbers: false,
            ligatures: false,
            indent: IndentStyle::Spaces,
            indent_size: 3,
        }
    );
}

#[test]
fn done_closes_only_when_released_where_it_was_pressed() {
    let layout = layout();
    let mut settings = EditorSettings::default();
    let mut dialog = SettingsDialog::new(settings);
    let done = centre(layout.done);
    dialog.handle_input(press(done), &mut settings, &layout);
    let outside = centre(layout.title_bar);
    assert_eq!(
        dialog.handle_input(release(outside), &mut settings, &layout),
        SettingsAction::None
    );
    dialog.handle_input(press(done), &mut settings, &layout);
    assert_eq!(
        dialog.handle_input(release(done), &mut settings, &layout),
        SettingsAction::Close
    );
}

#[test]
fn the_window_fits_inside_its_area_with_its_parts_apart() {
    let layout = layout();
    assert!(layout.window.x >= 0 && layout.window.right() <= 320);
    assert!(layout.window.y >= 16 && layout.window.bottom() <= 228);
    let mut parts = layout.rows.to_vec();
    parts.push(layout.done);
    for pair in parts.windows(2) {
        assert!(pair[0].bottom() <= pair[1].y, "{pair:?}");
    }
    for row in [Row::TabSize, Row::IndentWith, Row::IndentSize] {
        let (minus, plus) = layout.arrows(row).unwrap();
        let rect = layout.rows[row.index()];
        assert!(minus.right() < plus.x, "{row:?}");
        assert!(plus.right() <= rect.right(), "{row:?}");
        assert!(
            (minus.y, minus.bottom()) == (rect.y, rect.bottom()),
            "{row:?}"
        );
    }
    assert_eq!(layout.arrows(Row::WordWrap), None);
}

#[test]
fn keys_step_the_indentation_rows() {
    let layout = layout();
    let mut settings = EditorSettings::default();
    let mut dialog = SettingsDialog::new(settings);
    let mut send =
        |dialog: &mut SettingsDialog, event| dialog.handle_input(event, &mut settings, &layout);
    for _ in 0..4 {
        send(&mut dialog, InputEvent::Down { select: false });
    }
    assert_eq!(dialog.focus(), Row::IndentWith);
    send(&mut dialog, InputEvent::Enter);
    send(&mut dialog, InputEvent::Right { select: false });
    send(&mut dialog, InputEvent::Tab { reverse: false });
    send(&mut dialog, InputEvent::Left { select: false });
    // Shift+Tab goes back up a row.
    send(&mut dialog, InputEvent::Tab { reverse: true });
    assert_eq!(dialog.focus(), Row::IndentWith);
    assert_eq!(
        (settings.indent, settings.indent_size),
        (IndentStyle::Tabs, 3)
    );
}
