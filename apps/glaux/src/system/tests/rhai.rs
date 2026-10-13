//! Rhai cartridges through the whole system: creation, the experimental
//! warning, and the run, pause, resume and stop lifecycle.

use super::*;
use crate::cartridge::fixture::write_rhai_cartridge;
use crate::system::shell::EXPERIMENTAL_LANGUAGE_WARNING;
use pretty_assertions::assert_eq;

/// The experimental warning as the transcript shows it.
fn warning() -> String {
    format!("WARN: {EXPERIMENTAL_LANGUAGE_WARNING}")
}

/// A system at the shell whose cartridge root holds a Rhai `Pong`
/// cartridge at `/pong` with entry `script`, loaded and run.
fn started_rhai(fixture: &Fixture, script: &str) -> System {
    write_rhai_cartridge(&fixture.root().join("pong"), "Pong", script);
    let system = shell_with_cartridge_root(fixture);
    enter(enter(system, "load pong"), "run")
}

/// A system at the shell navigating `fixture`'s cartridge root.
fn shell_with_cartridge_root(fixture: &Fixture) -> System {
    let mut system = System::new(
        info(),
        sounds(),
        video_defaults(),
        fixture.storage(),
        fake_rom().font(),
        fake_rom().palette().clone(),
    );
    while matches!(system.state(), SystemState::Boot { .. }) {
        system = system.update(&[]);
    }
    system
}

const COUNTER: &str = r#"
    print("entry");
    fn boot() { print("boot"); this.n = 0; }
    fn tick() { this.n += 1; log::info(`tick ${this.n}`); }
"#;

#[test]
fn running_a_rhai_cartridge_warns_on_every_run() {
    let fixture = Fixture::new("sys-rhai-warn");
    let system = started_rhai(&fixture, COUNTER);
    assert!(is_running(&system));
    assert_eq!(
        after(&system, "> run"),
        [
            "Running Pong 0.1.0".to_owned(),
            warning(),
            "entry".into(),
            "boot".into()
        ]
    );

    let system = enter(stop_cartridge(system), "run");
    assert_eq!(
        after(&system, "> run")[..2],
        ["Running Pong 0.1.0".to_owned(), warning()]
    );
    let warnings = transcript(&system)
        .iter()
        .filter(|line| **line == warning())
        .count();
    assert_eq!(warnings, 2);
}

#[test]
fn pausing_keeps_the_rhai_state_and_resuming_continues_it() {
    let fixture = Fixture::new("sys-rhai-pause");
    let system = press_break(ticks(started_rhai(&fixture, COUNTER), 3));
    assert!(is_paused_game(&system));
    let system = ticks(system, 5);
    let system = press_break(system);
    assert!(is_running(&system));
    let system = ticks(system, 1);
    assert_eq!(transcript(&system).last().unwrap(), "tick 4");
    assert_eq!(
        transcript(&system)
            .iter()
            .filter(|line| *line == "boot")
            .count(),
        1,
        "boot ran only once"
    );
}

#[test]
fn stopping_a_rhai_cartridge_disposes_of_its_state() {
    let fixture = Fixture::new("sys-rhai-stop");
    let system = stop_cartridge(ticks(started_rhai(&fixture, COUNTER), 3));
    assert_eq!(loaded_name(&system), Some("Pong"));
    let system = ticks(enter(system, "run"), 1);
    assert_eq!(transcript(&system).last().unwrap(), "tick 1");
}

#[test]
fn rhai_failures_return_to_the_shell_with_located_errors() {
    let fixture = Fixture::new("sys-rhai-fail");
    for (script, error) in [
        (
            "fn tick() { print(\"t\");\n throw \"boom\"; }",
            "! Runtime error in tick: code/main.rhai:2: boom",
        ),
        (
            "fn tick() { print(\"t\"); }\nfn draw() { pset(0, 0, 999); }",
            "! Runtime error in draw: code/main.rhai:2: bad argument #3 to 'pset' (palette index 0-255 expected, got 999)",
        ),
        (
            "fn tick() { print(\"t\"); try { loop {} } catch {} }",
            "! Execution limit exceeded in tick",
        ),
    ] {
        let system = ticks(started_rhai(&fixture, script), 1);
        assert_eq!(
            after(&system, "> run"),
            [
                "Running Pong 0.1.0".to_owned(),
                warning(),
                "t".into(),
                error.into()
            ],
            "{script}"
        );
        assert_eq!(loaded_name(&system), Some("Pong"), "{script}");
    }

    let system = started_rhai(&fixture, "fn boot() {\n  x +; }");
    assert_eq!(
        after(&system, "> run"),
        ["! Syntax error: code/main.rhai:2: Unexpected ';'"]
    );
}

#[test]
fn new_rhai_cartridges_warn_before_the_welcome() {
    let fixture = Fixture::new("sys-rhai-new");
    let system = enter(shell_with_cartridge_root(&fixture), "new owl --lang rhai");
    let lines = after(&system, "> new owl --lang rhai");
    assert_eq!(lines[0], warning());
    assert_eq!(lines[1], "Your new cartridge is ready!");
    assert_eq!(loaded_name(&system), Some("Owl"));

    let system = enter(system, "new hoot");
    assert!(
        !after(&system, "> new hoot").contains(&warning()),
        "Luau cartridges are not experimental"
    );
}

#[test]
fn the_rhai_starter_runs_and_draws() {
    let fixture = Fixture::new("sys-rhai-starter");
    let system = enter(
        enter(shell_with_cartridge_root(&fixture), "new owl --lang rhai"),
        "run",
    );
    // Hoot once, then let the hop and the speech bubble play out.
    let system = system.update(&[InputEvent::ButtonDown(Button::A)]);
    let system = system.update(&[InputEvent::ButtonUp(Button::A)]);
    let system = ticks(system, 30);
    assert!(is_running(&system), "{:?}", transcript(&system));
    assert_eq!(
        after(&system, "> run"),
        ["Running Owl 0.1.0".to_owned(), warning()]
    );
    let SystemState::Running { running, .. } = system.state() else {
        unreachable!();
    };
    let rom = fake_rom();
    let mut fb = Framebuffer::new();
    running.render(&mut fb, rom.font());
    let sky = fb.pixel(0, 0).unwrap();
    assert_ne!(sky, 0, "the sky is drawn");
    let colors: std::collections::BTreeSet<_> = (0..240)
        .flat_map(|y| (0..320).map(move |x| (x, y)))
        .filter_map(|(x, y)| fb.pixel(x, y))
        .collect();
    assert!(
        colors.len() >= 8,
        "the scene has its many colors: {colors:?}"
    );
}
