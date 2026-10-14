use super::*;
use crate::system::test_font::LAYOUT_FONT;
use pretty_assertions::assert_eq;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Undo,
    Cut,
    Paste,
}

const AREA: Rect = Rect {
    x: 0,
    y: 16,
    width: 320,
    height: 200,
};

fn menu(at: Point) -> ContextMenu<Action> {
    ContextMenu::new(
        vec![
            MenuEntry::when(false, "Undo", Some("Ctrl+Z"), Action::Undo),
            MenuEntry::Separator,
            MenuEntry::item("Cut", Some("Ctrl+X"), Action::Cut),
            MenuEntry::when(false, "Copy", Some("Ctrl+C"), Action::Cut),
            MenuEntry::item("Paste", Some("Ctrl+V"), Action::Paste),
        ],
        at,
    )
}

fn layout(menu: &ContextMenu<Action>) -> MenuLayout {
    menu.layout(&LAYOUT_FONT, AREA, 6, 12)
}

fn centre(rect: Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

fn press(button: PointerButton, at: Point) -> InputEvent {
    InputEvent::PointerDown {
        button,
        at,
        select: false,
    }
}

#[test]
fn keys_skip_separators_and_disabled_items_and_wrap() {
    let mut menu = menu(Point::new(10, 30));
    let layout = layout(&menu);
    let down = InputEvent::Down { select: false };
    let up = InputEvent::Up { select: false };
    assert_eq!(
        menu.handle_input(InputEvent::Enter, &layout),
        MenuOutcome::Stay
    );
    menu.handle_input(down, &layout);
    assert_eq!(menu.focus(), Some(2));
    menu.handle_input(down, &layout);
    assert_eq!(menu.focus(), Some(4), "Copy is disabled");
    menu.handle_input(down, &layout);
    assert_eq!(menu.focus(), Some(2), "wraps past disabled Undo");
    menu.handle_input(up, &layout);
    assert_eq!(
        menu.handle_input(InputEvent::Enter, &layout),
        MenuOutcome::Run(Action::Paste)
    );
}

#[test]
fn only_enabled_items_can_be_clicked() {
    let mut menu = menu(Point::new(10, 30));
    let layout = layout(&menu);
    let primary = |row: usize| press(PointerButton::Primary, centre(layout.rows[row]));
    assert_eq!(menu.handle_input(primary(0), &layout), MenuOutcome::Stay);
    assert_eq!(menu.handle_input(primary(1), &layout), MenuOutcome::Stay);
    assert_eq!(menu.handle_input(primary(3), &layout), MenuOutcome::Stay);
    assert!(!menu.chooses_at(centre(layout.rows[3]), &layout));
    assert!(menu.chooses_at(centre(layout.rows[2]), &layout));
    assert_eq!(
        menu.handle_input(primary(2), &layout),
        MenuOutcome::Run(Action::Cut)
    );
}

#[test]
fn the_pointer_highlights_only_enabled_items() {
    let mut menu = menu(Point::new(10, 30));
    let layout = layout(&menu);
    menu.handle_input(
        InputEvent::PointerMove(Some(centre(layout.rows[4]))),
        &layout,
    );
    assert_eq!(menu.focus(), Some(4));
    menu.handle_input(
        InputEvent::PointerMove(Some(centre(layout.rows[0]))),
        &layout,
    );
    assert_eq!(menu.focus(), None);
}

#[test]
fn escape_and_presses_outside_close_it() {
    let mut menu = menu(Point::new(10, 30));
    let layout = layout(&menu);
    assert_eq!(
        menu.handle_input(InputEvent::Break, &layout),
        MenuOutcome::Close
    );
    for button in [PointerButton::Primary, PointerButton::Secondary] {
        assert_eq!(
            menu.handle_input(press(button, Point::new(300, 200)), &layout),
            MenuOutcome::Outside
        );
    }
}

#[test]
fn near_the_areas_edges_the_menu_moves_back_inside() {
    let menu = menu(Point::new(319, 215));
    let layout = layout(&menu);
    assert!(layout.window.right() <= AREA.right());
    assert!(layout.window.bottom() <= AREA.bottom());
    let at = Point::new(20, 40);
    let free = ContextMenu::new(menu.entries().to_vec(), at).layout(&LAYOUT_FONT, AREA, 6, 12);
    assert_eq!(
        (free.window.x, free.window.y),
        (20, 40),
        "opens at the anchor"
    );
    // Rows stack without overlapping, inside the window.
    for pair in layout.rows.windows(2) {
        assert_eq!(pair[0].bottom(), pair[1].y);
    }
    assert!(layout.rows.last().unwrap().bottom() < layout.window.bottom());
}
