//! Context menus, NEW FILE and GO TO LINE in the Code view.

use super::selection::{caret, cell, geometry, open, selected};
use super::*;
use crate::system::workspace::context_menu::MenuEntry;
use pretty_assertions::assert_eq;

/// Each item's label and whether it can be chosen, separators left out.
fn items(editor: &CodeEditor) -> Vec<(&str, bool)> {
    editor
        .context_menu()
        .unwrap()
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item { label, enabled, .. } => Some((label.as_str(), *enabled)),
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
            ("Close tab", true),
            ("Rename script...", true),
            ("Delete script...", true),
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

/// Opens the tab menu on tab `index` of `session`.
fn tab_menu(editor: &mut CodeEditor, session: &mut CodeSession, index: usize) {
    let (_, tab, _) = editor.tab_rects(session)[index];
    feed(
        editor,
        session,
        &[press(PointerButton::Secondary, centre(tab))],
    );
}

#[test]
fn the_entry_script_cannot_be_renamed_or_deleted_from_its_tab() {
    let fixture = Fixture::new("code-menu-entry");
    let (_, mut session) = session_with_text(&fixture, &["a.luau", "b.luau"], "");
    let mut editor = editor();
    editor.refresh_metrics(&mut session);
    // The manifest, not a file name, decides which script is the entry.
    editor.set_entry("code/b.luau");

    tab_menu(&mut editor, &mut session, 0);
    assert!(enabled(&editor, "Rename script..."));
    assert!(enabled(&editor, "Delete script..."));
    feed(&mut editor, &mut session, &[InputEvent::Break]);

    tab_menu(&mut editor, &mut session, 1);
    assert!(!enabled(&editor, "Rename script..."));
    assert!(!enabled(&editor, "Delete script..."));
    // Disabled items are skipped, so Enter cannot reach them.
    assert_eq!(
        requests(
            &mut editor,
            &mut session,
            &[InputEvent::Down { select: false }; 6]
        ),
        []
    );
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::Enter]),
        []
    );
}

#[test]
fn a_script_outside_code_can_be_deleted_but_not_renamed() {
    let fixture = Fixture::new("code-menu-outside-code");
    let (cart, mut session) = session_with_text(&fixture, &["a.luau"], "");
    let dir = fixture.root().join("games/pong");
    std::fs::create_dir_all(dir.join("tools")).unwrap();
    std::fs::write(dir.join("tools/t.luau"), "").unwrap();
    session
        .open(&cart, &SourcePath::parse("tools/t.luau").unwrap())
        .unwrap();
    let mut editor = editor();
    editor.refresh_metrics(&mut session);

    tab_menu(&mut editor, &mut session, 1);
    assert!(!enabled(&editor, "Rename script..."));
    assert!(enabled(&editor, "Delete script..."));
}

#[test]
fn delete_script_asks_the_workspace_and_changes_nothing_itself() {
    let fixture = Fixture::new("code-menu-delete");
    let (mut editor, _, mut session) = open(&fixture, "abc", EditorSettings::default());
    feed(&mut editor, &mut session, &typing("x"));
    let id = session.tabs().active().unwrap();
    tab_menu(&mut editor, &mut session, 0);
    assert_eq!(
        choose(&mut editor, &mut session, "Delete script..."),
        [CodeRequest::DeleteScript(id)]
    );
    assert!(session.document(id).unwrap().is_modified());
    assert_eq!(session.tabs().len(), 1);
}

#[test]
fn rename_script_starts_from_the_current_path_and_takes_every_key() {
    let fixture = Fixture::new("code-menu-rename");
    let (mut editor, _, mut session) = open(&fixture, "abc", EditorSettings::default());
    feed(&mut editor, &mut session, &typing("x"));
    let id = session.tabs().active().unwrap();
    tab_menu(&mut editor, &mut session, 0);
    assert_eq!(choose(&mut editor, &mut session, "Rename script..."), []);

    let dialog = editor.rename().unwrap();
    assert_eq!(
        dialog.prompt().text(),
        "a",
        "without code/ and the extension"
    );
    assert!(editor.has_overlay());
    assert_eq!(
        requests(
            &mut editor,
            &mut session,
            &[InputEvent::Save, InputEvent::Find, InputEvent::Undo]
        ),
        [],
        "document commands are swallowed"
    );
    assert_eq!(session.active_document().unwrap().text(), "xabc");

    // The prefilled name is selected, so typing replaces it.
    feed(&mut editor, &mut session, &typing("ui/hud"));
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::Enter]),
        [CodeRequest::Rename {
            id,
            to: SourcePath::parse("code/ui/hud.luau").unwrap()
        }]
    );
    editor.script_renamed(Err("code/ui/hud.luau already exists.".into()));
    assert_eq!(
        editor.rename().unwrap().message().0,
        "code/ui/hud.luau already exists.",
        "stays open to fix the path"
    );
    editor.script_renamed(Ok(()));
    assert!(editor.rename().is_none());
}

#[test]
fn rename_script_rejects_bad_paths_without_asking_and_closes_on_the_same_path() {
    let fixture = Fixture::new("code-menu-rename-invalid");
    let (mut editor, _, mut session) = open(&fixture, "abc", EditorSettings::default());
    tab_menu(&mut editor, &mut session, 0);
    choose(&mut editor, &mut session, "Rename script...");

    for typed in ["", "x.luau", "x.rhai", "../x", "a//b", "a\\b"] {
        feed(&mut editor, &mut session, &[InputEvent::SelectAll]);
        feed(&mut editor, &mut session, &typing(typed));
        if typed.is_empty() {
            feed(&mut editor, &mut session, &[InputEvent::Backspace]);
        }
        assert_eq!(
            requests(&mut editor, &mut session, &[InputEvent::Enter]),
            [],
            "{typed:?}"
        );
        assert!(editor.rename().is_some(), "{typed:?} keeps the window open");
    }

    // Typing the path it already has asks the filesystem for nothing.
    feed(&mut editor, &mut session, &[InputEvent::SelectAll]);
    feed(&mut editor, &mut session, &typing("a"));
    assert_eq!(
        requests(&mut editor, &mut session, &[InputEvent::Enter]),
        []
    );
    assert!(editor.rename().is_none());
}
