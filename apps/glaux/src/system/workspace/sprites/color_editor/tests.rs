use super::*;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::obj_palette::ObjBank;
use pretty_assertions::assert_eq;

const METRICS: Metrics = Metrics {
    cell_w: 6,
    cell_h: 10,
    top: 16,
};

const OLD: Rgb = Rgb::from_hex(0x4a9bcd);

fn area() -> Rect {
    Rect {
        x: 0,
        y: METRICS.top,
        width: WIDTH as i32,
        height: HEIGHT as i32 - METRICS.top,
    }
}

fn layout(advanced: bool) -> ColorEditorLayout {
    ColorEditorLayout::new(area(), METRICS, advanced)
}

fn target(bank: u8, local: u8) -> ColorTarget {
    ColorTarget {
        bank: ObjBank::new(bank).unwrap(),
        local,
    }
}

fn uses(master: u8, entries: &[(u8, u8)], tiles: usize) -> ColorUses {
    ColorUses {
        master,
        entries: entries.iter().map(|&(b, l)| target(b, l)).collect(),
        tiles,
    }
}

/// An editor on color 5 of palette 2, whose master no other color shares.
fn editor() -> ColorEditor {
    ColorEditor::new(7, target(2, 5), uses(155, &[], 0), OLD)
}

fn point(x: i32, y: i32) -> Point {
    Point::new(x as u16, y as u16)
}

fn centre(rect: Rect) -> Point {
    point(rect.x + rect.width / 2, rect.y + rect.height / 2)
}

fn press(at: Point) -> InputEvent {
    InputEvent::PointerDown {
        button: PointerButton::Primary,
        at,
    }
}

fn release(at: Point) -> InputEvent {
    InputEvent::PointerUp {
        button: PointerButton::Primary,
        at,
    }
}

fn type_text(editor: &mut ColorEditor, text: &str, layout: &ColorEditorLayout) {
    for c in text.chars() {
        assert_eq!(editor.handle_input(InputEvent::Text(c), layout), None);
    }
}

fn focus_field(editor: &mut ColorEditor, field: Field, layout: &ColorEditorLayout) {
    let rect = layout.fields[Field::ALL.iter().position(|&f| f == field).unwrap()];
    editor.handle_input(press(centre(rect)), layout);
    editor.handle_input(release(centre(rect)), layout);
    assert_eq!(editor.focus(), Control::Field(field));
}

// Pickers.

#[test]
fn dragging_the_field_picks_saturation_and_value_clamped_to_its_edges() {
    let layout = layout(false);
    let mut editor = editor();
    let area = layout.sat_val;
    let hue = editor.hsv().h;

    editor.handle_input(press(point(area.x, area.y)), &layout);
    assert_eq!((editor.hsv().s, editor.hsv().v), (0, 255));
    assert_eq!(editor.color(), Rgb::from_hex(0xffffff));
    assert_eq!(editor.focus(), Control::SatVal);

    // Far outside the field, the drag still picks, at its corner.
    editor.handle_input(InputEvent::PointerMove(Some(point(319, 239))), &layout);
    assert_eq!((editor.hsv().s, editor.hsv().v), (255, 0));
    assert_eq!(editor.color(), Rgb::from_hex(0x000000));
    assert_eq!(editor.hsv().h, hue, "black keeps the hue");

    editor.handle_input(release(point(area.right() - 1, area.y)), &layout);
    assert_eq!((editor.hsv().s, editor.hsv().v), (255, 255));
    // The release ended the drag.
    editor.handle_input(
        InputEvent::PointerMove(Some(point(area.x, area.y))),
        &layout,
    );
    assert_eq!((editor.hsv().s, editor.hsv().v), (255, 255));
}

#[test]
fn the_hue_strip_changes_only_the_hue() {
    let layout = layout(false);
    let mut editor = editor();
    let before = editor.hsv();
    editor.handle_input(press(point(layout.hue.x, layout.hue.y)), &layout);
    editor.handle_input(InputEvent::PointerMove(Some(point(0, 0))), &layout);
    assert_eq!(editor.hsv(), Hsv { h: 0, ..before });
    editor.handle_input(
        InputEvent::PointerMove(Some(point(layout.hue.right() + 20, 0))),
        &layout,
    );
    assert_eq!(
        editor.hsv(),
        Hsv {
            h: HUE_STEPS - 1,
            ..before
        }
    );
    assert_eq!(editor.focus(), Control::Hue);
    // The marker follows to the strip's last column.
    assert_eq!(layout.hue_x(editor.hsv().h), layout.hue.right() - 1);
}

