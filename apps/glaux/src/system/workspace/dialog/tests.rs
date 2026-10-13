use super::*;
use crate::system::test_font::LAYOUT_FONT;
use glaux_machine::font::FontMetrics;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use pretty_assertions::assert_eq;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Yes,
    No,
    Cancel,
}

const CELL_W: i32 = FontMetrics::SYSTEM.cell_width() as i32;
const CELL_H: i32 = FontMetrics::SYSTEM.cell_height() as i32;

/// The workspace's area below a 16-pixel tab bar on the 320×240 screen.
const AREA: Rect = Rect {
    x: 0,
    y: 16,
    width: WIDTH as i32,
    height: HEIGHT as i32 - 16,
};

fn dialog() -> Dialog<Choice> {
    Dialog::new(
        "QUESTION",
        vec![
            "A first paragraph that is long enough to need wrapping at the window's width.".into(),
            "A second one.".into(),
        ],
        vec![
            ("YES", Choice::Yes),
            ("NO", Choice::No),
            ("CANCEL", Choice::Cancel),
        ],
        2,
        Some(Choice::Cancel),
    )
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
fn focus_moves_both_ways_and_wraps() {
    let mut dialog = dialog();
    let layout = dialog.layout(&LAYOUT_FONT, AREA, CELL_W, CELL_H);
    assert_eq!(dialog.focus(), 2);
    for (event, focus) in [
        (InputEvent::Down { select: false }, 0),
        (InputEvent::Tab, 1),
        (InputEvent::Right { select: false }, 2),
        (InputEvent::Up { select: false }, 1),
        (InputEvent::Left { select: false }, 0),
        (InputEvent::Up { select: false }, 2),
    ] {
        assert_eq!(dialog.handle_input(event, &layout), None);
        assert_eq!(dialog.focus(), focus, "{event:?}");
    }
}

#[test]
fn enter_chooses_the_focused_action_and_escape_cancels() {
    let mut dialog = dialog();
    let layout = dialog.layout(&LAYOUT_FONT, AREA, CELL_W, CELL_H);
    assert_eq!(
        dialog.handle_input(InputEvent::Enter, &layout),
        Some(Choice::Cancel)
    );
    dialog.handle_input(InputEvent::Down { select: false }, &layout);
    assert_eq!(
        dialog.handle_input(InputEvent::Enter, &layout),
        Some(Choice::Yes)
    );
    assert_eq!(
        dialog.handle_input(InputEvent::Break, &layout),
        Some(Choice::Cancel)
    );

    let mut plain = Dialog::new("X", vec![], vec![("OK", Choice::Yes)], 0, None);
    let layout = plain.layout(&LAYOUT_FONT, AREA, CELL_W, CELL_H);
    assert_eq!(plain.handle_input(InputEvent::Break, &layout), None);
}

#[test]
fn a_click_chooses_the_button_released_on() {
    let mut dialog = dialog();
    let layout = dialog.layout(&LAYOUT_FONT, AREA, CELL_W, CELL_H);
    let no = centre(layout.buttons[1]);
    assert_eq!(dialog.handle_input(press(no), &layout), None);
    assert_eq!(dialog.focus(), 1, "a press focuses its button");
    assert_eq!(dialog.handle_input(release(no), &layout), Some(Choice::No));
}

#[test]
fn releasing_elsewhere_or_a_cancelled_press_chooses_nothing() {
    let mut dialog = dialog();
    let layout = dialog.layout(&LAYOUT_FONT, AREA, CELL_W, CELL_H);
    let yes = centre(layout.buttons[0]);
    let no = centre(layout.buttons[1]);
    dialog.handle_input(press(yes), &layout);
    assert_eq!(dialog.handle_input(release(no), &layout), None);

    dialog.handle_input(press(yes), &layout);
    dialog.handle_input(InputEvent::PointerCancel, &layout);
    assert_eq!(dialog.handle_input(release(yes), &layout), None);

    let outside = Point::new(1, 20);
    dialog.handle_input(press(outside), &layout);
    assert_eq!(dialog.handle_input(release(outside), &layout), None);
    assert_eq!(dialog.focus(), 0, "presses outside keep the focus");
}

#[test]
fn other_input_is_swallowed() {
    let mut dialog = dialog();
    let layout = dialog.layout(&LAYOUT_FONT, AREA, CELL_W, CELL_H);
    for event in [
        InputEvent::Text('y'),
        InputEvent::NextView,
        InputEvent::Save,
        InputEvent::Undo,
        InputEvent::Wheel { delta: 1 },
    ] {
        assert_eq!(dialog.handle_input(event, &layout), None);
        assert_eq!(dialog.focus(), 2);
    }
}

#[test]
fn the_window_fits_the_workspace_and_its_text_fits_the_window() {
    let dialog = dialog();
    let layout = dialog.layout(&LAYOUT_FONT, AREA, CELL_W, CELL_H);
    let window = layout.window;
    assert!(window.x >= AREA.x && window.y >= AREA.y, "{window:?}");
    assert!(window.right() + SHADOW_OFFSET <= AREA.right(), "{window:?}");
    assert!(
        window.bottom() + SHADOW_OFFSET <= AREA.bottom(),
        "{window:?}"
    );
    let body = window.inset(1 + PAD);
    assert!(layout.lines.len() > 3, "the long paragraph wraps");
    for (line, y) in &layout.lines {
        assert!(
            layout.text_x + text_cells(line) * CELL_W <= body.right(),
            "{line:?}"
        );
        assert!(*y >= layout.title_bar.bottom() && y + CELL_H <= body.bottom());
    }
    for (button, next) in layout.buttons.iter().zip(&layout.buttons[1..]) {
        assert!(button.bottom() < next.y);
    }
    for button in &layout.buttons {
        assert!(button.right() <= body.right() && button.bottom() <= body.bottom());
    }
    assert!(
        (window.x + window.width / 2 - (AREA.x + AREA.width / 2)).abs() <= SHADOW_OFFSET,
        "centred"
    );
}

#[test]
fn the_focused_button_is_drawn_inverted() {
    let rom = crate::rom::load().unwrap();
    let dialog = dialog();
    let layout = dialog.layout(&LAYOUT_FONT, AREA, CELL_W, CELL_H);
    let mut fb = Framebuffer::new();
    dialog.render(&mut fb, rom.font(), &layout);
    let face = |rect: Rect| fb.pixel(rect.x + 1, rect.y + 1);
    assert_eq!(face(layout.buttons[2]), Some(INK), "focused");
    assert_eq!(face(layout.buttons[0]), Some(BODY));
    assert_eq!(fb.pixel(layout.title_bar.x, layout.title_bar.y), Some(INK));
    let shadow = layout.window;
    assert_eq!(
        fb.pixel(shadow.right(), shadow.bottom()),
        Some(SHADOW),
        "the shadow sits just past the bottom-right corner"
    );
}
