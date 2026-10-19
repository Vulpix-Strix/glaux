//! Deleting and renaming scripts from their tabs' context menu, through the
//! whole system: dialogs, files on disk, tabs and unsaved edits.

use super::code_tabs::{loaded_with, open_file};
use super::*;
use pretty_assertions::assert_eq;
use std::path::PathBuf;

const UTIL: &str = "-- util\n";

fn code(system: &System) -> &CodeSession {
    system.code.as_ref().unwrap()
}

fn host(fixture: &Fixture, path: &str) -> PathBuf {
    fixture.root().join("pong").join(path)
}

fn read(fixture: &Fixture, path: &str) -> String {
    std::fs::read_to_string(host(fixture, path)).unwrap()
}

fn typed(system: System, text: &str) -> System {
    let events: Vec<InputEvent> = text.chars().map(InputEvent::Text).collect();
    system.update(&events)
}

fn modal(system: &System) -> Option<(&str, Vec<&str>)> {
    workspace::tests_support::modal(&system.workspace)
}

fn details(system: &System) -> (usize, Vec<String>) {
    workspace::tests_support::modal_details(&system.workspace).unwrap()
}

fn open_paths(system: &System) -> Vec<String> {
    let code = code(system);
    code.tabs()
        .ids()
        .map(|id| code.document(id).unwrap().path().to_string())
        .collect()
}

fn active_path(system: &System) -> String {
    code(system).active_document().unwrap().path().to_string()
}

/// `main.luau` and `util.luau` open, `util.luau` active and edited by
/// typing `edit` at its start.
fn with_util(fixture: &Fixture, edit: &str) -> System {
    let system = press_break(loaded_with(fixture, &[("code/util.luau", UTIL)]));
    typed(open_file(system, "util"), edit)
}

/// Presses the secondary button on the tab of the script called `name`.
fn right_click_tab(system: System, name: &str) -> System {
    let code = code(&system);
    let index = code
        .tabs()
        .ids()
        .position(|id| code.document(id).unwrap().path().file_name() == name)
        .unwrap();
    let (_, tab, _) = system.workspace.code().tab_rects(code)[index];
    let at = Point::new(
        (tab.x + tab.width / 2) as u16,
        (tab.y + tab.height / 2) as u16,
    );
    system.update(&[InputEvent::PointerDown {
        button: PointerButton::Secondary,
        at,
        select: false,
    }])
}

fn menu(system: &System) -> Vec<(&str, bool)> {
    workspace::tests_support::code_menu_items(&system.workspace)
}

/// Runs the context menu's item `label` with the keyboard.
fn choose(system: System, label: &str) -> System {
    let shown = menu(&system);
    let target = shown.iter().position(|&(item, _)| item == label).unwrap();
    let steps = shown[..=target].iter().filter(|(_, on)| *on).count();
    let mut events = vec![InputEvent::Down { select: false }; steps];
    events.push(InputEvent::Enter);
    system.update(&events)
}

fn delete_from_menu(system: System, name: &str) -> System {
    choose(right_click_tab(system, name), "Delete script...")
}

/// Moves up to DELETE, just above the focused CANCEL, and chooses it.
fn confirm_delete(system: System) -> System {
    system.update(&[InputEvent::Up { select: false }, InputEvent::Enter])
}

fn rename_to(system: System, name: &str, to: &str) -> System {
    let system = choose(right_click_tab(system, name), "Rename script...");
    typed(system, to).update(&[InputEvent::Enter])
}

#[test]
fn deleting_a_clean_script_asks_once_then_removes_the_file_and_closes_the_tab() {
    let fixture = Fixture::new("sys-delete-clean");
    let system = with_util(&fixture, "");
    let system = delete_from_menu(system, "util.luau");

    assert_eq!(
        modal(&system),
        Some(("DELETE SCRIPT", vec!["DELETE", "CANCEL"]))
    );
    let (focus, message) = details(&system);
    assert_eq!(focus, 1, "CANCEL is focused first");
    assert_eq!(
        message,
        [
            "Delete code/util.luau from the cartridge?",
            "The file cannot be recovered."
        ]
    );
    assert!(
        host(&fixture, "code/util.luau").exists(),
        "nothing happens before the answer"
    );

    let system = confirm_delete(system);
    assert_eq!(modal(&system), None);
    assert!(!host(&fixture, "code/util.luau").exists());
    assert!(host(&fixture, "code").is_dir(), "directories are kept");
    assert_eq!(open_paths(&system), ["code/main.luau"]);
    assert_eq!(active_path(&system), "code/main.luau");
}

