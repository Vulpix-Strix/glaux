//! Running a cartridge under the debugger.

use std::fs;
use std::path::Path;
use std::time::Duration;

use glaux_input::{Button, InputEvent};
use glaux_script::Phase;
use glaux_script::debug::{Breakpoint, DebugReply, DebugRequest, Resume, Step};
use pretty_assertions::assert_eq;

use super::*;
use crate::cartridge::fixture::{write_cartridge, write_rhai_cartridge};
use crate::cartridge_fs::fixture::Fixture;
use crate::cartridge_fs::{CartridgeFs, CartridgeStorage};

pub(super) fn at(path: &str, line: u32) -> Breakpoint {
    Breakpoint {
        path: path.into(),
        line,
    }
}

pub(super) fn luau_cartridge(fixture: &Fixture, script: &str) -> LoadedCartridge {
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    fs::write(dir.join("code/main.luau"), script).unwrap();
    LoadedCartridge::load(&fixture.storage(), "pong").unwrap()
}

pub(super) fn rhai_cartridge(fixture: &Fixture, script: &str) -> LoadedCartridge {
    let dir = fixture.root().join("owl");
    write_rhai_cartridge(&dir, "Owl", script);
    LoadedCartridge::load(&fixture.storage(), "owl").unwrap()
}

pub(super) fn start_debug(
    cartridge: LoadedCartridge,
    breakpoints: Vec<Breakpoint>,
) -> (RunningCartridge, Option<Stop>) {
    RunningCartridge::start_debug(
        cartridge,
        &defaults(),
        VideoOverrides::default(),
        &font(),
        breakpoints,
    )
    .unwrap_or_else(|failed| panic!("expected the cartridge to start: {failed:?}"))
}

/// The first pixel of the published frame. The scripts of this file draw
/// `cls(1)`, so it is 1 once a `draw()` has finished and 0 before.
fn shown(running: &RunningCartridge) -> Option<u8> {
    running.published.framebuffer().pixel(0, 0)
}

pub(super) fn stopped(tick: Result<Tick, RuntimeError>) -> Stop {
    match tick {
        Ok(Tick::Stopped(stop)) => stop,
        other => panic!("expected a stop, got {other:?}"),
    }
}

pub(super) const LUAU: &str = "function boot()
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

pub(super) const RHAI: &str = "fn boot() {
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
            &font(),
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
            assert!(debugged.published == normal.published, "{name}");
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
        assert_eq!(shown(&running), Some(1), "{name}: draw ran too");
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
        &font(),
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
            RunningCartridge::start(load(), &defaults(), VideoOverrides::default(), &font())
                .unwrap();
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
            assert!(debugged.published == normal.published, "{name} tick {step}");
        }
        assert_eq!(
            texts(debugged.take_output()),
            texts(normal.take_output()),
            "{name}"
        );
    }
}

type Cartridge = fn(&Fixture, &str) -> LoadedCartridge;

/// Each language with its cartridge, the script of [`LUAU`] or [`RHAI`] and
/// the path breakpoints name.
const LANGUAGES: [(&str, Cartridge, &str, &str); 2] = [
    ("luau", luau_cartridge, LUAU, "code/main.luau"),
    ("rhai", rhai_cartridge, RHAI, "code/main.rhai"),
];

/// Drops the debugged runtime of `running` on another thread and fails if
/// that takes too long, so a join that never ends fails the test instead of
/// blocking the suite.
fn drops_promptly(running: RunningCartridge) {
    let RunningCartridge { runtime, .. } = running;
    let ScriptRuntime::Debugged(runtime) = runtime else {
        panic!("expected a debugged run");
    };
    let (done, finished) = std::sync::mpsc::channel();
    let dropper = std::thread::spawn(move || {
        drop(runtime);
        let _ = done.send(());
    });
    finished
        .recv_timeout(Duration::from_secs(10))
        .expect("dropping the run hung");
    dropper.join().unwrap();
}

#[test]
fn a_debugged_phase_hands_the_hardware_back_as_the_same_allocation() {
    for (name, cartridge, script, _) in LANGUAGES {
        let fixture = Fixture::new(&format!("running-debug-owner-{name}"));
        let (mut running, _) = start_debug(cartridge(&fixture, script), Vec::new());
        let before = hardware_address(&running);
        assert!(before.is_some(), "{name}");
        for _ in 0..3 {
            assert_eq!(running.advance(&[]), Ok(Tick::Done), "{name}");
            assert_eq!(hardware_address(&running), before, "{name}");
        }
    }
}

