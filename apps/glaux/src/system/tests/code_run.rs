//! Running the cartridge from the Code view: where a run returns to, what
//! is saved before it, and how its failures reach the editor and the log.

use super::rhai::shell_with_cartridge_root;
use super::*;
use crate::cartridge::fixture::write_rhai_cartridge;
use crate::cartridge::source::SourcePath;
use crate::system::editor_settings::EditorSettings;
use crate::system::logs::{Category, Severity, SourceLocation};
use pretty_assertions::assert_eq;

/// The workspace on its Code view with `script` as `code/main.luau`.
fn in_code(fixture: &Fixture, script: &str) -> System {
    press_break(enter(shell_with_cartridge(fixture, script), "load pong"))
}

fn typed(system: System, text: &str) -> System {
    let events: Vec<InputEvent> = text.chars().map(InputEvent::Text).collect();
    system.update(&events)
}

fn is_workspace(system: &System) -> bool {
    matches!(system.state(), SystemState::Workspace { .. })
}

fn code_text(system: &System) -> String {
    let code = system.code.as_ref().unwrap();
    code.active_document().unwrap().text().to_owned()
}

fn main_on_disk(fixture: &Fixture) -> String {
    std::fs::read_to_string(fixture.root().join("pong/code/main.luau")).unwrap()
}

/// The line, counted from 1, the caret of the active script is on.
fn caret_line(system: &System) -> usize {
    let code = system.code.as_ref().unwrap();
    let document = code.active_document().unwrap();
    let caret = code.tabs().selection(document.id()).unwrap().caret().byte();
    document.line_of(caret) + 1
}

fn failure_location(system: &System) -> Option<SourceLocation> {
    system.logs.terminal().unwrap().location.clone()
}

#[test]
fn a_run_from_code_saves_runs_and_returns_to_the_editor_as_it_was() {
    let fixture = Fixture::new("sys-run-from-code");
    let script = "function tick() log.info(btn(Button.A)) end\n";
    let system = typed(in_code(&fixture, script), "log.info('saved')\n");
    let edited = code_text(&system);

    let system = system.update(&[InputEvent::Run]);
    assert!(is_running(&system));
    assert_eq!(main_on_disk(&fixture), edited, "the edit was saved first");
    assert!(transcript(&system).contains(&"saved".to_owned()));

    // The panel takes the keys while it is open and the game keeps going.
    let system = system.update(&[InputEvent::ToggleLogs]);
    assert!(system.log_panel.is_open());
    let (system, lines) = logged_per_tick(system, &[&[InputEvent::ButtonDown(Button::A)], &[]]);
    assert_eq!(lines, ["false", "false"]);
    let system = system.update(&[InputEvent::ToggleLogs]);
    assert!(!system.log_panel.is_open());
    let (system, lines) = logged_per_tick(system, &[&[], &[InputEvent::ButtonDown(Button::A)]]);
    assert_eq!(lines, ["false", "true"], "nothing the panel took stuck");

    // Enter on the pause screen stops the run and shows the editor again.
    let system = open_shell_or_stop(press_break(system));
    assert!(is_workspace(&system));
    assert_eq!(code_text(&system), edited);
    assert!(!code_is_modified(&system));
    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(code_text(&system), script, "undo history survived the run");
}

fn open_shell_or_stop(system: System) -> System {
    assert!(is_paused_game(&system));
    system.update(&[InputEvent::Enter])
}

fn code_is_modified(system: &System) -> bool {
    system.code.as_ref().unwrap().is_modified()
}

#[test]
fn a_runtime_error_returns_to_the_script_line_and_the_log_keeps_it() {
    let fixture = Fixture::new("sys-run-error-line");
    let script = "local x = 1\n\nfunction tick() error('boom') end\n";
    let system = ticks(in_code(&fixture, script).update(&[InputEvent::Run]), 1);

    assert!(is_workspace(&system));
    assert_eq!(caret_line(&system), 3);
    let record = system.logs.terminal().unwrap();
    assert_eq!(record.severity, Severity::Error);
    assert_eq!(record.category, Category::Runtime);
    assert_eq!(
        failure_location(&system),
        Some(SourceLocation {
            path: SourcePath::parse("code/main.luau").unwrap(),
            line: 3
        })
    );

    // From the editor the log can take the user back to the line.
    let system = system.update(&[InputEvent::DocumentStart { select: false }]);
    assert_eq!(caret_line(&system), 1);
    let system = system.update(&[InputEvent::ToggleLogs, InputEvent::Enter]);
    assert!(!system.log_panel.is_open());
    assert_eq!(caret_line(&system), 3);
}