#[test]
fn cancelling_a_delete_changes_neither_file_nor_document_nor_tabs() {
    let fixture = Fixture::new("sys-delete-cancel");
    let system = with_util(&fixture, "abc");
    let before = open_paths(&system);

    // Enter on the focused CANCEL, then Escape on a second attempt.
    let system = delete_from_menu(system, "util.luau").update(&[InputEvent::Enter]);
    assert_eq!(modal(&system), None);
    let system = delete_from_menu(system, "util.luau").update(&[InputEvent::Break]);
    assert_eq!(modal(&system), None);

    assert_eq!(read(&fixture, "code/util.luau"), UTIL);
    assert_eq!(open_paths(&system), before);
    assert_eq!(active_path(&system), "code/util.luau");
    assert_eq!(
        code(&system).active_document().unwrap().text(),
        format!("abc{UTIL}")
    );
    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(
        code(&system).active_document().unwrap().text(),
        UTIL,
        "history kept"
    );
}

#[test]
fn deleting_a_script_with_unsaved_edits_warns_in_the_same_dialog_and_never_saves() {
    let fixture = Fixture::new("sys-delete-dirty");
    let system = delete_from_menu(with_util(&fixture, "abc"), "util.luau");

    // One dialog, not an UNSAVED CHANGES question followed by another.
    assert_eq!(
        modal(&system),
        Some(("DELETE SCRIPT", vec!["DELETE", "CANCEL"]))
    );
    assert!(
        details(&system)
            .1
            .contains(&"Its unsaved changes will be lost.".to_owned())
    );

    let system = confirm_delete(system);
    assert_eq!(modal(&system), None);
    assert!(!host(&fixture, "code/util.luau").exists());
    assert_eq!(open_paths(&system), ["code/main.luau"]);
}

#[test]
fn a_file_changed_outside_glaux_is_flagged_and_deleted_only_if_unchanged_since() {
    let fixture = Fixture::new("sys-delete-outside");
    let system = with_util(&fixture, "");
    std::fs::write(host(&fixture, "code/util.luau"), "-- other\n").unwrap();

    let system = delete_from_menu(system, "util.luau");
    assert!(
        details(&system)
            .1
            .contains(&"It changed outside Glaux; those changes will be lost.".to_owned())
    );

    // It changes again while the question is open: the delete is refused.
    std::fs::write(host(&fixture, "code/util.luau"), "-- third\n").unwrap();
    let system = confirm_delete(system);
    assert_eq!(modal(&system), Some(("DELETE FAILED", vec!["OK"])));
    assert_eq!(read(&fixture, "code/util.luau"), "-- third\n");
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(open_paths(&system), ["code/main.luau", "code/util.luau"]);
}

#[test]
fn a_file_already_deleted_outside_glaux_only_loses_its_tab() {
    let fixture = Fixture::new("sys-delete-gone");
    let system = with_util(&fixture, "");
    std::fs::remove_file(host(&fixture, "code/util.luau")).unwrap();

    let system = delete_from_menu(system, "util.luau");
    assert!(details(&system).1[1].contains("already deleted outside Glaux"));
    let system = confirm_delete(system);
    assert_eq!(open_paths(&system), ["code/main.luau"]);
}

#[test]
fn a_failed_delete_keeps_the_file_the_tab_and_the_edits_and_says_so() {
    let fixture = Fixture::new("sys-delete-fails");
    let system = with_util(&fixture, "abc");
    let file = host(&fixture, "code/util.luau");
    let mut permissions = std::fs::metadata(&file).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&file, permissions).unwrap();

    let system = confirm_delete(delete_from_menu(system, "util.luau"));
    assert_eq!(modal(&system), Some(("DELETE FAILED", vec!["OK"])));
    let message = details(&system).1;
    assert_eq!(
        message[0],
        "code/util.luau is read-only. Make it writable, then try again."
    );
    assert_eq!(message[1], "Nothing was deleted.");
    assert!(file.exists());
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(open_paths(&system), ["code/main.luau", "code/util.luau"]);
    assert_eq!(
        code(&system).active_document().unwrap().text(),
        format!("abc{UTIL}")
    );
}

#[test]
fn the_entry_script_offers_neither_delete_nor_rename() {
    let fixture = Fixture::new("sys-entry-protected");
    let system = press_break(loaded_with(&fixture, &[("code/util.luau", UTIL)]));
    let system = right_click_tab(system, "main.luau");
    assert_eq!(
        menu(&system),
        [
            ("New file...", true),
            ("Open file...", true),
            ("Close tab", true),
            ("Rename script...", false),
            ("Delete script...", false),
        ]
    );
    let system = system.update(&[InputEvent::Break]);
    assert_eq!(modal(&system), None);
    assert!(host(&fixture, "code/main.luau").exists());
}

