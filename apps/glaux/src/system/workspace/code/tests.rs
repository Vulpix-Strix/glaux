use std::fs;

use super::*;
use crate::cartridge::LoadedCartridge;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use crate::system::code_document::DocumentId;
use crate::system::code_session::DocumentView;
use crate::system::editor_settings::EditorSettings;
use glaux_machine::font::FontMetrics;
use pretty_assertions::assert_eq;

pub(super) fn editor() -> CodeEditor {
    let metrics = FontMetrics::SYSTEM;
    CodeEditor::new(
        Metrics {
            cell_w: i32::from(metrics.cell_width()),
            cell_h: i32::from(metrics.cell_height()),
            top: 16,
        },
        EditorSettings::default(),
    )
}

/// A cartridge holding `names` in `code/`, each holding `lines` numbered
/// lines, and a session with all of them open in that order, the last
/// active.
fn session_with(fixture: &Fixture, names: &[&str], lines: usize) -> (LoadedCartridge, CodeSession) {
    let text: String = (1..=lines).map(|n| format!("line {n}\n")).collect();
    session_with_text(fixture, names, &text)
}

/// A cartridge holding `names` in `code/`, each holding `text`, and a
/// session with all of them open in that order, the last active and
/// measured for `editor`.
pub(super) fn session_with_text(
    fixture: &Fixture,
    names: &[&str],
    text: &str,
) -> (LoadedCartridge, CodeSession) {
    let dir = fixture.root().join("games/pong");
    write_cartridge(&dir, "Pong");
    for name in names {
        fs::write(dir.join("code").join(name), text).unwrap();
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

pub(super) fn id_of(session: &CodeSession, name: &str) -> DocumentId {
    session
        .tabs()
        .ids()
        .find(|&id| session.document(id).unwrap().path().file_name() == name)
        .unwrap()
}

pub(super) fn centre(rect: Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

pub(super) fn press(button: PointerButton, at: Point) -> InputEvent {
    InputEvent::PointerDown { button, at }
}

pub(super) fn release(at: Point) -> InputEvent {
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
    // 101 lines, the last empty, each 60 columns wide: 13 more than the 47
    // left beside a three-digit gutter.
    let text = format!("{}\n", "x".repeat(60)).repeat(100);
    let (_, mut session) = session_with_text(&fixture, &["a.luau", "b.luau"], &text);
    let mut editor = editor();
    let text_centre = centre(editor.layout().text);
    editor.handle_input(
        InputEvent::PointerMove(Some(text_centre)),
        Some(&mut session),
    );
    let view = |session: &CodeSession, name| *session.tabs().view(id_of(session, name)).unwrap();
    let at = |top_line, top_row, left_column| DocumentView {
        top_line,
        top_row,
        left_column,
    };

    editor.handle_input(InputEvent::Wheel { delta: -2 }, Some(&mut session));
    editor.handle_input(
        InputEvent::WheelHorizontal { delta: -1 },
        Some(&mut session),
    );
    assert_eq!(view(&session, "b.luau"), at(6, 0, 6));
    editor.handle_input(InputEvent::PreviousDocument, Some(&mut session));
    assert_eq!(view(&session, "a.luau"), at(0, 0, 0));
    editor.handle_input(InputEvent::NextDocument, Some(&mut session));
    assert_eq!(view(&session, "b.luau"), at(6, 0, 6));

    // The text never scrolls past its last screenful or the column after
    // its widest line, where the caret can be, nor above the top or left
    // of the first column.
    editor.handle_input(InputEvent::Wheel { delta: i8::MIN }, Some(&mut session));
    editor.handle_input(
        InputEvent::WheelHorizontal { delta: i8::MIN },
        Some(&mut session),
    );
    assert_eq!(view(&session, "b.luau"), at(101 - 18, 0, 61 - 47));
    editor.handle_input(InputEvent::Wheel { delta: i8::MAX }, Some(&mut session));
    editor.handle_input(
        InputEvent::WheelHorizontal { delta: i8::MAX },
        Some(&mut session),
    );
    assert_eq!(view(&session, "b.luau"), at(0, 0, 0));

    // Folded in two, three rows down lands on the second fold of line 1.
    editor.set_settings(EditorSettings {
        word_wrap: true,
        ..EditorSettings::default()
    });
    editor.handle_input(InputEvent::Wheel { delta: -1 }, Some(&mut session));
    assert_eq!(view(&session, "b.luau"), at(1, 1, 0));
    editor.handle_input(InputEvent::PreviousDocument, Some(&mut session));
    assert_eq!(view(&session, "a.luau"), at(0, 0, 0));
}

#[test]
fn the_wheel_over_the_strip_never_scrolls_the_text() {
    let fixture = Fixture::new("code-ui-wheel-strip");
    let text = format!("{}\n", "x".repeat(60)).repeat(100);
    let (_, mut session) = session_with_text(&fixture, &["a.luau"], &text);
    let mut editor = editor();
    let over_strip = centre(editor.layout().strip);
    editor.handle_input(
        InputEvent::PointerMove(Some(over_strip)),
        Some(&mut session),
    );
    editor.handle_input(InputEvent::Wheel { delta: -2 }, Some(&mut session));
    editor.handle_input(
        InputEvent::WheelHorizontal { delta: -2 },
        Some(&mut session),
    );
    let id = id_of(&session, "a.luau");
    assert_eq!(session.tabs().view(id), Some(&DocumentView::default()));
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

#[test]
fn settings_apply_at_once_and_are_kept_when_their_window_closes() {
    let fixture = Fixture::new("code-ui-settings");
    let (_, mut session) = session_with(&fixture, &["a.luau", "b.luau"], 3);
    let mut editor = editor();
    let gear = centre(editor.layout().settings_button());
    editor.handle_input(press(PointerButton::Primary, gear), Some(&mut session));
    assert!(editor.has_overlay());

    editor.handle_input(InputEvent::Enter, Some(&mut session));
    let wrapping = EditorSettings {
        word_wrap: true,
        ..EditorSettings::default()
    };
    assert_eq!(editor.settings(), wrapping);
    // Every open document is measured again for the new settings, not
    // only the one shown: folded at the 48 columns beside a two-digit
    // gutter.
    for id in session.tabs().ids() {
        let key = session.tabs().metrics(id).unwrap().key();
        assert_eq!(key.wrap_columns, Some(48));
    }

    assert_eq!(
        editor.handle_input(InputEvent::Break, Some(&mut session)),
        Some(CodeRequest::SaveSettings(wrapping))
    );
    assert!(!editor.has_overlay());
    assert_eq!(editor.settings(), wrapping);
}

#[test]
fn closing_settings_unchanged_asks_for_no_save() {
    let fixture = Fixture::new("code-ui-settings-same");
    let (_, mut session) = session_with(&fixture, &["a.luau"], 3);
    let mut editor = editor();
    let gear = centre(editor.layout().settings_button());
    editor.handle_input(press(PointerButton::Primary, gear), Some(&mut session));
    editor.handle_input(InputEvent::Enter, Some(&mut session));
    editor.handle_input(InputEvent::Enter, Some(&mut session));
    assert_eq!(
        editor.handle_input(InputEvent::Break, Some(&mut session)),
        None
    );
    assert!(!editor.has_overlay());
}

#[test]
fn settings_open_without_a_cartridge_and_survive_forgetting_one() {
    let mut editor = editor();
    let gear = centre(editor.layout().settings_button());
    editor.handle_input(press(PointerButton::Primary, gear), None);
    assert!(editor.has_overlay());
    editor.forget();
    assert!(editor.has_overlay());
    editor.handle_input(InputEvent::Enter, None);
    assert_eq!(
        editor.handle_input(InputEvent::Break, None),
        Some(CodeRequest::SaveSettings(EditorSettings {
            word_wrap: true,
            ..EditorSettings::default()
        }))
    );
}

fn caret_of(session: &CodeSession, name: &str) -> Caret {
    session.tabs().caret(id_of(session, name)).unwrap()
}

fn feed(editor: &mut CodeEditor, session: &mut CodeSession, events: &[InputEvent]) {
    for &event in events {
        editor.handle_input(event, Some(session));
    }
}

#[test]
fn each_document_keeps_its_own_caret() {
    let fixture = Fixture::new("code-ui-carets");
    let (_, mut session) = session_with(&fixture, &["a.luau", "b.luau"], 10);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    let right = InputEvent::Right { select: false };
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Down, InputEvent::Down, right],
    );
    // "line N\n" is 7 bytes: line 2, column 1.
    assert_eq!(caret_of(&session, "b.luau").byte(), 15);

    feed(&mut editor, &mut session, &[InputEvent::PreviousDocument]);
    assert_eq!(caret_of(&session, "a.luau").byte(), 0);
    feed(&mut editor, &mut session, &[right]);
    feed(&mut editor, &mut session, &[InputEvent::NextDocument]);
    assert_eq!(caret_of(&session, "b.luau").byte(), 15);
    assert_eq!(caret_of(&session, "a.luau").byte(), 1);
}

#[test]
fn a_click_in_the_text_moves_the_caret_and_one_elsewhere_does_not() {
    let fixture = Fixture::new("code-ui-click");
    let (_, mut session) = session_with(&fixture, &["a.luau"], 10);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    let geometry = editor.geometry(session.active_document().unwrap());
    let click = |x: i32, y: i32| InputEvent::PointerDown {
        button: PointerButton::Primary,
        at: Point::new(x as u16, y as u16),
    };
    let (x0, y0) = (geometry.viewport.x, geometry.viewport.y);
    feed(&mut editor, &mut session, &[click(x0 + 3 * 6, y0 + 25)]);
    assert_eq!(caret_of(&session, "a.luau").byte(), 2 * 7 + 3);
    // The strip and the options bar are not text.
    let options = editor.layout().options;
    feed(&mut editor, &mut session, &[click(x0, options.y + 2)]);
    assert_eq!(caret_of(&session, "a.luau").byte(), 2 * 7 + 3);
}

#[test]
fn moving_the_caret_past_the_view_scrolls_it_one_row_at_a_time() {
    let fixture = Fixture::new("code-ui-caret-scroll");
    let (_, mut session) = session_with(&fixture, &["a.luau"], 100);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    let rows = editor.geometry(session.active_document().unwrap()).rows;
    let id = id_of(&session, "a.luau");
    let downs = vec![InputEvent::Down; rows - 1];
    feed(&mut editor, &mut session, &downs);
    assert_eq!(session.tabs().view(id).unwrap().top_line, 0);
    feed(&mut editor, &mut session, &[InputEvent::Down]);
    assert_eq!(session.tabs().view(id).unwrap().top_line, 1);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentStart { select: false }],
    );
    assert_eq!(session.tabs().view(id).unwrap().top_line, 0);
}

