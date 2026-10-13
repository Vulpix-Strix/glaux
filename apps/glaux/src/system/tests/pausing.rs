use super::*;
use pretty_assertions::assert_eq;

/// What the cartridge logged during `n` more ticks of `system`.
fn logged_ticks(system: System, n: usize) -> (System, Vec<String>) {
    let empty: Vec<&[InputEvent]> = vec![&[]; n];
    logged_per_tick(system, &empty)
}

#[test]
fn break_pauses_without_disposing_of_the_vm() {
    let fixture = Fixture::new("sys-pause");
    let system = press_break(ticks(started(&fixture, COUNTER), 3));
    assert!(is_paused_game(&system));
    assert_eq!(transcript(&system).last().unwrap(), "Paused Pong 0.1.0");
    let (system, lines) = logged_ticks(system, 5);
    assert_eq!(
        lines,
        Vec::<String>::new(),
        "a paused cartridge is not ticked"
    );
    assert!(is_paused_game(&system));
}

#[test]
fn break_on_the_pause_screen_resumes_the_same_vm() {
    let fixture = Fixture::new("sys-pause-resume");
    let system = press_break(press_break(ticks(started(&fixture, COUNTER), 3)));
    assert!(is_running(&system));
    assert_eq!(transcript(&system).last().unwrap(), "Resumed Pong 0.1.0");
    let (system, lines) = logged_ticks(system, 1);
    assert_eq!(lines, ["tick    4"]);
    assert_eq!(
        transcript(&system)
            .iter()
            .filter(|line| *line == "entry" || *line == "boot")
            .count(),
        2,
        "entry and boot ran only once"
    );
}

#[test]
fn paused_cartridges_keep_their_logical_time() {
    let fixture = Fixture::new("sys-pause-time");
    let script = "function tick() log.info(frame(), t() * 60) end";
    let system = press_break(ticks(started(&fixture, script), 2));
    let system = press_break(ticks(system, 10));
    let (_, lines) = logged_ticks(system, 1);
    assert_eq!(lines, ["2   2"]);
}

#[test]
fn the_pause_screen_shows_the_frozen_frame_under_the_overlay() {
    let fixture = Fixture::new("sys-pause-render");
    let script = "local n = 0 function tick() n += 1 end function draw() cls(n) end";
    let system = press_break(ticks(started(&fixture, script), 1));
    let rom = fake_rom();
    let SystemState::Paused { running, .. } = system.state() else {
        panic!("expected a paused cartridge");
    };
    let frozen = |running: &RunningCartridge| {
        let mut fb = Framebuffer::new();
        running.render(&mut fb, rom.font());
        fb
    };
    assert_frame_eq!(frozen(running), filled(1));

    let fb = render(&system);
    assert!(
        fb != filled(1),
        "the pause indicator is drawn over the frame"
    );
    let shaded = glaux_video::shade::shade_table(rom.palette(), 65);
    assert_eq!(fb.pixel(0, 0), Some(shaded[1]));
    let (x, y) = pause::PLATE_ORIGIN;
    assert_eq!(fb.pixel(x + 1, y + 1), Some(BACKGROUND));
    assert_eq!(fb.pixel(x + 5, y + 5), Some(TEXT));

    // Neither rendering nor time on the pause screen touches the frame
    // the cartridge drew.
    let system = ticks(system, pause::PULSE_TICKS as usize);
    let SystemState::Paused { running, .. } = system.state() else {
        panic!("expected a paused cartridge");
    };
    assert_frame_eq!(frozen(running), filled(1));
    let pulsed = render(&system);
    assert!(pulsed != fb, "the indicator animates");
    assert_eq!(pulsed.pixel(0, 0), Some(shaded[1]));
}

#[test]
fn enter_opens_the_shell_with_the_cartridge_paused() {
    let fixture = Fixture::new("sys-pause-shell");
    let system = open_shell(press_break(ticks(started(&fixture, COUNTER), 2)));
    assert!(is_paused_shell(&system));
    assert_eq!(system.input_mode(), InputMode::Text);
    assert_eq!(
        transcript(&system).last().unwrap(),
        "Type 'resume' to continue or 'stop' to end."
    );
    assert!(row_has_ink(&render(&system), 0), "the shell is drawn");

    // Break in the shell leaves the cartridge paused.
    let system = press_break(enter(system, "info"));
    assert!(is_paused_shell(&system));
    assert_eq!(transcript(&system).last().unwrap(), "STATE       PAUSED");
}

#[test]
fn events_after_enter_reach_the_shell() {
    let fixture = Fixture::new("sys-pause-shell-events");
    let mut events = vec![InputEvent::Enter];
    events.extend("info".chars().map(InputEvent::Text));
    events.push(InputEvent::Enter);
    let system = press_break(started(&fixture, "")).update(&events);
    assert_eq!(transcript(&system).last().unwrap(), "STATE       PAUSED");
}

#[test]
fn resume_from_the_shell_continues_the_same_vm() {
    let fixture = Fixture::new("sys-pause-shell-resume");
    let system = open_shell(press_break(ticks(started(&fixture, COUNTER), 2)));
    let system = enter(system, "resume");
    assert!(is_running(&system));
    assert_eq!(after(&system, "> resume"), ["Resumed Pong 0.1.0"]);
    let (_, lines) = logged_ticks(system, 1);
    assert_eq!(lines, ["tick    3"]);
}