#[test]
fn a_stop_in_tick_keeps_the_hardware_with_the_runtime_until_the_tick_finishes() {
    for (name, cartridge, script, path) in LANGUAGES {
        let fixture = Fixture::new(&format!("running-debug-stop-tick-{name}"));
        let (mut running, _) = start_debug(cartridge(&fixture, script), vec![at(path, 6)]);
        let before = hardware_address(&running);

        stopped(running.advance(&[]));
        assert_eq!(running.hardware, None, "{name}: the runtime holds it");
        assert_eq!(running.stopped_in, Some(Phase::Tick), "{name}");
        assert_eq!(shown(&running), Some(0), "{name}: draw has not run");

        assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done), "{name}");
        assert_eq!(hardware_address(&running), before, "{name}");
        assert_eq!(running.stopped_in, None, "{name}");
        assert_eq!(shown(&running), Some(1), "{name}: draw ran after");
    }
}

/// Advancing a stopped run is refused without touching the input or the
/// thread. A command sent to the thread would be answered as if it were the
/// reply to the pending resume, so resuming afterwards must still finish the
/// stopped phase exactly once.
#[test]
fn a_stopped_run_refuses_to_advance_and_changes_nothing() {
    let press = [InputEvent::ButtonDown(Button::A)];
    for (phase, line) in [(Phase::Entry, 2), (Phase::Tick, 6), (Phase::Draw, 9)] {
        for (name, cartridge, script, path) in LANGUAGES {
            let fixture = Fixture::new(&format!("running-debug-refuse-{phase:?}-{name}"));
            let (mut running, first) =
                start_debug(cartridge(&fixture, script), vec![at(path, line)]);
            if phase != Phase::Entry {
                stopped(running.advance(&[]));
            } else {
                assert!(first.is_some(), "{name}: stopped in boot");
            }
            let input = running.input.clone();
            let held = running.hardware.is_some();
            let published = running.published.clone();
            running.take_output();

            let error = running.advance(&press).unwrap_err();
            assert_eq!(error.phase, phase, "{name} {phase:?}");
            assert!(
                error.to_string().contains("internal error"),
                "{name} {phase:?}: {error}"
            );
            assert_eq!(running.stopped_in, Some(phase), "{name} {phase:?}");
            assert_eq!(running.input, input, "{name} {phase:?}: input untouched");
            assert_eq!(running.hardware.is_some(), held, "{name} {phase:?}");
            assert!(running.published == published, "{name} {phase:?}");
            assert!(running.take_output().is_empty(), "{name} {phase:?}");

            assert_eq!(
                running.resume(Resume::Continue),
                Ok(Tick::Done),
                "{name} {phase:?}"
            );
            assert_eq!(running.stopped_in, None, "{name} {phase:?}");
            assert!(running.hardware.is_some(), "{name} {phase:?}");
            let expected = if phase == Phase::Entry { 0 } else { 1 };
            assert_eq!(shown(&running), Some(expected), "{name} {phase:?}");
        }
    }
}

#[test]
fn a_stop_in_draw_keeps_the_hardware_until_the_draw_finishes() {
    for (name, cartridge, script, path) in LANGUAGES {
        let fixture = Fixture::new(&format!("running-debug-stop-draw-{name}"));
        let (mut running, _) = start_debug(cartridge(&fixture, script), vec![at(path, 9)]);
        let before = hardware_address(&running);
        running.take_output();

        let stop = stopped(running.advance(&[]));
        assert_eq!(stop.frames[0].function, "draw", "{name}");
        assert_eq!(running.hardware, None, "{name}");
        assert_eq!(running.stopped_in, Some(Phase::Draw), "{name}");
        assert_eq!(
            texts(running.take_output()),
            ["tick"],
            "{name}: tick finished"
        );
        assert_eq!(shown(&running), Some(0), "{name}: not published yet");

        assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done), "{name}");
        assert_eq!(hardware_address(&running), before, "{name}");
        assert_eq!(shown(&running), Some(1), "{name}");
    }
}

#[test]
fn a_stop_while_starting_does_not_lend_the_hardware_or_run_draw() {
    for (name, cartridge, script, path) in LANGUAGES {
        let fixture = Fixture::new(&format!("running-debug-stop-boot-{name}"));
        let (mut running, stop) = start_debug(cartridge(&fixture, script), vec![at(path, 2)]);
        assert!(stop.is_some(), "{name}: stopped in boot");
        let before = hardware_address(&running);
        assert!(before.is_some(), "{name}: starting lends nothing");
        assert_eq!(running.stopped_in, Some(Phase::Entry), "{name}");

        assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done), "{name}");
        assert_eq!(running.stopped_in, None, "{name}");
        assert_eq!(shown(&running), Some(0), "{name}: no tick, so no draw");
        assert_eq!(hardware_address(&running), before, "{name}");
        assert_eq!(running.advance(&[]), Ok(Tick::Done), "{name}");
    }
}

