use std::fs;

use super::*;
use crate::cartridge::LoadedCartridge;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use crate::system::code_document::DocumentId;
use glaux_machine::font::FontMetrics;
use pretty_assertions::assert_eq;

fn editor() -> CodeEditor {
    let metrics = FontMetrics::SYSTEM;
    CodeEditor::new(Metrics {
        cell_w: i32::from(metrics.cell_width()),
        cell_h: i32::from(metrics.cell_height()),
        top: 16,
    })
}

/// A cartridge holding `names` in `code/`, each holding `lines` numbered
/// lines, and a session with all of them open in that order, the last
/// active.
fn session_with(fixture: &Fixture, names: &[&str], lines: usize) -> (LoadedCartridge, CodeSession) {
    let dir = fixture.root().join("games/pong");
    write_cartridge(&dir, "Pong");
    let text: String = (1..=lines).map(|n| format!("line {n}\n")).collect();
    for name in names {
        fs::write(dir.join("code").join(name), &text).unwrap();
    }
    let cartridge = LoadedCartridge::load(&fixture.storage(), "games/pong").unwrap();
    let mut session = CodeSession::default();
    for name in names {
        let path = SourcePath::parse(&format!("code/{name}")).unwrap();
        session.open(&cartridge, &path).unwrap();
    }
    (cartridge, session)
}

fn names(session: &CodeSession) -> Vec<&str> {
    session
        .tabs()
        .ids()
        .map(|id| session.document(id).unwrap().path().file_name())
        .collect()
}

fn active_name(session: &CodeSession) -> Option<&str> {
    session
        .active_document()
        .map(|document| document.path().file_name())
}

fn id_of(session: &CodeSession, name: &str) -> DocumentId {
    session
        .tabs()
        .ids()
        .find(|&id| session.document(id).unwrap().path().file_name() == name)
        .unwrap()
}