#[test]
fn stop_disposes_of_the_paused_vm() {
    let fixture = Fixture::new("sys-pause-stop");
    let system = stop_cartridge(ticks(started(&fixture, COUNTER), 2));
    assert_eq!(loaded_name(&system), Some("Pong"));
    assert_eq!(after(&system, "> stop"), ["Stopped Pong 0.1.0"]);

    let system = enter(system, "run");
    assert_eq!(
        after(&system, "> run"),
        ["Running Pong 0.1.0", "entry", "boot"]
    );
    let (_, lines) = logged_ticks(system, 1);
    assert_eq!(lines, ["tick    1"]);
}

#[test]
fn load_and_run_cannot_replace_a_paused_cartridge() {
    let fixture = Fixture::new("sys-pause-refuse");
    let system = open_shell(press_break(ticks(started(&fixture, COUNTER), 2)));
    let system = enter(enter(system, "load pong"), "run");
    assert!(is_paused_shell(&system));
    assert_eq!(
        after(&system, "> load pong"),
        [
            "! Cartridge paused: resume or stop it first",
            "> run",
            "! Cartridge paused: resume or stop it first",
        ]
    );
    let (_, lines) = logged_ticks(enter(system, "resume"), 1);
    assert_eq!(lines, ["tick    3"]);
}

#[test]
fn unload_cannot_dispose_of_a_paused_cartridge() {
    let fixture = Fixture::new("sys-pause-unload");
    let system = open_shell(press_break(ticks(started(&fixture, COUNTER), 2)));
    let system = enter(system, "unload");
    assert!(is_paused_shell(&system));
    assert_eq!(
        after(&system, "> unload"),
        ["! Cartridge paused: resume or stop it first"]
    );

    let (_, lines) = logged_ticks(enter(system, "resume"), 1);
    assert_eq!(lines, ["tick    3"]);
}

#[test]
fn a_runtime_failure_after_resuming_is_not_resumable() {
    let fixture = Fixture::new("sys-pause-fail");
    let script = r#"local n = 0 function tick() n += 1 if n == 2 then error("boom") end end"#;
    let system = press_break(press_break(ticks(started(&fixture, script), 1)));
    let system = ticks(system, 1);
    assert_eq!(loaded_name(&system), Some("Pong"));
    assert_eq!(
        transcript(&system).last().unwrap(),
        "! Runtime error in tick: code/main.luau:1: boom"
    );
    let system = enter(system, "resume");
    assert_eq!(after(&system, "> resume"), ["! No paused cartridge"]);
    assert_eq!(loaded_name(&system), Some("Pong"));
}

#[test]
fn scrollback_and_history_survive_a_pause() {
    let fixture = Fixture::new("sys-pause-history");
    let system = enter(shell_with_cartridge(&fixture, COUNTER), "load pong");
    let system = ticks(enter(system, "run"), 1);
    let system = open_shell(press_break(system));
    let system = system.update(&[InputEvent::Up { select: false }]);
    let SystemState::Paused { shell, .. } = system.state() else {
        panic!("expected a paused cartridge");
    };
    let lines = shell.transcript();
    assert!(lines.contains(&"> load pong".to_owned()));
    assert!(lines.contains(&"tick    1".to_owned()));
    // Up recalled the last command, typed before the cartridge ran.
    let editor = shell.lines(fake_rom().font(), text_columns(CELL_W), text_rows(CELL_H));
    assert!(
        editor
            .iter()
            .any(|line| line.text == "run" && line.col == 2)
    );
}

#[test]
fn resumed_input_starts_released() {
    let fixture = Fixture::new("sys-pause-input");
    let resumes: [fn(System) -> System; 2] =
        [press_break, |system| enter(open_shell(system), "resume")];
    for resume in resumes {
        // Up goes down before the pause and stays physically held, so the
        // host sends nothing more for it.
        let (system, lines) = logged_per_tick(started(&fixture, UP_STATE), &[&[down()]]);
        assert_eq!(lines, ["true    true"]);
        let system = resume(press_break(system));
        let (_, lines) = logged_per_tick(system, &[&[], &[]]);
        assert_eq!(lines, ["false   false", "false   false"]);
    }
}

#[test]
fn resumed_keys_start_released() {
    let fixture = Fixture::new("sys-pause-keys");
    let resumes: [fn(System) -> System; 2] =
        [press_break, |system| enter(open_shell(system), "resume")];
    for resume in resumes {
        let (system, lines) = logged_per_tick(started(&fixture, SPACE_STATE), &[&[space_down()]]);
        assert_eq!(lines, ["true    true"]);
        let system = resume(press_break(system));
        let (_, lines) = logged_per_tick(system, &[&[], &[]]);
        assert_eq!(lines, ["false   false", "false   false"]);
    }
}

#[test]
fn keys_pressed_while_paused_do_not_reach_the_cartridge() {
    let fixture = Fixture::new("sys-pause-keys-ignored");
    let system = press_break(started(&fixture, SPACE_STATE)).update(&[space_down()]);
    let (_, lines) = logged_per_tick(press_break(system), &[&[]]);
    assert_eq!(lines, ["false   false"]);
}

#[test]
fn a_new_run_starts_with_keys_released() {
    let fixture = Fixture::new("sys-key-rerun");
    let system = started(&fixture, SPACE_STATE).update(&[space_down()]);
    let system = enter(stop_cartridge(system), "run");
    let (_, lines) = logged_per_tick(system, &[&[]]);
    assert_eq!(lines, ["false   false"]);
}

#[test]
fn a_press_in_the_pausing_tick_is_dropped() {
    let fixture = Fixture::new("sys-pause-input-same-tick");
    let system = started(&fixture, UP_STATE).update(&[down(), InputEvent::Break]);
    let (_, lines) = logged_per_tick(press_break(system), &[&[], &[up()], &[down()]]);
    assert_eq!(lines, ["false   false", "false   false", "true    true"]);
}