#[test]
fn script_errors_in_either_phase_return_the_hardware() {
    for (phase, luau, rhai) in [
        (
            Phase::Tick,
            "function tick() error(\"boom\") end",
            "fn tick() { throw \"boom\"; }",
        ),
        (
            Phase::Draw,
            "function draw() error(\"boom\") end",
            "fn draw() { throw \"boom\"; }",
        ),
    ] {
        for (name, cartridge, script) in [
            ("luau", luau_cartridge as Cartridge, luau),
            ("rhai", rhai_cartridge, rhai),
        ] {
            let fixture = Fixture::new(&format!("running-debug-error-{phase:?}-{name}"));
            let (mut running, _) = start_debug(cartridge(&fixture, script), Vec::new());
            let before = hardware_address(&running);

            let error = running.advance(&[]).unwrap_err();
            assert_eq!(error.phase, phase, "{name}");
            assert_eq!(hardware_address(&running), before, "{name} {phase:?}");
        }
    }
}

#[test]
fn stopping_the_script_from_a_breakpoint_hands_the_hardware_back() {
    for (name, cartridge, script, path) in LANGUAGES {
        let fixture = Fixture::new(&format!("running-debug-stop-resume-{name}"));
        let (mut running, _) = start_debug(cartridge(&fixture, script), vec![at(path, 6)]);
        let before = hardware_address(&running);
        stopped(running.advance(&[]));

        let error = running.resume(Resume::Stop).unwrap_err();
        assert_eq!(error.error, ScriptError::Stopped, "{name}");
        assert_eq!(hardware_address(&running), before, "{name}");
        assert_eq!(running.stopped_in, None, "{name}");
        drops_promptly(running);
    }
}

#[test]
fn a_step_from_the_end_of_tick_goes_on_in_draw() {
    for (name, cartridge, script, path) in LANGUAGES {
        let fixture = Fixture::new(&format!("running-debug-step-draw-{name}"));
        let (mut running, _) = start_debug(cartridge(&fixture, script), vec![at(path, 6)]);
        let mut stop = stopped(running.advance(&[]));
        let mut seen = vec![stop.frames[0].function.clone()];
        for _ in 0..6 {
            if stop.frames[0].function == "draw" {
                break;
            }
            stop = stopped(running.resume(Resume::Step(Step::Over)));
            seen.push(stop.frames[0].function.clone());
        }
        assert_eq!(stop.frames[0].function, "draw", "{name}: {seen:?}");
        assert_eq!(running.stopped_in, Some(Phase::Draw), "{name}");
        assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done), "{name}");
    }
}

#[test]
fn audio_written_in_both_phases_reaches_the_psg_once_even_across_a_stop() {
    let script = "function tick()\n  audio.tone(0, 440, 8, \"square\")\nend\nfunction draw()\n  audio.gate(0, true)\nend";

    let normal_fixture = Fixture::new("running-debug-audio-normal");
    let mut normal = RunningCartridge::start(
        luau_cartridge(&normal_fixture, script),
        &defaults(),
        VideoOverrides::default(),
        &font(),
    )
    .unwrap();
    normal.tick(&[]).unwrap();
    let expected = normal.runtime.audio_mut().writes().len();
    assert!(expected > 0);

    for breakpoints in [Vec::new(), vec![at("code/main.luau", 5)]] {
        let stops = !breakpoints.is_empty();
        let fixture = Fixture::new(&format!("running-debug-audio-split-{stops}"));
        let (mut running, _) = start_debug(luau_cartridge(&fixture, script), breakpoints);
        if stops {
            stopped(running.advance(&[]));
            assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done));
        } else {
            assert_eq!(running.advance(&[]), Ok(Tick::Done));
        }
        assert_eq!(
            running.runtime.audio_mut().writes().len(),
            expected,
            "stops: {stops}"
        );
        let mut psg = Psg::new();
        running.apply_audio(&mut psg, true);
        assert!(
            running.runtime.audio_mut().writes().is_empty(),
            "applied once"
        );
        running.apply_audio(&mut psg, false);
        assert!(
            running.runtime.audio_mut().writes().is_empty(),
            "stops: {stops}"
        );
    }
}

#[test]
fn dropping_a_run_stopped_in_any_phase_ends_its_thread_without_hanging() {
    for (name, cartridge, script, path) in LANGUAGES {
        // A stop in `tick()`, in `draw()`, and while starting in `boot()`.
        for (what, line) in [("tick", 6), ("draw", 9), ("boot", 2)] {
            let fixture = Fixture::new(&format!("running-debug-drop-{what}-{name}"));
            let (mut running, start) =
                start_debug(cartridge(&fixture, script), vec![at(path, line)]);
            if what != "boot" {
                stopped(running.advance(&[]));
            } else {
                assert!(start.is_some(), "{name}: boot stopped");
            }
            drops_promptly(running);
        }

        // Right after a whole tick, with no stop at all.
        let fixture = Fixture::new(&format!("running-debug-drop-idle-{name}"));
        let (mut running, _) = start_debug(cartridge(&fixture, script), Vec::new());
        running.advance(&[]).unwrap();
        drops_promptly(running);
    }
}