#[test]
fn a_page_moves_the_caret_and_the_view_together() {
    let fixture = Fixture::new("code-ui-page");
    let (_, mut session) = session_with(&fixture, &["a.luau"], 100);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    let rows = editor.geometry(session.active_document().unwrap()).rows;
    let id = id_of(&session, "a.luau");
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Down, InputEvent::Down],
    );
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PageDown { select: false }],
    );
    let line = |session: &CodeSession| {
        let byte = caret_of(session, "a.luau").byte();
        session.active_document().unwrap().line_of(byte)
    };
    // The caret keeps its row on screen.
    assert_eq!(line(&session), 2 + rows);
    assert_eq!(session.tabs().view(id).unwrap().top_line, rows);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PageUp { select: false }],
    );
    assert_eq!(line(&session), 2);
    assert_eq!(session.tabs().view(id).unwrap().top_line, 0);
}

#[test]
fn end_of_a_long_line_scrolls_sideways_to_show_the_caret() {
    let fixture = Fixture::new("code-ui-caret-sideways");
    // 60 columns beside a two-digit gutter that leaves 48.
    let (_, mut session) = session_with_text(&fixture, &["a.luau"], &"x".repeat(60));
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    let id = id_of(&session, "a.luau");
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::End { select: false }],
    );
    assert_eq!(caret_of(&session, "a.luau").byte(), 60);
    assert_eq!(session.tabs().view(id).unwrap().left_column, 61 - 48);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Home { select: false }],
    );
    assert_eq!(session.tabs().view(id).unwrap().left_column, 0);
}

#[test]
fn changing_how_text_folds_keeps_the_caret_but_not_its_column() {
    let fixture = Fixture::new("code-ui-caret-settings");
    let (_, mut session) = session_with_text(&fixture, &["a.luau"], "abcdef\nab\nabcdef");
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::End { select: false }],
    );
    feed(&mut editor, &mut session, &[InputEvent::Down]);
    let before = caret_of(&session, "a.luau");
    assert_eq!((before.byte(), before.preferred_column()), (9, Some(6)));

    editor.set_settings(EditorSettings {
        word_wrap: true,
        ..EditorSettings::default()
    });
    editor.refresh_metrics(&mut session);
    let after = caret_of(&session, "a.luau");
    assert_eq!((after.byte(), after.preferred_column()), (9, None));
}

#[test]
fn caret_keys_do_nothing_while_an_overlay_is_open() {
    let fixture = Fixture::new("code-ui-caret-overlay");
    let (_, mut session) = session_with(&fixture, &["a.luau"], 10);
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    editor.open_picker(Ok(ScriptListing::default()));
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::Down, InputEvent::End { select: false }],
    );
    assert_eq!(caret_of(&session, "a.luau").byte(), 0);
}
