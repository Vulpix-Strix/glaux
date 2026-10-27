//! Running the cartridge from the Code view under the debugger: where it
//! stops, what the editor shows and allows meanwhile, and how it goes on.

use super::code_run::in_code;
use super::rhai::shell_with_cartridge_root;
use super::*;
use crate::cartridge::fixture::write_rhai_cartridge;
use crate::system::editor_settings::EditorSettings;
use pretty_assertions::assert_eq;

const SCRIPT: &str = "function tick()
  local x = 41
  local y = x + 1
  log.info(y)
end
";

fn is_debugging(system: &System) -> bool {
    matches!(system.state(), SystemState::Debugging { .. })
}

fn is_workspace(system: &System) -> bool {
    matches!(system.state(), SystemState::Workspace { .. })
}

/// Sets the breakpoints on `lines`, counted from 1, of the active script.
fn with_breakpoints(mut system: System, lines: &[usize]) -> System {
    let code = system.code.as_mut().unwrap();
    let id = code.tabs().active().unwrap();
    for &line in lines {
        code.toggle_breakpoint(id, line - 1);
    }
    system
}

/// Starts the cartridge under the debugger and ticks until it stops.
fn debugged(system: System) -> System {
    let mut system = system.update(&[InputEvent::Debug]);
    for _ in 0..5 {
        if !matches!(system.state(), SystemState::Running { .. }) {
            break;
        }
        system = system.update(&[]);
    }
    system
}

/// The script and line, counted from 1, the stopped cartridge is at.
fn stopped_at(system: &System) -> (String, u32) {
    let (path, line) = system
        .workspace
        .stop_view()
        .expect("a stop to show")
        .place()
        .expect("a frame in a script");
    (path.to_owned(), line)
}

fn variables(system: &System) -> Vec<(String, String)> {
    system
        .workspace
        .stop_view()
        .unwrap()
        .rows()
        .into_iter()
        .map(|row| (row.name, row.value))
        .collect()
}

#[test]
fn a_breakpoint_stops_the_run_and_shows_the_line_in_the_editor() {
    let fixture = Fixture::new("sys-debug-stop");
    let system = with_breakpoints(in_code(&fixture, SCRIPT), &[3]);

    let system = system.update(&[InputEvent::Debug]);
    assert!(is_running(&system), "it runs until it reaches the line");
    let system = system.update(&[]);
    assert!(is_debugging(&system));
    assert_eq!(system.input_mode(), InputMode::Workspace);
    assert_eq!(stopped_at(&system), ("code/main.luau".to_owned(), 3));
    assert_eq!(variables(&system), [("x".to_owned(), "41".to_owned())]);
    assert!(system.code.as_ref().unwrap().is_read_only());
    assert_eq!(caret_line_of(&system), 3);
}

fn caret_line_of(system: &System) -> usize {
    let code = system.code.as_ref().unwrap();
    let document = code.active_document().unwrap();
    let caret = code.tabs().selection(document.id()).unwrap().caret().byte();
    document.line_of(caret) + 1
}

#[test]
fn a_run_without_breakpoints_under_the_debugger_just_runs() {
    let fixture = Fixture::new("sys-debug-no-stop");
    let system = debugged(in_code(&fixture, SCRIPT));
    assert!(is_running(&system));
    let (_, lines) = logged_per_tick(system, &[&[], &[]]);
    assert_eq!(lines, ["42", "42"]);
}

#[test]
fn the_scripts_cannot_be_edited_while_stopped_but_can_be_read() {
    let fixture = Fixture::new("sys-debug-read-only");
    let system = debugged(with_breakpoints(in_code(&fixture, SCRIPT), &[3]));
    let before = code_text(&system);
    let system = system.update(&[InputEvent::Text('x'), InputEvent::Enter, InputEvent::Undo]);
    assert_eq!(code_text(&system), before);
    assert!(!system.code.as_ref().unwrap().is_modified());
    assert!(is_debugging(&system));

    let system = system.update(&[InputEvent::Down { select: false }]);
    assert_eq!(caret_line_of(&system), 4, "the caret still moves");
}

fn code_text(system: &System) -> String {
    let code = system.code.as_ref().unwrap();
    code.active_document().unwrap().text().to_owned()
}

#[test]
fn continuing_finishes_the_tick_and_the_game_goes_on_until_the_next_stop() {
    let fixture = Fixture::new("sys-debug-continue");
    let system = debugged(with_breakpoints(in_code(&fixture, SCRIPT), &[3]));

    let system = system.update(&[InputEvent::Run]);
    assert!(
        is_running(&system),
        "the tick finished and the game is back"
    );
    assert!(!system.code.as_ref().unwrap().is_read_only());
    assert_eq!(transcript(&system).last().map(String::as_str), Some("42"));

    let system = system.update(&[]);
    assert!(
        is_debugging(&system),
        "the next tick reaches the line again"
    );
    assert_eq!(stopped_at(&system).1, 3);
}