#[test]
fn an_error_without_a_position_is_not_given_one() {
    let fixture = Fixture::new("sys-run-error-unlocated");
    let script = "local x = 1\n\nfunction tick() while true do end end\n";
    let system = ticks(in_code(&fixture, script).update(&[InputEvent::Run]), 1);

    assert!(is_workspace(&system));
    assert_eq!(failure_location(&system), None);
    assert_eq!(caret_line(&system), 1, "the caret was not moved");
}

#[test]
fn a_rhai_runtime_error_is_located_too() {
    let fixture = Fixture::new("sys-run-error-rhai");
    write_rhai_cartridge(
        &fixture.root().join("pong"),
        "Pong",
        "let x = 1;\n\nfn tick() {\n    throw \"boom\";\n}\n",
    );
    let system = press_break(enter(shell_with_cartridge_root(&fixture), "load pong"));
    let system = ticks(system.update(&[InputEvent::Run]), 1);

    assert!(is_workspace(&system));
    let location = failure_location(&system).unwrap();
    assert_eq!(location.path.as_str(), "code/main.rhai");
    assert_eq!(location.line, 4);
    assert_eq!(caret_line(&system), 4);
}

#[test]
fn a_save_that_fails_cancels_the_run() {
    let fixture = Fixture::new("sys-run-save-fails");
    let system = typed(in_code(&fixture, "-- main\n"), "-- mine\n");
    // The file changes behind the editor, so writing it would lose that.
    std::fs::write(fixture.root().join("pong/code/main.luau"), "-- theirs\n").unwrap();

    let system = system.update(&[InputEvent::Run]);
    assert!(is_workspace(&system), "nothing started");
    assert!(workspace::tests_support::modal(&system.workspace).is_some());
    assert_eq!(system.logs.terminal().unwrap().category, Category::Save);
    assert_eq!(main_on_disk(&fixture), "-- theirs\n");
    assert!(code_is_modified(&system), "the edit is still in memory");
}

#[test]
fn without_save_before_running_the_files_run_and_the_buffers_stay() {
    let fixture = Fixture::new("sys-run-no-save");
    let off = EditorSettings {
        save_before_running: false,
        ..EditorSettings::default()
    };
    let system = shell_with_cartridge(&fixture, "log.info('disk')\n").with_editor_settings(off);
    let system = press_break(enter(system, "load pong"));
    let system = typed(system, "log.info('buffer')\n");

    let system = system.update(&[InputEvent::Run]);
    assert!(is_running(&system));
    let lines = transcript(&system);
    assert!(lines.contains(&"disk".to_owned()), "{lines:?}");
    assert!(!lines.contains(&"buffer".to_owned()), "{lines:?}");
    assert_eq!(main_on_disk(&fixture), "log.info('disk')\n");

    let system = open_shell_or_stop(press_break(system));
    assert!(code_is_modified(&system));
    assert!(code_text(&system).starts_with("log.info('buffer')"));
}

#[test]
fn saving_before_running_covers_the_graphics_too() {
    let fixture = Fixture::new("sys-run-saves-graphics");
    let system = super::sprite_editor::edited(&fixture);
    assert!(system.sprites.as_ref().unwrap().is_modified());

    let system = system.update(&[InputEvent::Run]);
    assert!(is_running(&system));
    assert!(!system.sprites.as_ref().unwrap().is_modified());
}

#[test]
fn restart_starts_over_and_stop_returns_to_the_editor() {
    let fixture = Fixture::new("sys-run-restart");
    let system = ticks(in_code(&fixture, COUNTER).update(&[InputEvent::Run]), 3);
    let (system, lines) = logged_per_tick(system, &[&[]]);
    assert_eq!(lines, ["tick    4"]);

    // R on the pause screen restarts from a fresh VM.
    let system = press_break(system).update(&[InputEvent::Text('r')]);
    assert!(is_running(&system));
    let (system, lines) = logged_per_tick(system, &[&[]]);
    assert_eq!(lines, ["tick    1"]);

    // So does the menu's Restart, and Stop ends the run from outside.
    let system = system.update(&[InputEvent::Restart]);
    assert!(is_running(&system));
    let system = system.update(&[InputEvent::Stop]);
    assert!(is_workspace(&system));
    assert!(!system.log_panel.is_open());
}

#[test]
fn a_run_from_the_shell_still_ends_in_the_shell() {
    let fixture = Fixture::new("sys-run-from-shell");
    let system = stop_cartridge(started(&fixture, COUNTER));
    assert_eq!(loaded_name(&system), Some("Pong"));
}
