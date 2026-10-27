//! Running a cartridge under the debugger.

use std::fs;
use std::path::Path;

use glaux_input::{Button, InputEvent};
use glaux_script::Phase;
use glaux_script::debug::{Breakpoint, DebugReply, DebugRequest, Resume, Step};
use pretty_assertions::assert_eq;

use super::*;
use crate::cartridge::fixture::{write_cartridge, write_rhai_cartridge};
use crate::cartridge_fs::fixture::Fixture;
use crate::cartridge_fs::{CartridgeFs, CartridgeStorage};

fn at(path: &str, line: u32) -> Breakpoint {
    Breakpoint {
        path: path.into(),
        line,
    }
}

fn luau_cartridge(fixture: &Fixture, script: &str) -> LoadedCartridge {
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    fs::write(dir.join("code/main.luau"), script).unwrap();
    LoadedCartridge::load(&fixture.storage(), "pong").unwrap()
}

fn rhai_cartridge(fixture: &Fixture, script: &str) -> LoadedCartridge {
    let dir = fixture.root().join("owl");
    write_rhai_cartridge(&dir, "Owl", script);
    LoadedCartridge::load(&fixture.storage(), "owl").unwrap()
}

fn start_debug(
    cartridge: LoadedCartridge,
    breakpoints: Vec<Breakpoint>,
) -> (RunningCartridge, Option<Stop>) {
    RunningCartridge::start_debug(
        cartridge,
        &defaults(),
        VideoOverrides::default(),
        breakpoints,
    )
    .unwrap_or_else(|failed| panic!("expected the cartridge to start: {failed:?}"))
}

fn stopped(tick: Result<Tick, RuntimeError>) -> Stop {
    match tick {
        Ok(Tick::Stopped(stop)) => stop,
        other => panic!("expected a stop, got {other:?}"),
    }
}

const LUAU: &str = "function boot()
  log.info(\"boot\")
end
function tick()
  local x = frame()
  log.info(\"tick\")
end
function draw()
  cls(1)
  print(\"hi\", 2, 2, 7)
end";

const RHAI: &str = "fn boot() {
  log::info(\"boot\");
}
fn tick() {
  let x = frame();
  log::info(\"tick\");
}
fn draw() {
  cls(1);
  text(\"hi\", 2, 2, 7);
}";

#[test]
fn a_debugged_run_without_stops_produces_what_a_normal_run_does() {
    for (name, cartridge) in [
        (
            "luau",
            luau_cartridge as fn(&Fixture, &str) -> LoadedCartridge,
        ),
        ("rhai", rhai_cartridge),
    ] {
        let script = if name == "luau" { LUAU } else { RHAI };
        let normal_fixture = Fixture::new(&format!("running-debug-normal-{name}"));
        let debug_fixture = Fixture::new(&format!("running-debug-debugged-{name}"));
        let mut normal = RunningCartridge::start(
            cartridge(&normal_fixture, script),
            &defaults(),
            VideoOverrides::default(),
        )
        .unwrap();
        let (mut debugged, stop) = start_debug(cartridge(&debug_fixture, script), Vec::new());
        assert!(stop.is_none());
        assert!(debugged.is_debugged());
        assert_eq!(texts(debugged.take_output()), texts(normal.take_output()));
        for _ in 0..3 {
            normal.tick(&[]).unwrap();
            assert_eq!(debugged.advance(&[]), Ok(Tick::Done), "{name}");
            assert_eq!(
                texts(debugged.take_output()),
                texts(normal.take_output()),
                "{name}"
            );
            assert_eq!(debugged.runtime.frame(), normal.runtime.frame(), "{name}");
        }
    }
}

#[test]
fn a_breakpoint_stops_the_tick_and_resuming_finishes_it() {
    for (name, cartridge, script, path) in [
        (
            "luau",
            luau_cartridge as fn(&Fixture, &str) -> LoadedCartridge,
            LUAU,
            "code/main.luau",
        ),
        ("rhai", rhai_cartridge, RHAI, "code/main.rhai"),
    ] {
        let fixture = Fixture::new(&format!("running-debug-stop-{name}"));
        let (mut running, stop) = start_debug(cartridge(&fixture, script), vec![at(path, 6)]);
        assert!(stop.is_none(), "{name}");
        running.take_output();

        let stop = stopped(running.advance(&[]));
        assert_eq!(stop.frames[0].line, Some(6), "{name}");
        assert_eq!(stop.frames[0].function, "tick", "{name}");
        let DebugReply::Output { entries, .. } = running.ask(DebugRequest::Output) else {
            panic!("expected the output");
        };
        assert!(entries.is_empty(), "{name}: nothing logged before the line");

        assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done), "{name}");
        assert_eq!(texts(running.take_output()), ["tick"], "{name}");
        assert_eq!(running.runtime.frame().len(), 2, "{name}: draw ran too");
    }
}

#[test]
fn a_breakpoint_in_the_entry_script_stops_the_start_and_resuming_completes_it() {
    let fixture = Fixture::new("running-debug-entry");
    let cartridge = luau_cartridge(
        &fixture,
        "local ready = true\nfunction boot() end\nfunction tick() end",
    );
    let (mut running, stop) = start_debug(cartridge, vec![at("code/main.luau", 1)]);
    assert_eq!(stop.expect("stopped at the start").frames[0].line, Some(1));
    assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done));
    assert_eq!(running.advance(&[]), Ok(Tick::Done));
}

#[test]
fn a_failure_after_resuming_is_reported_as_a_failure_of_the_run() {
    let fixture = Fixture::new("running-debug-failure");
    let cartridge = luau_cartridge(
        &fixture,
        "function tick()\n  local x = 1\n  error(\"boom\")\nend",
    );
    let (mut running, _) = start_debug(cartridge, vec![at("code/main.luau", 2)]);
    stopped(running.advance(&[]));
    let err = running.resume(Resume::Continue).unwrap_err();
    assert_eq!(err.phase, Phase::Tick);
    assert!(matches!(err.error, ScriptError::Runtime(m) if m.contains("boom")));
}

#[test]
fn a_script_that_fails_to_start_is_handed_back_with_its_cartridge() {
    let fixture = Fixture::new("running-debug-start-failure");
    let cartridge = luau_cartridge(&fixture, "local = 1");
    let failed = RunningCartridge::start_debug(
        cartridge,
        &defaults(),
        VideoOverrides::default(),
        Vec::new(),
    )
    .unwrap_err();
    assert!(matches!(
        failed.error,
        StartError::Script(StartFailure {
            error: RuntimeError {
                phase: Phase::Entry,
                ..
            },
            ..
        })
    ));
}

#[test]
fn stepping_goes_line_by_line_and_breakpoints_can_change_between_ticks() {
    let fixture = Fixture::new("running-debug-step");
    let (mut running, _) = start_debug(
        luau_cartridge(&fixture, LUAU),
        vec![at("code/main.luau", 5)],
    );
    let first = stopped(running.advance(&[]));
    assert_eq!(first.frames[0].line, Some(5));
    let next = stopped(running.resume(Resume::Step(Step::Over)));
    assert_eq!(next.frames[0].line, Some(6));
    assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done));

    running.set_breakpoints(vec![at("code/main.luau", 6)]);
    assert_eq!(stopped(running.advance(&[])).frames[0].line, Some(6));
    assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done));
}

