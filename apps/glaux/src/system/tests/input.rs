use super::*;
use pretty_assertions::assert_eq;

#[test]
fn buttons_reach_the_cartridge_per_tick() {
    let fixture = Fixture::new("sys-btn");
    let (_, lines) = logged_per_tick(
        started(&fixture, UP_STATE),
        &[
            &[down()],
            &[],
            &[up()],
            &[down()],
            &[up(), InputEvent::ButtonDown(Button::A)],
            &[down(), up()],
            &[],
        ],
    );
    assert_eq!(
        lines,
        [
            "true    true",
            "true    false",
            "false   false",
            "true    true",
            "false   false",
            "false   true",
            "false   false",
        ]
    );
}

#[test]
fn keys_reach_the_cartridge_per_tick() {
    let fixture = Fixture::new("sys-key");
    let (_, lines) = logged_per_tick(
        started(&fixture, SPACE_STATE),
        &[
            &[space_down()],
            // A platform auto-repeat that reaches the system anyway.
            &[space_down()],
            &[InputEvent::KeyUp(Key::Space)],
            &[space_down(), InputEvent::KeyUp(Key::Space)],
        ],
    );
    assert_eq!(
        lines,
        [
            "true    true",
            "true    false",
            "false   false",
            "false   true",
        ]
    );
}

#[test]
fn catch_up_ticks_press_only_once() {
    // As the main loop does: the first due tick takes the pending input
    // and the catch-up ticks after it take none.
    let fixture = Fixture::new("sys-btn-catch-up");
    let (_, lines) = logged_per_tick(
        started(&fixture, UP_STATE),
        &[&[down(), down()], &[], &[], &[]],
    );
    assert_eq!(
        lines,
        [
            "true    true",
            "true    false",
            "true    false",
            "true    false"
        ]
    );
}

#[test]
fn editing_events_do_not_press_buttons() {
    let fixture = Fixture::new("sys-btn-edit");
    let (_, lines) = logged_per_tick(
        started(&fixture, UP_STATE),
        &[&[InputEvent::Up, InputEvent::Text('z')]],
    );
    assert_eq!(lines, ["false   false"]);
}

#[test]
fn break_pauses_the_cartridge_despite_buttons() {
    let fixture = Fixture::new("sys-btn-break");
    let system = started(&fixture, UP_STATE).update(&[down(), InputEvent::Break]);
    assert!(is_paused_game(&system));
    assert_eq!(
        after(&system, "> run"),
        ["Running Pong 0.1.0", "Paused Pong 0.1.0"]
    );
}

#[test]
fn a_new_run_starts_with_buttons_released() {
    let fixture = Fixture::new("sys-btn-rerun");
    let system = started(&fixture, UP_STATE).update(&[down()]);
    let system = enter(stop_cartridge(system), "run");
    // Up stays physically held, so the host sends nothing new.
    let (system, lines) = logged_per_tick(system, &[&[]]);
    assert_eq!(lines, ["false   false"]);
    assert!(is_running(&system));
}

#[test]
fn input_mode_follows_the_active_context() {
    let fixture = Fixture::new("sys-input-mode");
    assert_eq!(new_system().input_mode(), InputMode::Text);
    let system = shell_with_cartridge(&fixture, "");
    assert_eq!(system.input_mode(), InputMode::Text);
    let system = enter(enter(system, "load pong"), "run");
    assert_eq!(system.input_mode(), InputMode::Game);
    let system = press_break(system);
    assert_eq!(system.input_mode(), InputMode::Text);
    let system = open_shell(system);
    assert_eq!(system.input_mode(), InputMode::Text);
    let system = enter(system, "resume");
    assert_eq!(system.input_mode(), InputMode::Game);
    let system = stop_cartridge(system);
    assert_eq!(system.input_mode(), InputMode::Text);
}
