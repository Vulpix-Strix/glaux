//! The frame published after each finished `draw()`.

use glaux_machine::palette::Rgb;
use glaux_script::debug::Resume;
use pretty_assertions::assert_eq;

use super::debugging::{at, luau_cartridge, rhai_cartridge, start_debug, stopped};
use super::*;
use crate::cartridge_fs::fixture::Fixture;
use crate::system::pause::PauseOverlay;

type Cartridge = fn(&Fixture, &str) -> LoadedCartridge;

/// A script of each language, and the path its breakpoints name.
type Scripts = [(&'static str, Cartridge, &'static str, &'static str); 2];

fn start_run(fixture: &Fixture, cartridge: Cartridge, script: &str) -> RunningCartridge {
    RunningCartridge::start(
        cartridge(fixture, script),
        &defaults(),
        VideoOverrides::default(),
        &font(),
    )
    .unwrap()
}

fn pixel(running: &RunningCartridge, x: i32, y: i32) -> Option<u8> {
    running.published.framebuffer().pixel(x, y)
}

fn is_blank(running: &RunningCartridge) -> bool {
    running
        .published
        .framebuffer()
        .as_bytes()
        .iter()
        .all(|&index| index == 0)
}

/// Scripts that draw `cls(n)` with the number of the tick, and a pixel.
const COUNTING: Scripts = [
    (
        "luau",
        luau_cartridge,
        "local n = 0
function tick() n += 1 end
function draw() cls(n) pset(3, 4, 9) end",
        "code/main.luau",
    ),
    (
        "rhai",
        rhai_cartridge,
        "fn boot() { this.n = 0; }
fn tick() { this.n += 1; }
fn draw() { cls(this.n); pset(3, 4, 9); }",
        "code/main.rhai",
    ),
];

#[test]
fn a_run_shows_a_blank_frame_until_a_draw_finishes_and_then_that_frame_and_palette() {
    for (name, cartridge, script, _) in COUNTING {
        let fixture = Fixture::new(&format!("running-publish-{name}"));
        let mut running = start_run(&fixture, cartridge, script);
        assert!(is_blank(&running), "{name}");
        assert_eq!(running.palette(), &running.hardware().palette, "{name}");

        running.tick(&[]).unwrap();
        assert_eq!(pixel(&running, 0, 0), Some(1), "{name}");
        assert_eq!(pixel(&running, 3, 4), Some(9), "{name}");
        assert!(
            running.published.framebuffer() == &running.hardware().framebuffer,
            "{name}: the published frame is a snapshot of the work framebuffer"
        );
    }
}

#[test]
fn a_palette_change_after_publication_does_not_recolor_the_published_frame() {
    for (name, cartridge, script, _) in COUNTING {
        let fixture = Fixture::new(&format!("running-publish-palette-{name}"));
        let mut running = start_run(&fixture, cartridge, script);
        running.tick(&[]).unwrap();
        let shown = running.palette().clone();

        let changed = Rgb::from_hex(0x123456);
        running.hardware_mut().palette.set(9, changed);
        assert_eq!(running.palette(), &shown, "{name}");
        assert_ne!(running.palette().get(9), changed, "{name}");

        running.tick(&[]).unwrap();
        assert_eq!(running.palette().get(9), changed, "{name}: next frame");
    }
}

#[test]
fn every_frame_is_rebuilt_from_the_backdrop_color() {
    for (name, cartridge, script) in [
        (
            "luau",
            luau_cartridge as Cartridge,
            "function draw() pset(1, 1, 3) end",
        ),
        ("rhai", rhai_cartridge, "fn draw() { pset(1, 1, 3); }"),
    ] {
        let fixture = Fixture::new(&format!("running-backdrop-{name}"));
        let mut running = start_run(&fixture, cartridge, script);
        running.hardware_mut().write(0x3C001, 12).unwrap();
        // Written before the frame is produced, so it does not survive it.
        running.hardware_mut().write(0x10000 + 5, 77).unwrap();

        running.tick(&[]).unwrap();
        assert_eq!(pixel(&running, 1, 1), Some(3), "{name}");
        assert_eq!(pixel(&running, 5, 0), Some(12), "{name}");
        assert_eq!(pixel(&running, 319, 239), Some(12), "{name}");
    }
}

#[test]
fn a_cartridge_without_draw_publishes_the_backdrop() {
    for (name, cartridge, script) in [
        ("luau", luau_cartridge as Cartridge, "function tick() end"),
        ("rhai", rhai_cartridge, "fn tick() {}"),
    ] {
        let fixture = Fixture::new(&format!("running-no-draw-{name}"));
        let mut running = start_run(&fixture, cartridge, script);
        running.hardware_mut().write(0x3C001, 12).unwrap();
        running.hardware_mut().write(0x10000, 77).unwrap();

        running.tick(&[]).unwrap();
        assert!(
            running
                .published
                .framebuffer()
                .as_bytes()
                .iter()
                .all(|&index| index == 12),
            "{name}"
        );
    }
}

#[test]
fn a_failed_tick_keeps_the_published_frame_and_what_the_hardware_holds() {
    for (name, cartridge, script) in [
        (
            "luau",
            luau_cartridge as Cartridge,
            "local n = 0
function tick() n += 1 if n == 2 then error('tick') end end
function draw() cls(n) end",
        ),
        (
            "rhai",
            rhai_cartridge,
            "fn boot() { this.n = 0; }
fn tick() { this.n += 1; if this.n == 2 { throw \"tick\"; } }
fn draw() { cls(this.n); }",
        ),
    ] {
        let fixture = Fixture::new(&format!("running-failed-tick-{name}"));
        let mut running = start_run(&fixture, cartridge, script);
        running.tick(&[]).unwrap();
        let published = running.published.clone();

        let changed = Rgb::from_hex(0x654321);
        running.hardware_mut().palette.set(7, changed);
        running.hardware_mut().write(9, 33).unwrap();
        let error = running.tick(&[]).unwrap_err();

        assert_eq!(error.phase, Phase::Tick, "{name}");
        assert!(running.published == published, "{name}");
        assert_eq!(running.hardware().work_ram[9], 33, "{name}");
        assert_eq!(running.hardware().palette.get(7), changed, "{name}");
        // The failed tick never started the next frame.
        assert_eq!(
            running.hardware().framebuffer.pixel(0, 0),
            Some(1),
            "{name}"
        );
    }
}

#[test]
fn a_failed_draw_keeps_the_published_frame_and_the_hardware_it_changed() {
    for (name, cartridge, script) in [
        (
            "luau",
            luau_cartridge as Cartridge,
            "local n = 0
function tick() n += 1 end
function draw() cls(n) if n == 2 then pset(1, 1, 9) error('draw') end end",
        ),
        (
            "rhai",
            rhai_cartridge,
            "fn boot() { this.n = 0; }
fn tick() { this.n += 1; }
fn draw() { cls(this.n); if this.n == 2 { pset(1, 1, 9); throw \"draw\"; } }",
        ),
    ] {
        let fixture = Fixture::new(&format!("running-failed-draw-{name}"));
        let mut running = start_run(&fixture, cartridge, script);
        running.tick(&[]).unwrap();
        let published = running.published.clone();

        // The palette is changed before the frame that fails, and has to
        // survive it in the hardware while the published one stays.
        let changed = Rgb::from_hex(0x654321);
        running.hardware_mut().palette.set(7, changed);
        let error = running.tick(&[]).unwrap_err();

        assert_eq!(error.phase, Phase::Draw, "{name}");
        assert!(running.published == published, "{name}");
        assert_eq!(pixel(&running, 0, 0), Some(1), "{name}");
        assert_eq!(
            pixel(&running, 1, 1),
            Some(1),
            "{name}: not the partial frame"
        );
        assert_ne!(running.palette().get(7), changed, "{name}");
        // What the failed draw drew stays in the hardware.
        assert_eq!(
            running.hardware().framebuffer.pixel(0, 0),
            Some(2),
            "{name}"
        );
        assert_eq!(
            running.hardware().framebuffer.pixel(1, 1),
            Some(9),
            "{name}"
        );
        assert_eq!(running.hardware().palette.get(7), changed, "{name}");
    }
}

#[test]
fn presenting_and_the_pause_overlay_never_write_to_the_run() {
    let rom = crate::rom::load().unwrap();
    for (name, cartridge, script, _) in COUNTING {
        let fixture = Fixture::new(&format!("running-present-{name}"));
        let mut running = start_run(&fixture, cartridge, script);
        running.tick(&[]).unwrap();
        let (hardware, published) = (running.hardware().clone(), running.published.clone());

        let mut presented = Framebuffer::new();
        running.render(&mut presented);
        assert!(&presented == published.framebuffer(), "{name}");
        let overlay = PauseOverlay::with_controls();
        overlay.render(&mut presented, running.palette(), rom.palette());
        overlay.render_controls(&mut presented, rom.font(), running.palette(), rom.palette());
        assert!(
            &presented != published.framebuffer(),
            "{name}: overlay drew"
        );

        assert!(*running.hardware() == hardware, "{name}");
        assert!(running.published == published, "{name}");
    }
}

/// Scripts that draw a frame, then a stop in `draw()` at line 6, then more.
const STOPPING: Scripts = [
    (
        "luau",
        luau_cartridge,
        "local n = 0
function tick() n += 1 end
function draw()
  log.info('draw ' .. n)
  cls(n)
  pset(1, 1, 9)
  pset(2, 2, 8)
end",
        "code/main.luau",
    ),
    (
        "rhai",
        rhai_cartridge,
        "fn boot() { this.n = 0; }
fn tick() { this.n += 1; }
fn draw() {
  log::info(`draw ${this.n}`);
  cls(this.n);
  pset(1, 1, 9);
  pset(2, 2, 8);
}",
        "code/main.rhai",
    ),
];

/// Starts `STOPPING` without breakpoints, shows its first frame and then
/// stops the second one in `draw()`, before its first `pset`.
fn stopped_in_the_second_draw(
    name: &str,
    cartridge: Cartridge,
    script: &str,
    path: &str,
) -> (RunningCartridge, PublishedFrame) {
    let fixture = Fixture::new(&format!("running-stop-draw-{name}-{}", script.len()));
    let (mut running, _) = start_debug(cartridge(&fixture, script), Vec::new());
    assert_eq!(running.advance(&[]), Ok(Tick::Done), "{name}");
    let first = running.published.clone();
    running.take_output();

    running.set_breakpoints(vec![at(path, 6)]);
    stopped(running.advance(&[]));
    (running, first)
}

#[test]
fn a_stop_in_draw_publishes_nothing_and_resuming_publishes_the_whole_frame() {
    for (name, cartridge, script, path) in STOPPING {
        let (mut running, first) = stopped_in_the_second_draw(name, cartridge, script, path);
        assert_eq!(running.stopped_in, Some(Phase::Draw), "{name}");
        assert!(running.hardware.is_none(), "{name}: the runtime holds it");
        assert!(running.published == first, "{name}: the first frame stays");
        assert_eq!(pixel(&running, 0, 0), Some(1), "{name}");

        assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done), "{name}");
        // The frame has what was drawn before the stop and after it.
        assert_eq!(pixel(&running, 0, 0), Some(2), "{name}");
        assert_eq!(pixel(&running, 1, 1), Some(9), "{name}");
        assert_eq!(pixel(&running, 2, 2), Some(8), "{name}");
        assert!(
            running.published.framebuffer() == &running.hardware().framebuffer,
            "{name}"
        );
        // What ran before the stop did not run again.
        assert_eq!(texts(running.take_output()), ["draw 2"], "{name}");
    }
}

#[test]
fn cancelling_a_stopped_draw_publishes_nothing_and_returns_the_hardware() {
    for (name, cartridge, script, path) in STOPPING {
        let (mut running, first) = stopped_in_the_second_draw(name, cartridge, script, path);

        let error = running.resume(Resume::Stop).unwrap_err();
        assert_eq!(error.phase, Phase::Draw, "{name}");
        assert_eq!(error.error, ScriptError::Stopped, "{name}");
        assert!(running.published == first, "{name}");
        // The hardware is back with what the draw had written so far.
        assert_eq!(
            running.hardware().framebuffer.pixel(0, 0),
            Some(2),
            "{name}"
        );
        assert_eq!(
            running.hardware().framebuffer.pixel(1, 1),
            Some(2),
            "{name}: cls ran, the pset did not"
        );
    }
}

#[test]
fn a_draw_that_fails_after_a_stop_publishes_nothing() {
    for (name, cartridge, script, path) in [
        (
            "luau",
            luau_cartridge as Cartridge,
            "function draw()
  cls(1)
  pset(1, 1, 2)
  error('late')
end",
            "code/main.luau",
        ),
        (
            "rhai",
            rhai_cartridge,
            "fn draw() {
  cls(1);
  pset(1, 1, 2);
  throw \"late\";
}",
            "code/main.rhai",
        ),
    ] {
        let fixture = Fixture::new(&format!("running-stop-fail-{name}"));
        let (mut running, _) = start_debug(cartridge(&fixture, script), vec![at(path, 3)]);
        let blank = running.published.clone();
        stopped(running.advance(&[]));

        let error = running.resume(Resume::Continue).unwrap_err();
        assert_eq!(error.phase, Phase::Draw, "{name}");
        assert!(running.published == blank, "{name}");
        assert_eq!(
            running.hardware().framebuffer.pixel(0, 0),
            Some(1),
            "{name}"
        );
        assert_eq!(
            running.hardware().framebuffer.pixel(1, 1),
            Some(2),
            "{name}: drawn after the stop, before the failure"
        );
    }
}

#[test]
fn a_draw_stopped_and_resumed_spends_one_budget() {
    // Two texts of 2047 characters cost 65 504 of the 65 536 units, so 33
    // more pixels are one too many only if the first two were remembered.
    for (name, cartridge, script, path) in [
        (
            "luau",
            luau_cartridge as Cartridge,
            "function draw()
  print(string.rep('x', 2047), 0, 0, 1)
  print(string.rep('x', 2047), 0, 8, 1)
  for i = 1, 33 do pset(0, 20, 1) end
end",
            "code/main.luau",
        ),
        (
            "rhai",
            rhai_cartridge,
            "fn draw() {
  let s = \"\"; for i in 0..2047 { s += \"x\"; }
  text(s, 0, 0, 1);
  text(s, 0, 8, 1);
  for i in 0..33 { pset(0, 20, 1); }
}",
            "code/main.rhai",
        ),
    ] {
        let line = if name == "luau" { 3 } else { 4 };
        let fixture = Fixture::new(&format!("running-stop-budget-{name}"));
        let (mut running, _) = start_debug(cartridge(&fixture, script), vec![at(path, line)]);
        let blank = running.published.clone();
        stopped(running.advance(&[]));

        let error = running.resume(Resume::Continue).unwrap_err();
        assert_eq!(error.phase, Phase::Draw, "{name}");
        assert!(
            error.to_string().contains("too many draw commands"),
            "{name}: {error}"
        );
        assert!(running.published == blank, "{name}");
        // The first 32 pixels were drawn, and the 33rd was not.
        assert_eq!(
            running.hardware().framebuffer.pixel(0, 20),
            Some(1),
            "{name}"
        );
    }
}

#[test]
fn resuming_a_stop_while_starting_publishes_no_frame() {
    for (name, cartridge, script, path) in [
        (
            "luau",
            luau_cartridge as Cartridge,
            super::debugging::LUAU,
            "code/main.luau",
        ),
        (
            "rhai",
            rhai_cartridge,
            super::debugging::RHAI,
            "code/main.rhai",
        ),
    ] {
        let fixture = Fixture::new(&format!("running-stop-boot-publish-{name}"));
        let (mut running, stop) = start_debug(cartridge(&fixture, script), vec![at(path, 2)]);
        assert!(stop.is_some(), "{name}: stopped in boot");
        let published = running.published.clone();

        assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done), "{name}");
        assert!(running.published == published, "{name}");
        assert!(is_blank(&running), "{name}");
    }
}