#[test]
fn renaming_moves_the_file_and_keeps_edits_history_and_the_dirty_state() {
    let fixture = Fixture::new("sys-rename");
    let system = with_util(&fixture, "abc");
    let system = rename_to(system, "util.luau", "lib/tools/util2");

    assert!(!system.workspace.code().has_overlay());
    assert_eq!(read(&fixture, "code/lib/tools/util2.luau"), UTIL);
    assert!(!host(&fixture, "code/util.luau").exists());
    assert_eq!(
        open_paths(&system),
        ["code/main.luau", "code/lib/tools/util2.luau"]
    );
    assert_eq!(active_path(&system), "code/lib/tools/util2.luau");
    let document = code(&system).active_document().unwrap();
    assert_eq!(document.text(), format!("abc{UTIL}"));
    assert!(document.is_modified(), "still unsaved");

    // Saving writes the new path; undo still reaches the saved text.
    let system = system.update(&[InputEvent::Save]);
    assert_eq!(
        read(&fixture, "code/lib/tools/util2.luau"),
        format!("abc{UTIL}")
    );
    assert!(!host(&fixture, "code/util.luau").exists());
    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(code(&system).active_document().unwrap().text(), UTIL);
}

#[test]
fn renaming_to_a_path_that_exists_or_is_open_keeps_everything_and_explains() {
    let fixture = Fixture::new("sys-rename-taken");
    let system = loaded_with(
        &fixture,
        &[("code/util.luau", UTIL), ("code/other.luau", "-- other\n")],
    );
    let system = open_file(press_break(system), "util");

    // On disk but not open.
    let system = rename_to(system, "util.luau", "other");
    assert!(system.workspace.code().rename().is_some());
    assert_eq!(
        system.workspace.code().rename().unwrap().message().0,
        "code/other.luau already exists."
    );
    assert_eq!(read(&fixture, "code/other.luau"), "-- other\n");
    assert_eq!(read(&fixture, "code/util.luau"), UTIL);

    // Open in another tab: refused before the disk is asked.
    let system = system.update(&[InputEvent::Break]);
    let system = open_file(system, "other");
    let system = rename_to(system, "util.luau", "other");
    assert_eq!(
        system.workspace.code().rename().unwrap().message().0,
        "code/other.luau is already open in another tab"
    );
    assert_eq!(read(&fixture, "code/util.luau"), UTIL);
    assert_eq!(
        open_paths(&system),
        ["code/main.luau", "code/util.luau", "code/other.luau"]
    );
}

#[test]
fn renaming_to_the_same_path_closes_the_dialog_and_touches_nothing() {
    let fixture = Fixture::new("sys-rename-same");
    let system = with_util(&fixture, "abc");
    let before = std::fs::metadata(host(&fixture, "code/util.luau"))
        .unwrap()
        .modified()
        .unwrap();
    let system = choose(right_click_tab(system, "util.luau"), "Rename script...");
    let system = system.update(&[InputEvent::Enter]);
    assert!(!system.workspace.code().has_overlay());
    assert_eq!(read(&fixture, "code/util.luau"), UTIL);
    assert_eq!(
        std::fs::metadata(host(&fixture, "code/util.luau"))
            .unwrap()
            .modified()
            .unwrap(),
        before
    );
    assert_eq!(open_paths(&system), ["code/main.luau", "code/util.luau"]);
}

#[test]
fn a_file_changed_or_deleted_outside_glaux_is_not_renamed() {
    let fixture = Fixture::new("sys-rename-outside");
    let system = with_util(&fixture, "abc");
    std::fs::write(host(&fixture, "code/util.luau"), "-- other\n").unwrap();
    let system = rename_to(system, "util.luau", "moved");
    assert_eq!(
        system.workspace.code().rename().unwrap().message().0,
        "code/util.luau changed on disk outside Glaux. Close and reopen it before renaming."
    );
    assert_eq!(read(&fixture, "code/util.luau"), "-- other\n");
    assert!(!host(&fixture, "code/moved.luau").exists());

    std::fs::remove_file(host(&fixture, "code/util.luau")).unwrap();
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(
        system.workspace.code().rename().unwrap().message().0,
        "code/util.luau no longer exists on disk."
    );
    assert_eq!(active_path(&system), "code/util.luau");
    assert_eq!(
        code(&system).active_document().unwrap().text(),
        format!("abc{UTIL}"),
        "the edits are still in memory"
    );
}
