//! The Docs button in the tab bar, and how the workspace hands the screen to
//! the documentation browser and takes it back.

use glaux_machine::system_rom::SystemCursor;

use super::*;
use crate::system::workspace::tests_support::{
    docs_button, docs_open, docs_pages, english_labels, tab_rect,
};
use pretty_assertions::assert_eq;

fn workspace() -> Workspace {
    Workspace::new(&LAYOUT_FONT, Palette::default())
}

fn centre(rect: Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

fn send(workspace: &mut Workspace, event: InputEvent) -> Option<WorkspaceRequest> {
    workspace.handle_input(event, None, None)
}

fn press(button: PointerButton, at: Point) -> InputEvent {
    InputEvent::PointerDown {
        button,
        at,
        select: false,
    }
}

#[test]
fn the_button_ends_the_bar_inside_the_screen_and_beyond_the_tabs() {
    let button = docs_button(&LAYOUT_FONT);
    let layout = layout();
    assert_eq!(button.y, 0);
    assert_eq!(button.height, layout.bar_height());
    assert_eq!(button.right(), WIDTH as i32 - DOCS_MARGIN);
    let [.., (_, last)] = layout.tabs();
    assert!(button.x > last.right());
    // The label fits its plate with the padding on both sides.
    let label = text_width(&LAYOUT_FONT, english_labels().docs());
    assert_eq!(button.width, label + 2 * DOCS_PAD);
}

#[test]
fn no_hint_reaches_the_button_and_the_name_stops_short_of_it() {
    let labels = english_labels();
    let layout = layout();
    let button = docs_button(&LAYOUT_FONT);
    // Every hint, the Docs one included, ends before the name area starts.
    let longest = labels.longest_cells() as i32 * layout.cell_w;
    let name_start = layout.hint_x() + longest + NAME_GAP;
    assert!(name_start < button.x, "{name_start} {}", button.x);
    for text in [
        labels.docs_hint(),
        labels.get(Tab::Code),
        labels.get(Tab::Sprites),
    ] {
        let end = layout.hint_x() + text_width(&LAYOUT_FONT, text);
        assert!(end < name_start, "{text}");
    }
    // The name ends left of the button with a gap, and has room for text.
    let right = layout.name_right(&labels, &LAYOUT_FONT);
    assert_eq!(right, button.x - NAME_PAD);
    let cells = layout.name_cells(&labels, &LAYOUT_FONT);
    assert!(cells >= MIN_NAME_CELLS, "{cells}");
    assert!(right - cells as i32 * layout.cell_w >= name_start);
}

#[test]
fn the_button_opens_the_browser_and_a_second_press_closes_it() {
    let mut workspace = workspace();
    let at = centre(docs_button(&LAYOUT_FONT));
    assert!(!docs_open(&workspace));
    assert_eq!(
        send(&mut workspace, press(PointerButton::Primary, at)),
        None
    );
    assert!(docs_open(&workspace));
    assert_eq!(docs_pages(&workspace).0, 1);
    assert_eq!(
        send(&mut workspace, press(PointerButton::Primary, at)),
        None
    );
    assert!(!docs_open(&workspace));
    assert_eq!(workspace.tab, Tab::Code);
}

#[test]
fn only_the_primary_button_presses_it() {
    let mut workspace = workspace();
    let at = centre(docs_button(&LAYOUT_FONT));
    for button in [PointerButton::Secondary, PointerButton::Middle] {
        send(&mut workspace, press(button, at));
        assert!(!docs_open(&workspace), "{button:?}");
    }
}

#[test]
fn escape_stays_in_the_workspace_while_the_browser_is_open() {
    let mut workspace = workspace();
    assert!(!workspace.claims_break());
    send(&mut workspace, InputEvent::Docs);
    assert!(workspace.claims_break());
    send(&mut workspace, InputEvent::Break);
    assert!(!docs_open(&workspace));
    assert!(!workspace.claims_break());
}

#[test]
fn the_pointer_is_a_hand_over_the_button_and_the_tabs_even_with_the_browser_open() {
    let mut workspace = workspace();
    let button = centre(docs_button(&LAYOUT_FONT));
    let tab = centre(tab_rect(Tab::Sprites, FontMetrics::SYSTEM));
    for open in [false, true] {
        if open {
            send(&mut workspace, InputEvent::Docs);
        }
        let over = |at| workspace.cursor(at, None).0;
        assert_eq!(over(button), SystemCursor::Hand, "open: {open}");
        assert_eq!(over(tab), SystemCursor::Hand, "open: {open}");
    }
    // Over the page, the browser decides.
    let page = Point::new(10, 300);
    assert_eq!(
        workspace.cursor(page, None).0,
        workspace.docs.cursor(page, docs_source())
    );
}

#[test]
fn a_dialog_takes_the_button_pointer_away() {
    let mut workspace = workspace();
    send(&mut workspace, InputEvent::Docs);
    workspace.open_new_script(Language::Luau);
    assert!(workspace.has_modal());
    let button = centre(docs_button(&LAYOUT_FONT));
    assert_eq!(workspace.cursor(button, None).0, SystemCursor::Arrow);
}

#[test]
fn pasting_goes_to_the_open_search_and_not_to_the_script() {
    let mut workspace = workspace();
    let mut session = CodeSession::default();
    send(&mut workspace, InputEvent::Docs);
    assert_eq!(send(&mut workspace, InputEvent::Find), None);
    assert_eq!(
        send(&mut workspace, InputEvent::Paste),
        Some(WorkspaceRequest::Host(HostWork::Paste))
    );
    workspace.paste(&mut session, Some("game".to_owned()));
    assert!(workspace.docs.search_hits().unwrap() > 0);
    assert!(!session.is_modified());
}