#[test]
fn stepping_moves_to_the_next_line_and_shows_what_changed() {
    let fixture = Fixture::new("sys-debug-step");
    let system = debugged(with_breakpoints(in_code(&fixture, SCRIPT), &[2]));
    assert_eq!(stopped_at(&system).1, 2);
    assert!(variables(&system).is_empty(), "x is not declared yet");

    let system = system.update(&[InputEvent::StepOver]);
    assert!(is_debugging(&system));
    assert_eq!(stopped_at(&system).1, 3);
    assert_eq!(variables(&system), [("x".to_owned(), "41".to_owned())]);

    let system = system.update(&[InputEvent::StepInto]);
    assert_eq!(stopped_at(&system).1, 4);
    assert_eq!(
        variables(&system),
        [
            ("x".to_owned(), "41".to_owned()),
            ("y".to_owned(), "42".to_owned())
        ]
    );
    assert_eq!(caret_line_of(&system), 4, "the editor follows the step");
}

#[test]
fn stepping_does_not_fill_the_places_back_goes_to() {
    let fixture = Fixture::new("sys-debug-history");
    let system = debugged(with_breakpoints(in_code(&fixture, SCRIPT), &[2]));
    let system = system
        .update(&[InputEvent::StepOver])
        .update(&[InputEvent::StepOver]);
    assert!(!system.code.as_ref().unwrap().can_go_back());
}

#[test]
fn stopping_returns_to_the_editor_with_the_scripts_editable_again() {
    let fixture = Fixture::new("sys-debug-end");
    let system = debugged(with_breakpoints(in_code(&fixture, SCRIPT), &[3]));
    let system = system.update(&[InputEvent::Stop]);
    assert!(is_workspace(&system));
    assert!(!system.code.as_ref().unwrap().is_read_only());
    assert!(system.workspace.stop_view().is_none());

    let system = typed_text(system, "x");
    assert!(system.code.as_ref().unwrap().is_modified());
}

fn typed_text(system: System, text: &str) -> System {
    let events: Vec<InputEvent> = text.chars().map(InputEvent::Text).collect();
    system.update(&events)
}

#[test]
fn restarting_a_stopped_run_starts_it_again_under_the_debugger() {
    let fixture = Fixture::new("sys-debug-restart");
    let system = debugged(with_breakpoints(in_code(&fixture, SCRIPT), &[3]));
    let system = system.update(&[InputEvent::Restart]);
    assert!(is_running(&system));
    let system = system.update(&[]);
    assert!(is_debugging(&system), "still under the debugger");
}

#[test]
fn a_failure_after_continuing_returns_to_the_script_line_that_failed() {
    let fixture = Fixture::new("sys-debug-failure");
    let script = "function tick()\n  local x = 1\n  error('boom')\nend\n";
    let system = debugged(with_breakpoints(in_code(&fixture, script), &[2]));
    let system = system.update(&[InputEvent::Run]);
    assert!(is_workspace(&system));
    assert!(!system.code.as_ref().unwrap().is_read_only());
    assert_eq!(caret_line_of(&system), 3);
}

#[test]
fn a_breakpoint_in_boot_stops_the_start_itself() {
    let fixture = Fixture::new("sys-debug-boot");
    let script = "function boot()\n  local ready = true\nend\nfunction tick() end\n";
    let system = with_breakpoints(in_code(&fixture, script), &[2]);
    let system = system.update(&[InputEvent::Debug]);
    assert!(is_debugging(&system));
    assert_eq!(stopped_at(&system).1, 2);

    let system = system.update(&[InputEvent::Run]);
    assert!(is_running(&system), "the start finished");
}

#[test]
fn a_script_that_fails_to_start_under_the_debugger_returns_to_the_editor() {
    let fixture = Fixture::new("sys-debug-start-failure");
    let system = in_code(&fixture, "local = 1\n").update(&[InputEvent::Debug]);
    assert!(is_workspace(&system));
}

#[test]
fn unsaved_edits_are_saved_before_debugging_even_when_that_is_off() {
    let fixture = Fixture::new("sys-debug-save");
    let system = in_code(&fixture, SCRIPT).with_editor_settings(EditorSettings {
        save_before_running: false,
        ..EditorSettings::default()
    });
    let system = typed_text(system, "-- note\n");
    let edited = code_text(&system);
    let system = system.update(&[InputEvent::Debug]);
    let on_disk = std::fs::read_to_string(fixture.root().join("pong/code/main.luau")).unwrap();
    assert_eq!(on_disk, edited);
    assert!(!system.code.as_ref().unwrap().is_modified());
}