fn centre(rect: Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

fn press(button: PointerButton, at: Point) -> InputEvent {
    InputEvent::PointerDown { button, at }
}

fn release(at: Point) -> InputEvent {
    InputEvent::PointerUp {
        button: PointerButton::Primary,
        at,
    }
}

/// Where the strip draws each tab, in tab order from its scroll.
fn tab_rects(editor: &CodeEditor, session: &CodeSession) -> Vec<layout::StripTab> {
    editor
        .layout()
        .strip_tabs(&labels(session), session.tabs().scroll())
}

#[test]
fn clicking_a_tab_activates_it() {
    let fixture = Fixture::new("code-ui-click");
    let (_, mut session) = session_with(&fixture, &["a.luau", "b.luau", "c.luau"], 1);
    let mut editor = editor();
    let first = tab_rects(&editor, &session)[0];
    let at = Point::new((first.rect.x + 2) as u16, (first.rect.y + 2) as u16);
    editor.handle_input(press(PointerButton::Primary, at), Some(&mut session));
    editor.handle_input(release(at), Some(&mut session));
    assert_eq!(active_name(&session), Some("a.luau"));
    assert_eq!(names(&session), ["a.luau", "b.luau", "c.luau"]);
}

#[test]
fn dragging_a_tab_along_the_strip_reorders_it() {
    let fixture = Fixture::new("code-ui-drag");
    let (_, mut session) = session_with(&fixture, &["a.luau", "b.luau", "c.luau"], 1);
    let mut editor = editor();
    let rects = tab_rects(&editor, &session);
    let grab = Point::new((rects[0].rect.x + 2) as u16, (rects[0].rect.y + 2) as u16);
    editor.handle_input(press(PointerButton::Primary, grab), Some(&mut session));
    for tab in [rects[1], rects[2]] {
        let at = Point::new((tab.rect.x + 2) as u16, grab.y);
        editor.handle_input(InputEvent::PointerMove(Some(at)), Some(&mut session));
    }
    editor.handle_input(release(grab), Some(&mut session));
    assert_eq!(names(&session), ["b.luau", "c.luau", "a.luau"]);
    assert_eq!(active_name(&session), Some("a.luau"));

    // Once released, moving the pointer reorders nothing.
    let at = centre(rects[0].rect);
    editor.handle_input(InputEvent::PointerMove(Some(at)), Some(&mut session));
    assert_eq!(names(&session), ["b.luau", "c.luau", "a.luau"]);
}

#[test]
fn a_cancelled_drag_stops_reordering() {
    let fixture = Fixture::new("code-ui-drag-cancel");
    let (_, mut session) = session_with(&fixture, &["a.luau", "b.luau"], 1);
    let mut editor = editor();
    let rects = tab_rects(&editor, &session);
    editor.handle_input(
        press(PointerButton::Primary, centre(rects[0].rect)),
        Some(&mut session),
    );
    editor.handle_input(InputEvent::PointerCancel, Some(&mut session));
    editor.handle_input(
        InputEvent::PointerMove(Some(centre(rects[1].rect))),
        Some(&mut session),
    );
    assert_eq!(names(&session), ["a.luau", "b.luau"]);
}

#[test]
fn the_close_glyph_and_a_middle_click_close_tabs() {
    let fixture = Fixture::new("code-ui-close");
    let (_, mut session) = session_with(&fixture, &["a.luau", "b.luau", "c.luau"], 1);
    let mut editor = editor();
    let rects = tab_rects(&editor, &session);
    editor.handle_input(
        press(PointerButton::Primary, centre(rects[2].close)),
        Some(&mut session),
    );
    assert_eq!(names(&session), ["a.luau", "b.luau"]);
    assert_eq!(active_name(&session), Some("b.luau"));

    let rects = tab_rects(&editor, &session);
    editor.handle_input(
        press(PointerButton::Middle, centre(rects[0].rect)),
        Some(&mut session),
    );
    assert_eq!(names(&session), ["b.luau"]);
}

#[test]
fn keyboard_commands_cycle_close_and_ask_for_files() {
    let fixture = Fixture::new("code-ui-keys");
    let (_, mut session) = session_with(&fixture, &["a.luau", "b.luau", "c.luau"], 1);
    let mut editor = editor();
    editor.handle_input(InputEvent::NextDocument, Some(&mut session));
    assert_eq!(active_name(&session), Some("a.luau"));
    editor.handle_input(InputEvent::PreviousDocument, Some(&mut session));
    assert_eq!(active_name(&session), Some("c.luau"));
    editor.handle_input(InputEvent::CloseDocument, Some(&mut session));
    assert_eq!(names(&session), ["a.luau", "b.luau"]);
    assert_eq!(
        editor.handle_input(InputEvent::OpenFile, Some(&mut session)),
        Some(CodeRequest::ListScripts)
    );
}

#[test]
fn switching_to_a_hidden_tab_scrolls_the_strip_to_it() {
    let fixture = Fixture::new("code-ui-overflow");
    let files: Vec<String> = (0..12).map(|n| format!("module_{n:02}.luau")).collect();
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_, mut session) = session_with(&fixture, &refs, 1);
    let mut editor = editor();
    editor.reveal_active(&mut session);
    let visible = |editor: &CodeEditor, session: &CodeSession| {
        let active = session.tabs().index_of(session.tabs().active().unwrap());
        tab_rects(editor, session)
            .iter()
            .any(|tab| Some(tab.index) == active)
    };
    assert!(tab_rects(&editor, &session).len() < files.len());
    assert!(visible(&editor, &session));

    // Wrapping to the first tab scrolls back to the start.
    editor.handle_input(InputEvent::NextDocument, Some(&mut session));
    assert_eq!(active_name(&session), Some("module_00.luau"));
    assert_eq!(session.tabs().scroll(), 0);
    for _ in 0..files.len() {
        editor.handle_input(InputEvent::NextDocument, Some(&mut session));
        assert!(visible(&editor, &session));
    }
}

