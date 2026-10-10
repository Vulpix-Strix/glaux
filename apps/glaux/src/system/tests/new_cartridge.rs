use super::*;
use pretty_assertions::assert_eq;

const WELCOME: [&str; 7] = [
    "Your new cartridge is ready!",
    "My Game",
    "",
    "Every great game starts somewhere.",
    "This one starts with you.",
    "",
    "Try 'run' to see your first creation.",
];

#[test]
fn new_loads_the_created_cartridge_without_running_it() {
    let fixture = Fixture::new("sys-new");
    let system = enter(shell_with_cartridge(&fixture, ""), "new my-game");
    assert!(matches!(system.state(), SystemState::Shell { .. }));
    assert_eq!(loaded_name(&system), Some("My Game"));
    assert_eq!(after(&system, "> new my-game"), WELCOME);

    let system = enter(system, "info");
    assert_eq!(transcript(&system).last().unwrap(), "STATE       LOADED");
}

#[test]
fn a_failed_new_announces_nothing_and_keeps_the_loaded_cartridge() {
    let fixture = Fixture::new("sys-new-fail");
    let system = enter(shell_with_cartridge(&fixture, ""), "load pong");
    assert_eq!(after(&system, "> load pong"), ["Loaded Pong 0.1.0"]);
    let system = enter(system, "new pong");
    assert_eq!(after(&system, "> new pong"), ["! /pong already exists"]);
    assert_eq!(loaded_name(&system), Some("Pong"));
}

#[test]
fn new_then_run_starts_the_starter_cartridge() {
    let fixture = Fixture::new("sys-new-run");
    let system = enter(
        enter(shell_with_cartridge(&fixture, ""), "new my-game"),
        "run",
    );
    assert!(is_running(&system));
    let system = ticks(system, 120);
    assert!(is_running(&system));
    assert_eq!(after(&system, "> run"), ["Running My Game 0.1.0"]);
}

/// Per-tick input that presses `button` on the first tick, releases it
/// after `held` ticks, and lasts `total` ticks.
fn hold(button: Button, held: usize, total: usize) -> Vec<Vec<InputEvent>> {
    let mut inputs = vec![Vec::new(); total];
    inputs[0].push(InputEvent::ButtonDown(button));
    if held < total {
        inputs[held].push(InputEvent::ButtonUp(button));
    }
    inputs
}

/// Per-tick input that taps `button` (pressed for one tick) at each of
/// `at`, lasting `total` ticks.
fn taps(button: Button, at: &[usize], total: usize) -> Vec<Vec<InputEvent>> {
    let mut inputs = vec![Vec::new(); total];
    for &tick in at {
        inputs[tick].push(InputEvent::ButtonDown(button));
        inputs[tick + 1].push(InputEvent::ButtonUp(button));
    }
    inputs
}

/// The frame a freshly created and started starter cartridge shows after
/// one tick per entry of `inputs`. `name` keeps each run's storage apart.
///
/// Runs compared with each other last the same number of ticks, so
/// anything the starter animates by time alone matches between them.
fn starter_frame(name: &str, inputs: &[Vec<InputEvent>]) -> Framebuffer {
    let fixture = Fixture::new(name);
    let mut system = enter(enter(shell_with_cartridge(&fixture, ""), "new owl"), "run");
    for input in inputs {
        system = system.update(input);
    }
    assert!(is_running(&system), "{:?}", transcript(&system));
    render(&system)
}

/// The leftmost and rightmost columns where `a` and `b` differ.
fn differing_columns(a: &Framebuffer, b: &Framebuffer) -> Option<(i32, i32)> {
    let columns: Vec<i32> = (0..WIDTH as i32)
        .filter(|&x| (0..HEIGHT as i32).any(|y| a.pixel(x, y) != b.pixel(x, y)))
        .collect();
    Some((*columns.first()?, *columns.last()?))
}

/// The topmost and bottommost rows where `a` and `b` differ.
fn differing_rows(a: &Framebuffer, b: &Framebuffer) -> Option<(i32, i32)> {
    let rows: Vec<i32> = (0..HEIGHT as i32)
        .filter(|&y| (0..WIDTH as i32).any(|x| a.pixel(x, y) != b.pixel(x, y)))
        .collect();
    Some((*rows.first()?, *rows.last()?))
}

#[test]
fn the_starter_owl_follows_the_arrows() {
    let idle = starter_frame("sys-starter-idle", &vec![Vec::new(); 30]);
    let right = starter_frame("sys-starter-right", &hold(Button::Right, 20, 30));
    let left = starter_frame("sys-starter-left", &hold(Button::Left, 20, 30));
    let (right_min, right_max) = differing_columns(&idle, &right).unwrap();
    let (left_min, left_max) = differing_columns(&idle, &left).unwrap();
    assert!(
        right_min + right_max > left_min + left_max,
        "right {right_min}..{right_max}, left {left_min}..{left_max}"
    );
}

#[test]
fn the_starter_owl_stays_on_screen() {
    let stopped = starter_frame("sys-starter-stop", &hold(Button::Left, 300, 600));
    let held = starter_frame("sys-starter-held", &hold(Button::Left, 600, 600));
    assert!(stopped == held, "holding Left longer moved the owl further");

    let idle = starter_frame("sys-starter-idle-long", &vec![Vec::new(); 600]);
    let (min, _) = differing_columns(&idle, &held).unwrap();
    assert_eq!(min, 0, "the owl rests against the left edge");
}

#[test]
fn the_starter_owl_reacts_to_a() {
    // Mid-hop, the owl has moved and a speech bubble shows: far more than
    // one line of text changes.
    let idle = starter_frame("sys-starter-a-idle", &vec![Vec::new(); 10]);
    let hopping = starter_frame("sys-starter-a-press", &taps(Button::A, &[0], 10));
    let (top, bottom) = differing_rows(&idle, &hopping).expect("A changed nothing");
    assert!(bottom - top > 2 * CELL_H, "rows {top}..={bottom}");

    // Long after the hop and the bubble, only the greeting line still
    // differs, and it keeps counting how many times the owl was greeted.
    let idle = starter_frame("sys-starter-a-idle-long", &vec![Vec::new(); 150]);
    let once = starter_frame("sys-starter-a-once", &taps(Button::A, &[0], 150));
    let twice = starter_frame("sys-starter-a-twice", &taps(Button::A, &[0, 4], 150));
    for (a, b, what) in [
        (&idle, &once, "idle and once"),
        (&once, &twice, "once and twice"),
    ] {
        let (top, bottom) = differing_rows(a, b).expect(what);
        assert!(bottom - top < CELL_H, "{what}: rows {top}..={bottom}");
    }
}