#[test]
fn arrows_step_the_focused_picker() {
    let layout = layout(false);
    let mut editor = editor();
    editor.handle_input(press(centre(layout.sat_val)), &layout);
    editor.handle_input(release(centre(layout.sat_val)), &layout);
    let start = editor.hsv();
    editor.handle_input(InputEvent::Right { select: false }, &layout);
    editor.handle_input(InputEvent::Up, &layout);
    assert_eq!(
        (editor.hsv().s, editor.hsv().v),
        (start.s + SV_STEP, start.v + SV_STEP)
    );
    editor.handle_input(InputEvent::Left { select: true }, &layout);
    assert_eq!(editor.hsv().s, start.s + SV_STEP - 4 * SV_STEP);
    assert_eq!(editor.color(), editor.hsv().to_rgb());

    editor.handle_input(InputEvent::Tab, &layout);
    assert_eq!(editor.focus(), Control::Hue);
    let h = editor.hsv().h;
    editor.handle_input(InputEvent::Right { select: false }, &layout);
    assert_eq!(editor.hsv().h, h + HUE_STEP);
}

// Fields.

#[test]
fn typing_rgb_updates_the_color_and_the_other_fields_while_valid() {
    let layout = layout(false);
    let mut editor = editor();
    focus_field(&mut editor, Field::Red, &layout);
    // The first key replaces the value shown.
    type_text(&mut editor, "25", &layout);
    assert_eq!(editor.color(), Rgb { r: 25, ..OLD });
    assert_eq!(editor.field_text(Field::Hex), "#199BCD");
    assert_eq!(
        editor.hsv(),
        Hsv::from_rgb(Rgb { r: 25, ..OLD }, editor.hsv())
    );

    // 256 is no channel: the text stays, the color keeps the last valid.
    type_text(&mut editor, "6", &layout);
    assert_eq!(editor.field_text(Field::Red), "256");
    assert_eq!(editor.color(), Rgb { r: 25, ..OLD });
    // A fourth digit does not fit.
    type_text(&mut editor, "1", &layout);
    assert_eq!(editor.field_text(Field::Red), "256");

    // Enter confirms the field without applying, showing the color.
    assert_eq!(editor.handle_input(InputEvent::Enter, &layout), None);
    assert_eq!(editor.field_text(Field::Red), "25");
}

#[test]
fn hex_takes_six_digits_with_an_optional_hash() {
    let layout = layout(false);
    let mut editor = editor();
    focus_field(&mut editor, Field::Hex, &layout);
    type_text(&mut editor, "#ff80", &layout);
    assert_eq!(editor.color(), OLD, "four digits are no color yet");
    type_text(&mut editor, "0z#1", &layout);
    assert_eq!(editor.field_text(Field::Hex), "#FF8001");
    assert_eq!(editor.color(), Rgb::from_hex(0xff8001));
    assert_eq!(editor.field_text(Field::Green), "128");
}

#[test]
fn escape_restores_the_color_typing_began_from_then_cancels() {
    let layout = layout(false);
    let mut editor = editor();
    focus_field(&mut editor, Field::Blue, &layout);
    type_text(&mut editor, "9", &layout);
    assert_eq!(editor.color(), Rgb { b: 9, ..OLD });
    assert_eq!(editor.handle_input(InputEvent::Break, &layout), None);
    assert_eq!(editor.color(), OLD);
    assert_eq!(editor.field_text(Field::Blue), "205");
    assert_eq!(
        editor.handle_input(InputEvent::Break, &layout),
        Some(ColorOutcome::Cancel)
    );
}

