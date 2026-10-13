use super::*;
use pretty_assertions::assert_eq;

const LIFECYCLE: &str = r#"
        log.info("entry")
        function boot() log.info("boot") end
        local n = 0
        function tick() n += 1 log.info("tick", n) end
        function draw() log.info("draw", n) end
    "#;

#[test]
fn run_starts_the_cartridge_and_ticks_drive_it() {
    let fixture = Fixture::new("sys-run");
    let system = started(&fixture, LIFECYCLE);
    assert!(is_running(&system));
    assert_eq!(
        after(&system, "> run"),
        ["Running Pong 0.1.0", "entry", "boot"]
    );

    let system = stop_cartridge(ticks(system, 3));
    assert_eq!(
        after(&system, "> run"),
        [
            "Running Pong 0.1.0",
            "entry",
            "boot",
            "tick    1",
            "draw    1",
            "tick    2",
            "draw    2",
            "tick    3",
            "draw    3",
            "Paused Pong 0.1.0",
            "Type 'resume' to continue or 'stop' to end.",
            "> stop",
            "Stopped Pong 0.1.0",
        ]
    );
    assert_eq!(loaded_name(&system), Some("Pong"));
}

#[test]
fn the_shell_takes_no_input_while_running() {
    let fixture = Fixture::new("sys-run-input");
    let system = enter(started(&fixture, ""), "help");
    assert!(is_running(&system));
    let system = open_shell(press_break(system));
    assert_eq!(
        after(&system, "> run"),
        [
            "Running Pong 0.1.0",
            "Paused Pong 0.1.0",
            "Type 'resume' to continue or 'stop' to end."
        ]
    );
    // Nothing typed while running reached the command line.
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(
        transcript(&system).last().unwrap(),
        "Type 'resume' to continue or 'stop' to end."
    );
}

#[test]
fn break_in_the_paused_shell_is_ignored() {
    let fixture = Fixture::new("sys-paused-shell-break");
    let system = open_shell(press_break(started(&fixture, "")));
    let before = transcript(&system);
    let system = press_break(system);
    assert!(is_paused_shell(&system));
    assert_eq!(transcript(&system), before);
}

#[test]
fn running_again_starts_a_fresh_vm() {
    let fixture = Fixture::new("sys-rerun");
    let script = "function tick() count = (count or 0) + 1 log.info(count) end";
    let system = stop_cartridge(ticks(started(&fixture, script), 2));
    let system = stop_cartridge(ticks(enter(system, "run"), 1));
    assert_eq!(
        after(&system, "> run"),
        [
            "Running Pong 0.1.0",
            "1",
            "Paused Pong 0.1.0",
            "Type 'resume' to continue or 'stop' to end.",
            "> stop",
            "Stopped Pong 0.1.0"
        ]
    );
    assert!(transcript(&system).contains(&"2".to_owned()));
}

#[test]
fn start_failures_keep_the_cartridge_loaded() {
    let fixture = Fixture::new("sys-start-fail");
    for (script, expected) in [
        (
            r#"log.info("x") error("boom")"#,
            vec!["x", "! Runtime error: code/main.luau:1: boom"],
        ),
        (
            r#"log.info("e") function boot() log.info("b") error("no") end"#,
            vec!["e", "b", "! Runtime error in boot: code/main.luau:1: no"],
        ),
        (
            "function boot() while true do end end",
            vec!["! Execution limit exceeded in boot"],
        ),
        ("while true do end", vec!["! Execution limit exceeded"]),
    ] {
        let system = started(&fixture, script);
        assert_eq!(after(&system, "> run"), expected, "{script}");
        assert_eq!(loaded_name(&system), Some("Pong"), "{script}");
    }

    let system = started(&fixture, "local = 1");
    let lines = after(&system, "> run");
    assert!(
        matches!(lines.as_slice(), [only] if only.starts_with("! Syntax error: code/main.luau:1:")),
        "{lines:?}"
    );
    assert_eq!(loaded_name(&system), Some("Pong"));
}

