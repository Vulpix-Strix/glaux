//! Asking before unsaved edits are dropped: closing a script's tab,
//! replacing or unloading the cartridge, and quitting.

use super::code_tabs::{loaded_with, open_file};
use super::*;
use pretty_assertions::assert_eq;
use std::path::{Path, PathBuf};

const MAIN: &str = "-- main\n";
const UTIL: &str = "-- util\n";

fn typed(system: System, text: &str) -> System {
    let events: Vec<InputEvent> = text.chars().map(InputEvent::Text).collect();
    system.update(&events)
}

fn modal(system: &System) -> Option<(&str, Vec<&str>)> {
    workspace::tests_support::modal(&system.workspace)
}

fn code(system: &System) -> &CodeSession {
    system.code.as_ref().unwrap()
}

fn active_text(system: &System) -> String {
    code(system).active_document().unwrap().text().to_owned()
}

/// The question's message, while it is open.
fn question_message(system: &System) -> Vec<String> {
    system.unsaved_flow().unwrap().dialog().message().to_vec()
}

fn host(fixture: &Fixture, path: &str) -> PathBuf {
    fixture.root().join("pong").join(path)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// Makes `path` read-only, returning what restores it.
fn read_only(path: &Path) -> std::fs::Permissions {
    let writable = std::fs::metadata(path).unwrap().permissions();
    let mut read_only = writable.clone();
    read_only.set_readonly(true);
    std::fs::set_permissions(path, read_only).unwrap();
    writable
}

/// `Pong` in the Code view with `code/main.luau` and `code/util.luau`
/// open, in that tab order, each edited by typing `a` or `b` at its start.
/// The workspace is left on `util.luau`.
fn two_edited(fixture: &Fixture) -> System {
    let system = press_break(loaded_with(fixture, &[("code/util.luau", UTIL)]));
    let system = typed(system, "a");
    let system = typed(open_file(system, "util"), "b");
    assert_eq!(code(&system).modified().len(), 2);
    system
}

/// Chooses SAVE ALL, two buttons above the focused CANCEL.
fn save_all(system: System) -> System {
    let up = InputEvent::Up { select: false };
    system.update(&[up, up, InputEvent::Enter])
}

/// Chooses DISCARD, just above the focused CANCEL.
fn discard(system: System) -> System {
    system.update(&[InputEvent::Up { select: false }, InputEvent::Enter])
}

#[test]
fn closing_an_unmodified_tab_asks_nothing() {
    let fixture = Fixture::new("sys-unsaved-close-clean");
    let system = press_break(loaded_with(&fixture, &[]));
    let system = system.update(&[InputEvent::CloseDocument]);
    assert_eq!(modal(&system), None);
    assert_eq!(code(&system).tabs().len(), 0);
}

#[test]
fn closing_a_modified_tab_asks_and_cancel_keeps_its_text_and_history() {
    let fixture = Fixture::new("sys-unsaved-close-cancel");
    let system = typed(press_break(loaded_with(&fixture, &[])), "abc");
    let system = system.update(&[InputEvent::CloseDocument]);
    assert_eq!(
        modal(&system),
        Some(("UNSAVED CHANGES", vec!["SAVE", "DISCARD", "CANCEL"]))
    );

    // CANCEL is focused, so a stray Enter loses nothing.
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(modal(&system), None);
    assert_eq!(code(&system).tabs().len(), 1);
    assert_eq!(active_text(&system), format!("abc{MAIN}"));
    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(active_text(&system), MAIN);
}

#[test]
fn discard_closes_the_tab_without_writing_and_save_writes_then_closes() {
    let fixture = Fixture::new("sys-unsaved-close-choices");
    let file = host(&fixture, "code/main.luau");
    let system = typed(press_break(loaded_with(&fixture, &[])), "x");
    let system = discard(system.update(&[InputEvent::CloseDocument]));
    assert_eq!(code(&system).tabs().len(), 0);
    assert_eq!(read(&file), MAIN);

    let system = typed(open_file(system, "main"), "y");
    let system = system.update(&[InputEvent::CloseDocument]);
    let up = InputEvent::Up { select: false };
    let system = system.update(&[up, up, InputEvent::Enter]);
    assert_eq!(code(&system).tabs().len(), 0);
    assert_eq!(read(&file), format!("y{MAIN}"));
}

#[test]
fn saving_a_tab_that_changed_outside_glaux_keeps_it_open_and_asks() {
    let fixture = Fixture::new("sys-unsaved-close-conflict");
    let file = host(&fixture, "code/main.luau");
    let system = typed(press_break(loaded_with(&fixture, &[])), "x");
    std::fs::write(&file, "theirs").unwrap();

    let system = system.update(&[InputEvent::CloseDocument]);
    let up = InputEvent::Up { select: false };
    let system = system.update(&[up, up, InputEvent::Enter]);
    assert_eq!(
        modal(&system),
        Some(("FILE CONFLICT", vec!["OVERWRITE DISK", "CANCEL"]))
    );
    assert_eq!(code(&system).tabs().len(), 1);
    assert_eq!(read(&file), "theirs");
    assert_eq!(active_text(&system), format!("x{MAIN}"));
}

#[test]
fn one_question_lists_every_unsaved_script_before_a_shell_command() {
    let fixture = Fixture::new("sys-unsaved-list");
    let system = enter(press_break(two_edited(&fixture)), "load pong");
    assert_eq!(unsaved_question(&system), asks_about_unsaved());
    assert_eq!(
        question_message(&system)[0],
        "code/main.luau and code/util.luau have unsaved changes."
    );
}

#[test]
fn cancel_keeps_the_cartridge_and_every_edit() {
    let fixture = Fixture::new("sys-unsaved-cancel");
    for command in ["load pong", "unload", "new other"] {
        let system = enter(press_break(two_edited(&fixture)), command);
        let system = press_break(system);
        assert_eq!(unsaved_question(&system), None, "{command}");
        assert_eq!(loaded_name(&system), Some("Pong"), "{command}");
        assert_eq!(code(&system).modified().len(), 2, "{command}");
        assert_eq!(read(&host(&fixture, "code/main.luau")), MAIN, "{command}");
    }
    assert!(
        !fixture.root().join("other").exists(),
        "new created nothing"
    );
}

#[test]
fn discard_runs_the_command_without_writing() {
    let fixture = Fixture::new("sys-unsaved-discard");
    let system = discard(enter(press_break(two_edited(&fixture)), "unload"));
    assert_eq!(after(&system, "> unload"), ["Unloaded Pong 0.1.0"]);
    assert!(system.code.is_none());
    assert_eq!(read(&host(&fixture, "code/main.luau")), MAIN);
    assert_eq!(read(&host(&fixture, "code/util.luau")), UTIL);
}

#[test]
fn save_all_writes_every_script_then_runs_the_command() {
    let fixture = Fixture::new("sys-unsaved-save-all");
    let system = save_all(enter(press_break(two_edited(&fixture)), "load pong"));
    assert_eq!(unsaved_question(&system), None);
    assert_eq!(read(&host(&fixture, "code/main.luau")), format!("a{MAIN}"));
    assert_eq!(read(&host(&fixture, "code/util.luau")), format!("b{UTIL}"));
    assert_eq!(after(&system, "> load pong"), ["Loaded Pong 0.1.0"]);
    assert!(system.code.is_none(), "the new load starts afresh");
}

#[test]
fn discard_on_the_command_line_still_skips_the_question() {
    let fixture = Fixture::new("sys-unsaved-flag");
    let system = enter(press_break(two_edited(&fixture)), "unload --discard");
    assert_eq!(unsaved_question(&system), None);
    assert_eq!(
        after(&system, "> unload --discard"),
        ["Unloaded Pong 0.1.0"]
    );
}

#[test]
fn a_failed_save_keeps_earlier_saves_and_stops_the_command() {
    let fixture = Fixture::new("sys-unsaved-save-failed");
    let util = host(&fixture, "code/util.luau");
    let system = enter(press_break(two_edited(&fixture)), "load pong");
    let writable = read_only(&util);
    let system = save_all(system);
    std::fs::set_permissions(&util, writable).unwrap();

    let dialog = system.unsaved_flow().unwrap().dialog();
    assert_eq!(dialog.title(), "SAVE FAILED");
    assert_eq!(dialog.labels(), ["OK"]);
    let message = dialog.message();
    assert_eq!(message[0], "Saved code/main.luau before stopping.");
    assert_eq!(message.last().unwrap(), "load pong did not run.");

    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(unsaved_question(&system), None);
    assert_eq!(loaded_name(&system), Some("Pong"));
    assert_eq!(read(&host(&fixture, "code/main.luau")), format!("a{MAIN}"));
    assert_eq!(read(&util), UTIL);
    let modified: Vec<String> = code(&system)
        .modified()
        .iter()
        .map(|document| document.path().to_string())
        .collect();
    assert_eq!(modified, ["code/util.luau"], "main.luau is saved and clean");
}

#[test]
fn an_outside_change_met_while_saving_is_settled_in_the_question() {
    let fixture = Fixture::new("sys-unsaved-conflict-overwrite");
    let main = host(&fixture, "code/main.luau");
    let system = two_edited(&fixture);
    std::fs::write(&main, "theirs").unwrap();
    let system = save_all(enter(press_break(system), "unload"));
    let dialog = system.unsaved_flow().unwrap().dialog();
    assert_eq!(dialog.title(), "FILE CONFLICT");
    assert_eq!(dialog.labels(), ["OVERWRITE DISK", "CANCEL"]);
    assert_eq!(dialog.focus(), 1);

    // Overwriting carries on with the other script, then the command.
    let system = system.update(&[InputEvent::Up { select: false }, InputEvent::Enter]);
    assert_eq!(unsaved_question(&system), None);
    assert_eq!(read(&main), format!("a{MAIN}"));
    assert_eq!(read(&host(&fixture, "code/util.luau")), format!("b{UTIL}"));
    assert_eq!(after(&system, "> unload"), ["Unloaded Pong 0.1.0"]);
}

#[test]
fn cancelling_a_conflict_keeps_earlier_saves_and_reports_them() {
    let fixture = Fixture::new("sys-unsaved-conflict-cancel");
    let util = host(&fixture, "code/util.luau");
    let system = two_edited(&fixture);
    std::fs::write(&util, "theirs").unwrap();
    let system = save_all(enter(press_break(system), "unload"));
    let message = question_message(&system);
    assert_eq!(message[1], "Saved code/main.luau before stopping.");

    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(unsaved_question(&system), None);
    assert_eq!(
        after(&system, "> unload"),
        ["Saved code/main.luau before stopping. unload did not run."]
    );
    assert_eq!(loaded_name(&system), Some("Pong"));
    assert_eq!(read(&util), "theirs");
    assert_eq!(code(&system).modified().len(), 1);
}

#[test]
fn quitting_without_unsaved_edits_needs_no_question() {
    let fixture = Fixture::new("sys-unsaved-quit-clean");
    let mut system = press_break(loaded_with(&fixture, &[]));
    system.request_exit();
    assert!(system.exit_confirmed());
}

#[test]
fn quitting_from_the_workspace_asks_and_cancel_stays() {
    let fixture = Fixture::new("sys-unsaved-quit-cancel");
    let mut system = two_edited(&fixture);
    system.request_exit();
    assert!(!system.exit_confirmed());
    let mut system = system.update(&[]);
    assert_eq!(unsaved_question(&system), asks_about_unsaved());
    assert!(is_workspace(&system));
    // A second request while the question is open changes nothing.
    system.request_exit();

    let system = press_break(system);
    assert_eq!(unsaved_question(&system), None);
    assert!(!system.exit_confirmed());
    assert!(is_workspace(&system));
    assert_eq!(code(&system).modified().len(), 2);
}

#[test]
fn quitting_with_discard_writes_nothing_and_with_save_all_writes_everything() {
    let fixture = Fixture::new("sys-unsaved-quit-discard");
    let mut system = two_edited(&fixture);
    system.request_exit();
    let system = discard(system.update(&[]));
    assert!(system.exit_confirmed());
    assert_eq!(read(&host(&fixture, "code/main.luau")), MAIN);

    let fixture = Fixture::new("sys-unsaved-quit-save");
    let mut system = two_edited(&fixture);
    system.request_exit();
    let system = save_all(system.update(&[]));
    assert!(system.exit_confirmed());
    assert_eq!(read(&host(&fixture, "code/main.luau")), format!("a{MAIN}"));
    assert_eq!(read(&host(&fixture, "code/util.luau")), format!("b{UTIL}"));
}

#[test]
fn a_failed_save_while_quitting_does_not_quit() {
    let fixture = Fixture::new("sys-unsaved-quit-failed");
    let main = host(&fixture, "code/main.luau");
    let mut system = two_edited(&fixture);
    system.request_exit();
    let system = system.update(&[]);
    let writable = read_only(&main);
    let system = save_all(system);
    std::fs::set_permissions(&main, writable).unwrap();
    assert_eq!(
        question_message(&system).last().map(String::as_str),
        Some("Glaux did not quit.")
    );

    let system = system.update(&[InputEvent::Enter]);
    assert!(!system.exit_confirmed());
    assert_eq!(code(&system).modified().len(), 2);
}

#[test]
fn quitting_while_a_cartridge_runs_pauses_it_and_asks_over_the_shell() {
    let fixture = Fixture::new("sys-unsaved-quit-running");
    let system = typed(press_break(loaded_with(&fixture, &[])), "x");
    let mut system = enter(press_break(system), "run");
    assert!(is_running(&system));
    system.request_exit();
    let system = system.update(&[]);
    assert!(is_paused_shell(&system));
    assert_eq!(unsaved_question(&system), asks_about_unsaved());

    let system = press_break(system);
    assert!(!system.exit_confirmed());
    assert!(is_paused_shell(&system), "the cartridge stays paused");
    let system = enter(system, "resume");
    assert!(is_running(&system));
}

#[test]
fn quitting_from_the_shell_asks_over_it() {
    let fixture = Fixture::new("sys-unsaved-quit-shell");
    let mut system = press_break(two_edited(&fixture));
    system.request_exit();
    let system = discard(system.update(&[]));
    assert!(system.exit_confirmed());
}