#[test]
fn audio_written_in_a_debugged_tick_reaches_the_psg_once() {
    let fixture = Fixture::new("running-debug-audio");
    let script = "function tick()\n  audio.tone(0, 440, 8, \"square\")\n  audio.gate(0, true)\nend";
    let (mut running, _) = start_debug(luau_cartridge(&fixture, script), Vec::new());
    running.advance(&[]).unwrap();
    assert_eq!(running.runtime.audio_mut().writes().len(), 1);
    let mut psg = Psg::new();
    running.apply_audio(&mut psg, true);
    assert!(running.runtime.audio_mut().writes().is_empty(), "applied");
    // No tick since: nothing is replayed.
    running.apply_audio(&mut psg, false);
    assert!(running.runtime.audio_mut().writes().is_empty());
}

#[test]
fn input_events_reach_a_debugged_cartridge() {
    let fixture = Fixture::new("running-debug-input");
    let script = "function tick()\n  if btn(Button.A) then log.info(\"A\") end\nend";
    let (mut running, _) = start_debug(luau_cartridge(&fixture, script), Vec::new());
    running.take_output();
    running
        .advance(&[InputEvent::ButtonDown(Button::A)])
        .unwrap();
    assert_eq!(texts(running.take_output()), ["A"]);
}

#[test]
fn dropping_a_stopped_run_ends_its_thread() {
    let fixture = Fixture::new("running-debug-drop");
    let (mut running, _) = start_debug(
        luau_cartridge(&fixture, LUAU),
        vec![at("code/main.luau", 5)],
    );
    stopped(running.advance(&[]));
    // The drop joins the thread, so a hang here fails the test by timeout.
    drop(running.stop());
}

#[test]
fn the_examples_play_the_same_under_the_debugger() {
    let examples =
        fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")).unwrap();
    for name in ["signal-lost", "signal-lost-rhai"] {
        let load = || {
            let storage = CartridgeStorage::Available(CartridgeFs::new(examples.clone()));
            LoadedCartridge::load(&storage, name).unwrap()
        };
        let mut normal =
            RunningCartridge::start(load(), &defaults(), VideoOverrides::default()).unwrap();
        let (mut debugged, _) = start_debug(load(), Vec::new());
        let inputs = [
            vec![],
            vec![InputEvent::ButtonDown(Button::A)],
            vec![InputEvent::ButtonUp(Button::A)],
            vec![InputEvent::ButtonDown(Button::Right)],
        ];
        for step in 0..120 {
            let input = &inputs[step % inputs.len()];
            normal.tick(input).unwrap();
            assert_eq!(
                debugged.advance(input),
                Ok(Tick::Done),
                "{name} tick {step}"
            );
            assert_eq!(
                debugged.runtime.frame(),
                normal.runtime.frame(),
                "{name} tick {step}"
            );
        }
        assert_eq!(
            texts(debugged.take_output()),
            texts(normal.take_output()),
            "{name}"
        );
    }
}