#[test]
fn callback_failures_stop_the_cartridge() {
    let fixture = Fixture::new("sys-tick-fail");
    for (script, error) in [
        (
            r#"function tick() log.info("t") error("boom") end"#,
            "! Runtime error in tick: code/main.luau:1: boom",
        ),
        (
            r#"function tick() log.info("t") end function draw() error("bad") end"#,
            "! Runtime error in draw: code/main.luau:1: bad",
        ),
        (
            r#"function tick() log.info("t") while true do end end"#,
            "! Execution limit exceeded in tick",
        ),
        (
            r#"function tick() log.info("t") end
                   function draw() pcall(function() while true do end end) end"#,
            "! Execution limit exceeded in draw",
        ),
    ] {
        let system = ticks(started(&fixture, script), 1);
        assert_eq!(
            after(&system, "> run"),
            ["Running Pong 0.1.0", "t", error],
            "{script}"
        );
        assert_eq!(loaded_name(&system), Some("Pong"), "{script}");
    }
}

#[test]
fn info_reflects_the_cartridge_state() {
    let fixture = Fixture::new("sys-info");
    let state = |system: System| {
        let system = enter(system, "info");
        let line = transcript(&system).last().unwrap().clone();
        (system, line)
    };
    let (system, line) = state(shell_with_cartridge(&fixture, ""));
    assert_eq!(line, "STATE       IDLE");
    let (system, line) = state(enter(system, "load pong"));
    assert_eq!(line, "STATE       LOADED");
    let (system, line) = state(open_shell(press_break(enter(system, "run"))));
    assert_eq!(line, "STATE       PAUSED");
    assert!(is_paused_shell(&system));
    let (system, line) = state(enter(system, "stop"));
    assert_eq!(line, "STATE       LOADED");
    assert!(transcript(&system).contains(&"CARTRIDGE   Pong 0.1.0".to_owned()));
}

#[test]
fn unload_returns_a_loaded_cartridge_to_idle() {
    let fixture = Fixture::new("sys-unload");
    let system = enter(shell_with_cartridge(&fixture, ""), "load pong");
    let system = enter(system, "unload");
    assert_eq!(loaded_name(&system), None);
    assert_eq!(after(&system, "> unload"), ["Unloaded Pong 0.1.0"]);

    let system = enter(system, "info");
    let lines = after(&system, "> info");
    assert_eq!(
        &lines[4..],
        ["RUNTIME     NONE", "CARTRIDGE   NONE", "STATE       IDLE"]
    );
}

#[test]
fn unload_preserves_cartridge_files_and_the_shell_directory() {
    let fixture = Fixture::new("sys-unload-storage");
    let dir = fixture.root().join("games/pong");
    write_cartridge(&dir, "Pong");
    std::fs::write(dir.join("save.dat"), b"kept").unwrap();
    let manifest = std::fs::read(dir.join("cartridge.ron")).unwrap();

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
    let system = enter(enter(enter(system, "cd games"), "load pong"), "unload");
    let system = enter(system, "pwd");

    assert_eq!(transcript(&system).last().unwrap(), "/games");
    assert_eq!(std::fs::read(dir.join("cartridge.ron")).unwrap(), manifest);
    assert_eq!(std::fs::read(dir.join("save.dat")).unwrap(), b"kept");
}