#[test]
fn the_list_of_open_tabs_reaches_hidden_ones() {
    let fixture = Fixture::new("code-ui-menu");
    let files: Vec<String> = (0..12).map(|n| format!("module_{n:02}.luau")).collect();
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_, mut session) = session_with(&fixture, &refs, 1);
    let mut editor = editor();
    editor.handle_input(InputEvent::NextDocument, Some(&mut session));
    let more = centre(editor.layout().more_button());
    editor.handle_input(press(PointerButton::Primary, more), Some(&mut session));
    assert!(editor.has_overlay());

    // The list opens on the active tab; Up wraps nowhere and Enter shows.
    editor.handle_input(InputEvent::Up, Some(&mut session));
    for _ in 0..11 {
        editor.handle_input(InputEvent::Down, Some(&mut session));
    }
    editor.handle_input(InputEvent::Enter, Some(&mut session));
    assert!(!editor.has_overlay());
    assert_eq!(active_name(&session), Some("module_11.luau"));
    assert!(
        tab_rects(&editor, &session)
            .iter()
            .any(|tab| tab.index == 11)
    );

    // Escape closes it without changing tabs.
    editor.handle_input(press(PointerButton::Primary, more), Some(&mut session));
    editor.handle_input(InputEvent::Up, Some(&mut session));
    editor.handle_input(InputEvent::Break, Some(&mut session));
    assert!(!editor.has_overlay());
    assert_eq!(active_name(&session), Some("module_11.luau"));
}

#[test]
fn each_document_keeps_its_own_scroll_position() {
    let fixture = Fixture::new("code-ui-views");
    let (_, mut session) = session_with(&fixture, &["a.luau", "b.luau"], 100);
    let mut editor = editor();
    let text_centre = centre(editor.layout().text);
    editor.handle_input(
        InputEvent::PointerMove(Some(text_centre)),
        Some(&mut session),
    );
    editor.handle_input(InputEvent::Wheel { delta: -2 }, Some(&mut session));
    let b = id_of(&session, "b.luau");
    assert_eq!(session.tabs().view(b).unwrap().top_line, 2 * WHEEL_LINES);

    editor.handle_input(InputEvent::PreviousDocument, Some(&mut session));
    let a = id_of(&session, "a.luau");
    assert_eq!(session.tabs().view(a).unwrap().top_line, 0);
    editor.handle_input(InputEvent::NextDocument, Some(&mut session));
    assert_eq!(session.tabs().view(b).unwrap().top_line, 2 * WHEEL_LINES);

    // The text never scrolls past its last screenful, nor above the top.
    editor.handle_input(InputEvent::Wheel { delta: i8::MIN }, Some(&mut session));
    let furthest = 101 - editor.layout().text_rows();
    assert_eq!(session.tabs().view(b).unwrap().top_line, furthest);
    editor.handle_input(InputEvent::Wheel { delta: i8::MAX }, Some(&mut session));
    assert_eq!(session.tabs().view(b).unwrap().top_line, 0);
}

#[test]
fn the_picker_takes_every_input_until_it_closes() {
    let fixture = Fixture::new("code-ui-picker");
    let (cartridge, mut session) = session_with(&fixture, &["a.luau", "b.luau"], 1);
    let mut editor = editor();
    editor.open_picker(cartridge.script_files());
    assert!(editor.has_overlay());

    // Ctrl+W filters nothing and closes no tab while the picker is open.
    editor.handle_input(InputEvent::CloseDocument, Some(&mut session));
    assert_eq!(names(&session), ["a.luau", "b.luau"]);
    for ch in "main".chars() {
        editor.handle_input(InputEvent::Text(ch), Some(&mut session));
    }
    assert_eq!(
        editor.handle_input(InputEvent::Enter, Some(&mut session)),
        Some(CodeRequest::Open(
            SourcePath::parse("code/main.luau").unwrap()
        ))
    );
    assert!(!editor.has_overlay());

    editor.open_picker(cartridge.script_files());
    assert_eq!(
        editor.handle_input(InputEvent::Break, Some(&mut session)),
        None
    );
    assert!(!editor.has_overlay());
}

#[test]
fn forgetting_closes_overlays_and_the_entry_error() {
    let fixture = Fixture::new("code-ui-forget");
    let (cartridge, _) = session_with(&fixture, &[], 1);
    let mut editor = editor();
    editor.set_entry_error(Some("No such file: code/main.luau".into()));
    editor.open_picker(cartridge.script_files());
    editor.forget();
    assert_eq!(editor, self::editor());
}