#[test]
fn a_breakpoint_set_while_stopped_reaches_the_run_before_it_goes_on() {
    let fixture = Fixture::new("sys-debug-live-breakpoint");
    let system = debugged(with_breakpoints(in_code(&fixture, SCRIPT), &[2]));
    let system = system.update(&[
        InputEvent::Down { select: false },
        InputEvent::Down { select: false },
        InputEvent::ToggleBreakpoint,
    ]);
    assert_eq!(system.code.as_ref().unwrap().breakpoints().len(), 2);
    let system = system.update(&[InputEvent::Run]);
    assert!(is_debugging(&system), "it stopped again in the same tick");
    assert_eq!(stopped_at(&system).1, 4);
}

#[test]
fn escape_does_not_leave_a_stopped_run() {
    let fixture = Fixture::new("sys-debug-escape");
    let system = debugged(with_breakpoints(in_code(&fixture, SCRIPT), &[3]));
    let system = system.update(&[InputEvent::Break]);
    assert!(is_debugging(&system));
}

#[test]
fn the_menu_offers_the_steps_and_nothing_that_edits() {
    let fixture = Fixture::new("sys-debug-menu");
    let system = in_code(&fixture, SCRIPT);
    let editing = system.menu_state();
    assert!(editing.debug && editing.toggle_breakpoint && !editing.step_over);

    let system = debugged(with_breakpoints(system, &[3]));
    let stopped = system.menu_state();
    assert!(stopped.step_over && stopped.step_into && stopped.step_out);
    assert!(stopped.run && stopped.stop && stopped.restart && stopped.find);
    assert!(!stopped.debug);
    assert!(!stopped.save && !stopped.undo && !stopped.cut && !stopped.paste);
}

#[test]
fn a_stopped_rhai_cartridge_shows_its_state_and_goes_on() {
    let fixture = Fixture::new("sys-debug-rhai");
    write_rhai_cartridge(
        &fixture.root().join("pong"),
        "Pong",
        "fn boot() {\n  this.hp = 3;\n}\nfn tick() {\n  let x = 41;\n  let y = x + 1;\n  log::info(y);\n}\n",
    );
    let system = press_break(enter(shell_with_cartridge_root(&fixture), "load pong"));
    let system = debugged(with_breakpoints(system, &[6]));
    assert!(is_debugging(&system));
    assert_eq!(stopped_at(&system), ("code/main.rhai".to_owned(), 6));
    let vars = variables(&system);
    assert_eq!(vars[0], ("this".to_owned(), "{..}".to_owned()));
    assert_eq!(vars[1], ("x".to_owned(), "41".to_owned()));

    let system = system.update(&[InputEvent::Run]);
    assert!(is_running(&system));
    assert_eq!(transcript(&system).last().map(String::as_str), Some("42"));
}

#[test]
fn the_variables_of_a_selected_frame_and_the_entries_of_a_table_are_fetched_on_demand() {
    let fixture = Fixture::new("sys-debug-panel");
    let script = "local function inner()\n  local depth = 2\n  return depth\nend\nfunction tick()\n  local cfg = {speed = 3, pos = {x = 1}}\n  inner()\nend\n";
    let system = debugged(with_breakpoints(in_code(&fixture, script), &[3]));
    assert_eq!(stopped_at(&system), ("code/main.luau".to_owned(), 3));
    let layout = system.workspace.stop_panel().expect("the panel is shown");

    // Select the caller: its place and variables replace the first frame's.
    let system = system.update(&click(centre_of(layout.frame_row(1))));
    assert_eq!(stopped_at(&system).1, 7);
    assert_eq!(caret_line_of(&system), 7);
    let in_caller: Vec<_> = variables(&system).into_iter().map(|(n, _)| n).collect();
    assert_eq!(in_caller, ["cfg", "inner"]);

    // Open the table, then the table inside it.
    let system = system.update(&click(centre_of(layout.local_row(0))));
    let rows = variables(&system);
    let opened: Vec<_> = rows.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        opened.contains(&"speed") && opened.contains(&"pos"),
        "{opened:?}"
    );
}

fn centre_of(rect: crate::system::workspace::Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

#[test]
fn a_debugged_run_can_be_paused_and_resumed_like_any_other() {
    let fixture = Fixture::new("sys-debug-pause");
    let system = debugged(in_code(&fixture, SCRIPT));
    assert!(is_running(&system));
    let system = press_break(system);
    assert!(is_paused_game(&system));
    let system = press_break(system);
    assert!(is_running(&system));
    let (_, lines) = logged_per_tick(system, &[&[]]);
    assert_eq!(lines, ["42"]);
}

#[test]
fn the_log_panel_shows_what_the_cartridge_logged_before_it_stopped() {
    let fixture = Fixture::new("sys-debug-logs");
    let script = "function tick()\n  log.info('before')\n  local x = 1\nend\n";
    let system = debugged(with_breakpoints(in_code(&fixture, script), &[3]));
    assert!(is_debugging(&system));
    assert!(transcript(&system).contains(&"before".to_owned()));

    let system = system.update(&[InputEvent::ToggleLogs]);
    assert!(system.log_panel.is_open());
    assert!(is_debugging(&system));
}
