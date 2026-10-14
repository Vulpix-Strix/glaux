//! Context menus, NEW FILE and GO TO LINE in the Code view.

use super::selection::{caret, cell, geometry, open, selected};
use super::*;
use crate::system::workspace::context_menu::MenuEntry;
use pretty_assertions::assert_eq;

/// Each item's label and whether it can be chosen, separators left out.
fn items(editor: &CodeEditor) -> Vec<(&'static str, bool)> {
    editor
        .context_menu()
        .unwrap()
        .entries()
        .iter()
        .filter_map(|entry| match *entry {
            MenuEntry::Item { label, enabled, .. } => Some((label, enabled)),
            MenuEntry::Separator => None,
        })
        .collect()
}

fn enabled(editor: &CodeEditor, label: &str) -> bool {
    items(editor)
        .into_iter()
        .find(|&(item, _)| item == label)
        .unwrap()
        .1
}

/// Runs the menu's item `label` with the keyboard.
fn choose(editor: &mut CodeEditor, session: &mut CodeSession, label: &str) -> Vec<CodeRequest> {
    let shown = items(editor);
    let target = shown.iter().position(|&(item, _)| item == label).unwrap();
    let steps = shown[..=target].iter().filter(|(_, on)| *on).count();
    let mut events = vec![InputEvent::Down { select: false }; steps];
    events.push(InputEvent::Enter);
    requests(editor, session, &events)
}

#[test]
fn the_text_menu_disables_what_does_not_apply() {
    let fixture = Fixture::new("code-menu-enabled");
    let (mut editor, _, mut session) = open(&fixture, "abc def", EditorSettings::default());
    let at = cell(&geometry(&editor, &session), 0, 5);
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Secondary, at)],
    );
    assert_eq!(caret(&session), 5, "the press moved the caret");
    for (label, on) in [
        ("Undo", false),
        ("Redo", false),
        ("Cut", false),
        ("Copy", false),
        ("Paste", true),
    ] {
        assert_eq!(enabled(&editor, label), on, "{label}");
    }
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    assert!(editor.context_menu().is_none());
    feed(&mut editor, &mut session, &typing("x"));
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::WordLeft { select: true }],
    );
    let inside = cell(&geometry(&editor, &session), 0, 5);
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Secondary, inside)],
    );
    assert_eq!(
        selected(&session),
        4..6,
        "a press inside the selection keeps it"
    );
    assert!(enabled(&editor, "Undo") && enabled(&editor, "Cut"));
    assert_eq!(
        choose(&mut editor, &mut session, "Copy"),
        [CodeRequest::Copy("dx".into())]
    );
    assert!(editor.context_menu().is_none());
}

#[test]
fn menu_items_act_as_their_shortcuts() {
    let fixture = Fixture::new("code-menu-run");
    let (mut editor, _, mut session) = open(&fixture, "abc", EditorSettings::default());
    feed(&mut editor, &mut session, &typing("x"));
    let at = cell(&geometry(&editor, &session), 0, 1);
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Secondary, at)],
    );
    choose(&mut editor, &mut session, "Undo");
    assert_eq!(session.active_document().unwrap().text(), "abc");
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Secondary, at)],
    );
    choose(&mut editor, &mut session, "Find...");
    assert!(editor.find().is_some());
}

#[test]
fn the_tab_strip_menu_creates_opens_and_closes_files() {
    let fixture = Fixture::new("code-menu-strip");
    let (mut editor, _, mut session) = open(&fixture, "abc", EditorSettings::default());
    feed(&mut editor, &mut session, &typing("x"));
    let (_, tab, _) = editor.tab_rects(&session)[0];
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Secondary, centre(tab))],
    );
    assert_eq!(
        items(&editor),
        [
            ("New file...", true),
            ("Open file...", true),
            ("Close tab", true)
        ]
    );
    let id = session.tabs().active().unwrap();
    assert_eq!(
        choose(&mut editor, &mut session, "Close tab"),
        [CodeRequest::ConfirmClose(id)],
        "unsaved edits are asked about"
    );
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Secondary, centre(tab))],
    );
    assert_eq!(
        choose(&mut editor, &mut session, "New file..."),
        [CodeRequest::NewFile]
    );
}

#[test]
fn the_empty_view_links_to_new_and_open_file() {
    let mut editor = editor();
    let mut session = CodeSession::default();
    let [(new, ..), (open, ..)] = editor.empty_actions();
    assert_eq!(
        requests(
            &mut editor,
            &mut session,
            &[press(PointerButton::Primary, centre(new))]
        ),
        [CodeRequest::NewFile]
    );
    assert_eq!(
        requests(
            &mut editor,
            &mut session,
            &[press(PointerButton::Primary, centre(open))]
        ),
        [CodeRequest::ListScripts]
    );
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Secondary, centre(new))],
    );
    assert_eq!(
        items(&editor),
        [("New file...", true), ("Open file...", true)]
    );
}

#[test]
fn new_file_takes_every_key_until_created() {
    let fixture = Fixture::new("code-new-file");
    let (mut editor, _, mut session) = open(&fixture, "abc", EditorSettings::default());
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::NewFile]),
        [CodeRequest::NewFile]
    );
    editor.open_new_file(crate::cartridge::Language::Rhai);
    assert!(editor.has_overlay());
    // Document commands are swallowed; Select All and Paste act on the
    // field.
    assert_eq!(
        requests(
            &mut editor,
            &mut session,
            &[InputEvent::Save, InputEvent::Find, InputEvent::Undo]
        ),
        []
    );
    assert!(editor.find().is_none());
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::Paste]),
        [CodeRequest::Paste]
    );
    editor.paste(Some("ui/hud".into()), Some(&mut session));
    assert_eq!(session.active_document().unwrap().text(), "abc");
    let path = SourcePath::parse("code/ui/hud.rhai").unwrap();
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::Enter]),
        [CodeRequest::Create(path)]
    );
    editor.script_created(Err("Already exists: /code/ui/hud.rhai".into()));
    assert!(editor.new_file().is_some(), "stays open to fix the name");
    editor.script_created(Ok(()));
    assert!(editor.new_file().is_none());
}

#[test]
fn go_to_line_checks_the_number_then_moves_there() {
    let fixture = Fixture::new("code-go-to");
    let text: String = (1..=100).map(|n| format!("line {n}\n")).collect();
    let (mut editor, _, mut session) = open(&fixture, &text, EditorSettings::default());
    feed(&mut editor, &mut session, &[InputEvent::GoToLine]);
    let go_to = editor.go_to().unwrap();
    assert_eq!(go_to.message().0, "Enter: go  Esc: cancel");
    // The field starts with the caret's line, selected; 0 is no line.
    feed(&mut editor, &mut session, &typing("0"));
    feed(&mut editor, &mut session, &[InputEvent::Enter]);
    assert_eq!(
        editor.go_to().unwrap().message().0,
        "Enter a line from 1 to 101."
    );
    feed(&mut editor, &mut session, &[InputEvent::Backspace]);
    feed(&mut editor, &mut session, &typing("8x0"));
    feed(&mut editor, &mut session, &[InputEvent::Enter]);
    assert!(editor.go_to().is_none());
    let document = session.active_document().unwrap();
    assert_eq!(caret(&session), document.line_range(79).unwrap().start);
    let id = document.id();
    let view = session.tabs().view(id).unwrap();
    let rows = geometry(&editor, &session).rows;
    assert!(view.top_line <= 79 && 79 < view.top_line + rows, "{view:?}");
}