#[test]
fn up_and_down_step_a_channel_field() {
    let layout = layout(false);
    let mut editor = editor();
    focus_field(&mut editor, Field::Green, &layout);
    editor.handle_input(InputEvent::Up, &layout);
    editor.handle_input(InputEvent::Up, &layout);
    editor.handle_input(InputEvent::Down, &layout);
    assert_eq!(editor.color(), Rgb { g: 156, ..OLD });
}

// Closing.

#[test]
fn enter_applies_the_exact_color_unless_cancel_is_focused() {
    let layout = layout(false);
    let mut editor = editor();
    focus_field(&mut editor, Field::Hex, &layout);
    type_text(&mut editor, "123456", &layout);
    editor.handle_input(InputEvent::Enter, &layout);
    assert_eq!(
        editor.handle_input(InputEvent::Enter, &layout),
        Some(ColorOutcome::Apply(Rgb::from_hex(0x123456)))
    );

    let mut editor = ColorEditor::new(7, target(2, 5), uses(155, &[], 0), OLD);
    for _ in 0..6 {
        editor.handle_input(InputEvent::Tab, &layout);
    }
    assert_eq!(editor.focus(), Control::Cancel);
    assert_eq!(
        editor.handle_input(InputEvent::Enter, &layout),
        Some(ColorOutcome::Cancel)
    );
}

#[test]
fn tab_visits_every_control_and_wraps() {
    let layout = layout(false);
    let mut editor = editor();
    let mut seen = vec![editor.focus()];
    for _ in 0..8 {
        editor.handle_input(InputEvent::Tab, &layout);
        seen.push(editor.focus());
    }
    assert_eq!(seen[..8], Control::ORDER);
    assert_eq!(seen[8], Control::SatVal);
}

#[test]
fn a_button_chooses_only_when_pressed_and_released_on_it() {
    let layout = layout(false);
    let mut editor = editor();
    editor.handle_input(press(centre(layout.apply)), &layout);
    assert_eq!(
        editor.handle_input(release(centre(layout.cancel)), &layout),
        None
    );
    editor.handle_input(press(centre(layout.cancel)), &layout);
    editor.handle_input(InputEvent::PointerCancel, &layout);
    assert_eq!(
        editor.handle_input(release(centre(layout.cancel)), &layout),
        None
    );

    editor.handle_input(press(centre(layout.apply)), &layout);
    assert_eq!(
        editor.handle_input(release(centre(layout.apply)), &layout),
        Some(ColorOutcome::Apply(OLD))
    );
}

#[test]
fn editor_shortcuts_do_nothing_inside_the_dialog() {
    let layout = layout(false);
    let mut editor = editor();
    let before = editor.clone();
    for event in [
        InputEvent::Text('p'),
        InputEvent::Undo,
        InputEvent::Save,
        InputEvent::NextView,
        InputEvent::Wheel { delta: 1 },
    ] {
        assert_eq!(editor.handle_input(event, &layout), None);
    }
    assert_eq!(editor, before);
}

// What the dialog tells and draws with.

#[test]
fn the_confirm_button_says_when_other_colors_change_too() {
    assert_eq!(editor().apply_label(), "APPLY");
    let shared = ColorEditor::new(7, target(2, 5), uses(3, &[(2, 9)], 0), OLD);
    assert_eq!(shared.apply_label(), "APPLY TO ALL");
}

#[test]
fn impact_counts_palettes_palette_colors_and_tiles_apart() {
    let shared = uses(27, &[(1, 3), (2, 7), (4, 7), (4, 12)], 14);
    assert_eq!(
        impact_lines(target(2, 5), &shared, false),
        [
            "! Shared: also in 2 other palettes".to_owned(),
            "  (4 palette colors), 14 other tiles.".to_owned(),
            "Game code may use it too.".to_owned(),
        ]
    );
    assert_eq!(
        impact_lines(target(2, 5), &shared, true),
        [
            "! Master $1B: 4 more entries, 2 banks".to_owned(),
            "  OBP1 L3, OBP2 L7, OBP4 L7, OBP4 LC".to_owned(),
            "14 other tiles; code using 27 too.".to_owned(),
        ]
    );

    assert_eq!(
        impact_lines(target(2, 5), &uses(40, &[(6, 1)], 0), false)[1],
        "  (1 palette color), no other tiles."
    );

    let same_bank = uses(40, &[(2, 9)], 1);
    assert_eq!(
        impact_lines(target(2, 5), &same_bank, false),
        [
            "! Shared with 1 other color of".to_owned(),
            "  this palette; 1 other tile.".to_owned(),
            "Game code may use it too.".to_owned(),
        ]
    );
}

