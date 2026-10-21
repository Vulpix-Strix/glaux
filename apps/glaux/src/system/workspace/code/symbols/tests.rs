use glaux_input::Point;
use pretty_assertions::assert_eq;

use super::*;
use crate::cartridge::source::SourcePath;

fn layout() -> PanelLayout {
    let code = CodeLayout::new(
        crate::system::workspace::tests_support::CODE_METRICS,
        crate::system::test_font::LAYOUT_FONT.clone(),
    );
    PanelLayout::new(&code)
}

fn row(name: &str, start: usize, line: usize) -> Row {
    Row {
        symbol: Symbol {
            name: name.to_owned(),
            kind: SymbolKind::Function,
            declared: start..start + name.len(),
            depth: 0,
        },
        line,
    }
}

fn panel(count: usize) -> SymbolPanel {
    let mut panel = SymbolPanel::new();
    let rows = (0..count)
        .map(|n| row(&format!("f{n}"), n * 10, n))
        .collect();
    panel.set_rows(None, rows);
    panel
}

fn press(panel: &mut SymbolPanel, event: InputEvent) -> PanelAction {
    panel.handle_input(event, &layout(), None)
}

fn primary(at: Point) -> InputEvent {
    InputEvent::PointerDown {
        button: PointerButton::Primary,
        at,
        select: false,
    }
}

fn point_in(rect: crate::system::workspace::Rect) -> Point {
    Point::new(rect.x as u16 + 4, rect.y as u16 + 1)
}

#[test]
fn the_highlight_follows_the_caret_while_the_text_has_the_keys() {
    let mut panel = panel(5);
    panel.focused = false;
    let visible = layout().visible;
    for (caret, expected) in [(0, 0), (25, 2), (30, 3), (9, 0), (500, 4)] {
        panel.follow(caret, visible);
        assert_eq!(panel.selected(), expected, "caret {caret}");
    }
}

#[test]
fn a_caret_before_the_first_declaration_highlights_the_first() {
    let mut panel = SymbolPanel::new();
    panel.set_rows(None, vec![row("late", 20, 5)]);
    panel.focused = false;
    panel.follow(3, layout().visible);
    assert_eq!(panel.selected(), 0);
}

#[test]
fn following_scrolls_the_highlight_into_view() {
    let visible = layout().visible;
    let mut panel = panel(visible * 3);
    panel.focused = false;
    panel.follow((visible * 3 - 1) * 10, visible);
    assert_eq!(panel.scroll() + visible, visible * 3);
    panel.follow(0, visible);
    assert_eq!(panel.scroll(), 0);
}

#[test]
fn the_caret_does_not_pull_the_highlight_away_while_the_panel_has_the_keys() {
    let mut panel = panel(5);
    press(&mut panel, InputEvent::Down { select: false });
    panel.follow(0, layout().visible);
    assert_eq!(panel.selected(), 1);
}

#[test]
fn arrows_move_within_the_list_and_enter_goes() {
    let mut panel = panel(3);
    for _ in 0..5 {
        press(&mut panel, InputEvent::Down { select: false });
    }
    assert_eq!(panel.selected(), 2);
    assert_eq!(
        press(&mut panel, InputEvent::Up { select: false }),
        PanelAction::Handled
    );
    assert_eq!(press(&mut panel, InputEvent::Enter), PanelAction::Go(1));
    press(&mut panel, InputEvent::DocumentStart { select: false });
    assert_eq!(panel.selected(), 0);
    press(&mut panel, InputEvent::DocumentEnd { select: false });
    assert_eq!(panel.selected(), 2);
}

#[test]
fn nothing_typed_reaches_the_document_while_the_panel_has_the_keys() {
    let mut panel = panel(2);
    for event in [
        InputEvent::Text('x'),
        InputEvent::Backspace,
        InputEvent::Delete,
        InputEvent::Left { select: false },
    ] {
        assert_eq!(press(&mut panel, event), PanelAction::Handled, "{event:?}");
    }
    // Commands that are not about typing still pass.
    assert_eq!(press(&mut panel, InputEvent::Save), PanelAction::Ignored);
}

#[test]
fn escape_gives_the_keys_back_and_then_events_pass_through() {
    let mut panel = panel(2);
    assert_eq!(press(&mut panel, InputEvent::Break), PanelAction::Handled);
    assert!(!panel.focused());
    assert_eq!(
        press(&mut panel, InputEvent::Text('x')),
        PanelAction::Ignored
    );
    assert_eq!(
        press(&mut panel, InputEvent::Down { select: false }),
        PanelAction::Ignored
    );
}

#[test]
fn clicking_a_row_goes_to_it_counting_from_the_scroll() {
    let layout = layout();
    let mut panel = panel(layout.visible * 2);
    panel.focused = false;
    panel.scroll = 3;
    let action = panel.handle_input(primary(point_in(layout.row(2))), &layout, None);
    assert_eq!(action, PanelAction::Go(5));
    assert_eq!(panel.selected(), 5);
}

#[test]
fn clicking_the_title_takes_the_keys_and_clicking_the_text_gives_them_up() {
    let layout = layout();
    let mut panel = panel(2);
    panel.focused = false;
    let action = panel.handle_input(primary(point_in(layout.title)), &layout, None);
    assert_eq!(action, PanelAction::Handled);
    assert!(panel.focused());
    let outside = Point::new(10, layout.area.y as u16 + 20);
    assert_eq!(
        panel.handle_input(primary(outside), &layout, None),
        PanelAction::Ignored
    );
    assert!(!panel.focused());
}

#[test]
fn a_click_below_the_last_row_goes_nowhere() {
    let layout = layout();
    let mut panel = panel(1);
    panel.focused = false;
    let below = point_in(layout.row(3));
    assert_eq!(
        panel.handle_input(primary(below), &layout, None),
        PanelAction::Handled
    );
}

#[test]
fn the_wheel_scrolls_only_over_the_panel_and_stays_in_range() {
    let layout = layout();
    let mut panel = panel(layout.visible + 4);
    let over = Some(point_in(layout.row(0)));
    for _ in 0..10 {
        panel.handle_input(InputEvent::Wheel { delta: -1 }, &layout, over);
    }
    assert_eq!(panel.scroll(), 4);
    panel.handle_input(
        InputEvent::Wheel { delta: 1 },
        &layout,
        Some(Point::new(5, 5)),
    );
    assert_eq!(panel.scroll(), 4);
    panel.handle_input(InputEvent::Wheel { delta: 3 }, &layout, over);
    assert_eq!(panel.scroll(), 0);
}

#[test]
fn a_row_targets_its_declaration_in_the_given_file() {
    let panel = panel(3);
    let path = SourcePath::parse("code/main.luau").unwrap();
    let target = panel.target(2, &path).unwrap();
    assert_eq!(target.path, path);
    assert_eq!(target.at, Spot::Offset(20));
    assert_eq!(panel.target(3, &path), None);
}

#[test]
fn rows_changing_under_the_selection_keep_it_in_range() {
    let mut panel = panel(5);
    press(&mut panel, InputEvent::DocumentEnd { select: false });
    panel.set_rows(None, vec![row("only", 0, 0)]);
    assert_eq!(panel.selected(), 0);
    panel.set_rows(None, Vec::new());
    assert_eq!(panel.selected(), 0);
}
