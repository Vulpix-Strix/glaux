//! Background layers composed between `tick()` and `draw()`.

use glaux_machine::tilemap::BgBank;
use glaux_script::debug::Resume;
use pretty_assertions::assert_eq;

use super::debugging::{at, luau_cartridge, start_debug, stopped};
use super::*;
use crate::cartridge_fs::fixture::Fixture;

const MAIN: &str = "code/main.luau";

/// `boot()` fills BG0 with the solid tile 257 in local index 1, which the
/// default palette bank maps to master index 1, and enables only BG0.
/// Appended after a script's own lines so its line numbers stay put.
const SETUP: &str = "
function boot()
  memset(0x24000 + 257 * 32, 0x11, 32)
  memset(0x34000, 0x01, 8192)
  poke(0x3C000, 1)
end";

fn with_setup(script: &str) -> String {
    format!("{script}{SETUP}")
}

fn start_run(fixture: &Fixture, script: &str) -> RunningCartridge {
    RunningCartridge::start(
        luau_cartridge(fixture, &with_setup(script)),
        &defaults(),
        VideoOverrides::default(),
        &font(),
    )
    .unwrap()
}

fn shown(running: &RunningCartridge, x: i32, y: i32) -> Option<u8> {
    running.published.framebuffer().pixel(x, y)
}

#[test]
fn the_published_frame_has_the_layers_under_what_draw_paints() {
    let fixture = Fixture::new("running-compose-published");
    let mut running = start_run(&fixture, "function draw() pset(3, 4, 9) end");
    assert_eq!(shown(&running, 0, 0), Some(0), "nothing before a draw");

    running.tick(&[]).unwrap();
    assert_eq!(shown(&running, 0, 0), Some(1));
    assert_eq!(shown(&running, 319, 239), Some(1));
    assert_eq!(shown(&running, 3, 4), Some(9));
}

#[test]
fn palette_and_backdrop_changes_show_in_the_next_frame() {
    let fixture = Fixture::new("running-compose-next-frame");
    let mut running = start_run(&fixture, "function draw() end");
    running.tick(&[]).unwrap();
    assert_eq!(shown(&running, 0, 0), Some(1));

    running
        .hardware_mut()
        .bg
        .palettes_mut()
        .set(BgBank::ZERO, 1, 5);
    running.hardware_mut().write(0x3C001, 12).unwrap();
    // The cell at the top left becomes empty, so the backdrop shows there.
    running.hardware_mut().write16(0x34000, 0).unwrap();
    assert_eq!(
        shown(&running, 0, 0),
        Some(1),
        "published frames do not change"
    );

    running.tick(&[]).unwrap();
    assert_eq!(shown(&running, 0, 0), Some(12));
    assert_eq!(shown(&running, 8, 0), Some(5));
}

#[test]
fn memory_changed_during_draw_waits_for_the_next_frame() {
    let fixture = Fixture::new("running-compose-vram-in-draw");
    let mut running = start_run(&fixture, "function draw() poke16(0x34000, 0) end");

    running.tick(&[]).unwrap();
    assert_eq!(shown(&running, 0, 0), Some(1), "composed before draw() ran");

    running.tick(&[]).unwrap();
    assert_eq!(shown(&running, 0, 0), Some(0), "composed after the change");
}

/// From its second tick on, `draw()` empties the first cell of BG0 before the
/// line the tests stop at.
const CHANGES_VRAM_IN_DRAW: &str = "local n = 0
function tick() n += 1 end
function draw()
  if n >= 2 then poke16(0x34000, 0) end
  pset(1, 1, 9)
  pset(2, 2, 8)
end";
const STOP_IN_DRAW: u32 = 6;

#[test]
fn resuming_a_stopped_draw_does_not_compose_again() {
    let fixture = Fixture::new("running-compose-resume-draw");
    let script = with_setup(CHANGES_VRAM_IN_DRAW);
    let (mut running, _) = start_debug(luau_cartridge(&fixture, &script), Vec::new());
    assert_eq!(running.advance(&[]), Ok(Tick::Done));
    assert_eq!(shown(&running, 0, 0), Some(1));

    running.set_breakpoints(vec![at(MAIN, STOP_IN_DRAW)]);
    stopped(running.advance(&[]));
    assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done));
    // The script emptied the cell before the stop. A new composition would
    // show the backdrop, while the frame prepared before draw() shows BG0.
    assert_eq!(shown(&running, 0, 0), Some(1));
    assert_eq!(shown(&running, 2, 2), Some(8));
    assert_eq!(running.hardware().tilemaps[0].entry(0, 0).unwrap().raw(), 0);

    running.set_breakpoints(Vec::new());
    assert_eq!(running.advance(&[]), Ok(Tick::Done));
    assert_eq!(shown(&running, 0, 0), Some(0), "the next tick composes it");
}

#[test]
fn a_stop_in_tick_composes_once_when_it_resumes() {
    let script = "function tick()
  poke(0x3C001, 6)
  poke(0x3C010, 3)
end
function draw() pset(1, 1, 9) end";
    let fixture = Fixture::new("running-compose-resume-tick");
    let (mut running, _) = start_debug(
        luau_cartridge(&fixture, &with_setup(script)),
        vec![at(MAIN, 3)],
    );
    stopped(running.advance(&[]));
    assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done));

    let other = Fixture::new("running-compose-uninterrupted");
    let (mut straight, _) = start_debug(luau_cartridge(&other, &with_setup(script)), Vec::new());
    assert_eq!(straight.advance(&[]), Ok(Tick::Done));

    assert!(running.published == straight.published);
    // BG0 is scrolled three pixels, which the frame reflects.
    assert_eq!(shown(&running, 0, 0), Some(1));
}