#[test]
fn an_unshared_color_warns_only_about_what_the_document_cannot_see() {
    assert_eq!(
        impact_lines(target(2, 5), &uses(155, &[], 6), false),
        [
            "Also shows in 6 other tiles.".to_owned(),
            String::new(),
            String::new()
        ]
    );
    assert_eq!(
        impact_lines(target(2, 5), &uses(155, &[], 0), true),
        [
            String::new(),
            "Master $9B (155) is not shared.".to_owned(),
            String::new()
        ]
    );
    // The default palette's first colors are the likeliest in code.
    assert_eq!(
        impact_lines(target(2, 5), &uses(12, &[], 0), false)[2],
        "Game code may use it too."
    );
    // Every frame starts cleared to master 0.
    assert_eq!(
        impact_lines(target(4, 12), &uses(0, &[], 0), false)[2],
        "Game background; code may use it too."
    );
}

#[test]
fn long_lines_are_cut_to_the_window() {
    assert_eq!(fit("abcdef", 6), "abcdef");
    assert_eq!(fit("abcdefg", 6), "abcd..");
}

#[test]
fn scratch_colors_hold_the_field_at_the_hue_the_strip_and_the_new_color() {
    let layout = layout(false);
    let mut editor = editor();
    // Pure red's hue.
    editor.handle_input(press(point(layout.hue.x, layout.hue.y)), &layout);
    let colors = editor.scratch_colors();
    // Top right: most saturated and brightest; bottom left: darkest grey.
    let top_right = colors[SV_COLUMNS - 1];
    assert!(
        top_right.r > 230 && top_right.g < 25 && top_right.b < 25,
        "{top_right:?}"
    );
    let bottom_left = colors[(SV_ROWS - 1) * SV_COLUMNS];
    assert!(
        [bottom_left.r, bottom_left.g, bottom_left.b]
            .iter()
            .all(|&c| c < 25),
        "{bottom_left:?}"
    );
    // The strip runs red, yellow, green ... back toward red.
    let strip = &colors[SV_COLUMNS * SV_ROWS..SCRATCH_COLORS - 1];
    assert_eq!(strip.len(), HUE_CELLS);
    assert!(strip[0].r == 255 && strip[0].b == 0);
    assert!(strip[HUE_CELLS / 3].g == 255);
    assert_eq!(colors[SCRATCH_COLORS - 1], editor.color());
    assert_eq!(usize::from(NEW_SLOT), SCRATCH_BASE + SCRATCH_COLORS - 1);
}

#[test]
fn the_layout_fits_the_area_without_overlapping_parts() {
    for advanced in [false, true] {
        let layout = layout(advanced);
        let window = layout.window;
        let area = area();
        assert!(window.x >= area.x && window.right() + SHADOW_OFFSET <= area.right());
        assert!(window.y >= area.y && window.bottom() + SHADOW_OFFSET <= area.bottom());
        let mut parts = vec![
            layout.sat_val.inset(-2),
            layout.preview.inset(-1),
            layout.swatches,
            layout.hue_target(),
            layout.cancel,
            layout.apply,
        ];
        parts.extend(layout.fields);
        for (n, a) in parts.iter().enumerate() {
            assert!(window.inset(1).contains_xy(a.x, a.y), "{a:?}");
            assert!(
                window.inset(1).contains_xy(a.right() - 1, a.bottom() - 1),
                "{a:?}"
            );
            for b in &parts[n + 1..] {
                assert!(!a.overlaps(*b), "{a:?} overlaps {b:?}");
            }
        }
        let last_impact = layout.impact_y + IMPACT_LINES as i32 * METRICS.cell_h;
        assert!(layout.fields[0].bottom() <= layout.impact_y);
        assert!(last_impact <= layout.apply.y);
        assert!(layout.columns() >= 38, "the impact note needs its columns");
    }
}
