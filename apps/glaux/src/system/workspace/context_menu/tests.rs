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

fn long_menu() -> ContextMenu<usize> {
    let entries = (0..20)
        .map(|n| MenuEntry::item(&format!("Item {n}"), None, n))
        .collect();
    ContextMenu::new(entries, Point::new(10, 30)).limited_to(5 * 14 + 2)
}

#[test]
fn a_limited_menu_scrolls_to_keep_the_highlight_in_view() {
    let area = AREA;
    let layout = |menu: &ContextMenu<usize>| menu.layout(&LAYOUT_FONT, area, 6, 12);
    let mut menu = long_menu();
    let first = layout(&menu);
    assert_eq!((first.first, first.rows.len()), (0, 5));
    assert_eq!(first.window.height, 5 * 14 + 2);

    let down = InputEvent::Down { select: false };
    for _ in 0..7 {
        let current = layout(&menu);
        menu.handle_input(down, &current);
    }
    let scrolled = layout(&menu);
    assert_eq!(menu.focus(), Some(6));
    assert_eq!(scrolled.first, 2, "the highlighted entry is the last shown");
    assert_eq!(
        scrolled.entry_at(centre(scrolled.rows[4])),
        Some(6),
        "rows report the entry they stand for"
    );
}

#[test]
fn the_wheel_scrolls_and_drops_a_highlight_that_leaves_the_view() {
    let mut menu = long_menu().focused_on(1);
    let layout = menu.layout(&LAYOUT_FONT, AREA, 6, 12);
    menu.handle_input(InputEvent::Wheel { delta: -3 }, &layout);
    let after = menu.layout(&LAYOUT_FONT, AREA, 6, 12);
    assert_eq!(after.first, 3);
    assert_eq!(menu.focus(), None);

    menu.handle_input(InputEvent::Wheel { delta: -100 }, &after);
    let end = menu.layout(&LAYOUT_FONT, AREA, 6, 12);
    assert_eq!(end.first + end.rows.len(), 20, "stops at the last entry");
}

#[test]
fn a_focused_current_item_is_not_inked_with_the_accent_on_the_highlight() {
    use crate::system::screen::ACCENT;
    use crate::system::workspace::{HOVER_SHADE, RenderContext};
    use glaux_machine::framebuffer::Framebuffer;

    let rom = crate::rom::load().unwrap();
    let mut menu = ContextMenu::new(
        vec![
            MenuEntry::item("Town", None, 0usize).styled(ItemStyle::Current),
            MenuEntry::item("Cave", None, 1usize),
        ],
        Point::new(10, 30),
    )
    .focused_on(0);
    let layout = menu.layout(rom.font(), AREA, 6, 12);
    let row = layout.rows[0];
    let count = |menu: &ContextMenu<usize>, color: u8| {
        let mut fb = Framebuffer::new();
        menu.render(&mut RenderContext::new(&mut fb, rom.font(), rom.ui()), &layout);
        (row.y..row.bottom())
            .flat_map(|y| (row.x..row.right()).map(move |x| (x, y)))
            .filter(|&(x, y)| fb.pixel(x, y) == Some(color))
            .count()
    };
    assert!(count(&menu, HOVER_SHADE) > 0, "the row is highlighted");
    assert_eq!(count(&menu, ACCENT), 0);
    menu.handle_input(InputEvent::Down { select: false }, &layout);
    assert!(count(&menu, ACCENT) > 0, "off the highlight it keeps the accent");
}

#[test]
fn headers_cannot_be_chosen_and_checks_do_not_change_what_runs() {
    let mut menu = ContextMenu::new(
        vec![
            MenuEntry::header("BG2  Decor"),
            MenuEntry::item("Visible", Some("V"), 1usize).checked(true),
            MenuEntry::item("Locked", Some("L"), 2usize),
        ],
        Point::new(10, 30),
    );
    let layout = menu.layout(&LAYOUT_FONT, AREA, 6, 12);
    menu.handle_input(InputEvent::Down { select: false }, &layout);
    assert_eq!(menu.focus(), Some(1), "the header is skipped");
    assert_eq!(
        menu.handle_input(InputEvent::Enter, &layout),
        MenuOutcome::Run(1)
    );
    let header = centre(layout.rows[0]);
    assert_eq!(
        menu.handle_input(press(PointerButton::Primary, header), &layout),
        MenuOutcome::Stay
    );
}