#[test]
fn loading_another_cartridge_replaces_the_loaded_one() {
    let fixture = Fixture::new("sys-load-replace");
    let owl = fixture.root().join("owl");
    write_cartridge(&owl, "Owl");
    std::fs::write(owl.join("code/main.luau"), r#"log.info("owl")"#).unwrap();
    let system = enter(
        shell_with_cartridge(&fixture, r#"log.info("pong")"#),
        "load pong",
    );

    let system = enter(system, "load owl");
    assert_eq!(after(&system, "> load owl"), ["Loaded Owl 0.1.0"]);
    assert_eq!(loaded_name(&system), Some("Owl"));

    let system = enter(system, "run");
    assert_eq!(after(&system, "> run"), ["Running Owl 0.1.0", "owl"]);
}

#[test]
fn missing_entry_is_reported_at_run() {
    let fixture = Fixture::new("sys-run-missing");
    let system = enter(shell_with_cartridge(&fixture, ""), "load pong");
    std::fs::remove_file(fixture.root().join("pong/code/main.luau")).unwrap();
    let system = enter(system, "run");
    assert_eq!(
        after(&system, "> run"),
        ["! Entry not found: code/main.luau"]
    );
    assert_eq!(loaded_name(&system), Some("Pong"));
}

#[test]
fn malformed_object_tiles_prevent_startup() {
    let cases = [
        (vec![0; 31], "length must be a multiple of 32 bytes, got 31"),
        (
            vec![0; glaux_machine::sprite::OBJ_TILE_BANK_BYTES + 32],
            "got 32800 bytes, maximum is 32768",
        ),
    ];
    for (n, (bytes, expected)) in cases.into_iter().enumerate() {
        let fixture = Fixture::new(&format!("sys-run-bad-sprites-{n}"));
        let system = shell_with_cartridge(&fixture, "");
        std::fs::write(fixture.root().join("pong/sprites.bin"), bytes).unwrap();
        let system = enter(enter(system, "load pong"), "run");
        let lines = after(&system, "> run");
        assert!(
            matches!(lines.as_slice(), [only] if only.starts_with("! Invalid sprites.bin:") && only.contains(expected)),
            "{lines:?}"
        );
        assert_eq!(loaded_name(&system), Some("Pong"));
    }
}

#[test]
fn malformed_palette_assets_prevent_startup() {
    let cases = [
        (
            "obp.bin",
            vec![0; 5],
            "! Invalid obp.bin: object palette data must be exactly 128 bytes, got 5",
        ),
        (
            "palette.bin",
            vec![0; 3],
            "! Invalid palette.bin: palette data must be exactly 768 bytes, got 3",
        ),
        (
            "objattr.bin",
            vec![0, 0x80],
            "! Invalid objattr.bin: object tile 1 attribute 0x80 sets reserved bits; only bits 0-2 are defined",
        ),
    ];
    for (name, bytes, expected) in cases {
        let fixture = Fixture::new(&format!("sys-run-bad-{name}"));
        let system = shell_with_cartridge(&fixture, "");
        std::fs::write(fixture.root().join("pong").join(name), bytes).unwrap();
        let system = enter(enter(system, "load pong"), "run");
        assert_eq!(after(&system, "> run"), [expected], "{name}");
        assert_eq!(loaded_name(&system), Some("Pong"), "{name}");
    }
}

#[test]
fn the_cartridge_palette_is_presented_only_with_its_frame() {
    let fixture = Fixture::new("sys-frame-palette");
    let rom = fake_rom();
    let cartridge = reversed_rom_palette();
    assert_eq!(new_system().frame_palette(&rom), rom.palette());

    let system = ticks(started_with_palette(&fixture, COUNTER, &cartridge), 1);
    assert_eq!(system.frame_palette(&rom), &cartridge);
    let system = press_break(system);
    assert_eq!(system.frame_palette(&rom), &cartridge);
    let system = open_shell(system);
    assert_eq!(system.frame_palette(&rom), rom.palette());
}

#[test]
fn the_pause_indicator_keeps_its_colors_under_a_cartridge_palette() {
    let fixture = Fixture::new("sys-pause-palette");
    let script = "function draw() cls(1) end";
    let system = ticks(
        started_with_palette(&fixture, script, &reversed_rom_palette()),
        1,
    );
    let fb = render(&press_break(system));
    let (x, y) = pause::PLATE_ORIGIN;
    assert_eq!(fb.pixel(x + 1, y + 1), Some(255 - BACKGROUND));
    assert_eq!(fb.pixel(x + 5, y + 5), Some(255 - TEXT));
}

#[test]
fn script_errors_do_not_reveal_host_paths() {
    let fixture = Fixture::new("sys-run-paths");
    let host_paths = [
        fixture.base().display().to_string(),
        fixture.root().display().to_string(),
        fixture.outside().display().to_string(),
    ];
    for script in [
        "local = 1",
        r#"error("boom")"#,
        "local t = nil\nlog.info(t.x)",
        r#"require("../../outside/secret")"#,
        r#"log.info(debug.info(1, "s"))"#,
        r#"function tick() log.info(debug.info(1, "s")) error("x") end"#,
    ] {
        let system = ticks(started(&fixture, script), 1);
        for line in after(&system, "> run") {
            for path in &host_paths {
                assert!(!line.contains(path.as_str()), "{script}: {line}");
            }
        }
    }
}
