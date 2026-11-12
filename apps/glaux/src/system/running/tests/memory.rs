//! Memory written by Luau scripts, from the start to the published frame.

use glaux_script::debug::Resume;
use pretty_assertions::assert_eq;

use super::debugging::{at, luau_cartridge, start_debug, stopped};
use super::*;
use crate::cartridge_fs::fixture::Fixture;

const MAIN: &str = "code/main.luau";

/// Sets up memory in `boot()` and sets the backdrop color to 9, so the first
/// published `draw()` shows it.
const BOOT: &str = "poke(0x3C001, 9)
function boot()
  poke(0, 42)
  poke16(0x3B000, 0x0102)
end
function draw() end";

fn start_run(fixture: &Fixture, script: &str) -> RunningCartridge {
    RunningCartridge::start(
        luau_cartridge(fixture, script),
        &defaults(),
        VideoOverrides::default(),
        &font(),
    )
    .unwrap()
}

fn shown(running: &RunningCartridge) -> Option<u8> {
    running.published.framebuffer().pixel(0, 0)
}

#[test]
fn boot_initializes_memory_and_only_a_finished_draw_publishes_it() {
    let fixture = Fixture::new("running-memory-boot");
    let mut running = start_run(&fixture, BOOT);

    let hardware = running.hardware();
    assert_eq!(hardware.work_ram[0], 42);
    assert_eq!(hardware.registers.backdrop_color(), 9);
    assert_eq!(hardware.oam.as_bytes()[0], 0x02);
    assert_eq!(shown(&running), Some(0), "nothing published after boot");

    running.advance(&[]).unwrap();
    assert_eq!(shown(&running), Some(9));
}

#[test]
fn a_debugged_start_gives_the_same_first_frame_and_memory() {
    let fixture = Fixture::new("running-memory-boot-debugged");
    let (mut running, stop) = start_debug(luau_cartridge(&fixture, BOOT), vec![at(MAIN, 3)]);
    assert!(stop.is_some());
    assert_eq!(running.hardware, None);
    assert_eq!(shown(&running), Some(0));

    assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done));
    assert_eq!(running.hardware().work_ram[0], 42);
    assert_eq!(running.hardware().registers.backdrop_color(), 9);
    assert_eq!(
        shown(&running),
        Some(0),
        "resuming a start publishes nothing"
    );

    running.advance(&[]).unwrap();
    assert_eq!(shown(&running), Some(9));
}

const PALETTE_DRAW: &str = "function draw()
  poke(0x3A000, 200)
  pset(0, 0, 5)
end";

#[test]
fn a_palette_and_a_pixel_written_in_draw_are_published_together() {
    let fixture = Fixture::new("running-memory-palette");
    let mut running = start_run(&fixture, PALETTE_DRAW);
    let before = running.palette().get(0);

    running.advance(&[]).unwrap();
    assert_ne!(running.palette().get(0), before);
    assert_eq!(running.palette().get(0).r, 200);
    assert_eq!(shown(&running), Some(5));
}

#[test]
fn a_failing_draw_changes_the_hardware_but_not_the_published_frame() {
    let fixture = Fixture::new("running-memory-failing-draw");
    let mut running = start_run(
        &fixture,
        "function draw() poke(0x3A000, 200) pset(0, 0, 5) error('boom') end",
    );
    let before = running.palette().get(0);

    assert!(running.advance(&[]).is_err());
    assert_eq!(running.hardware().palette.get(0).r, 200);
    assert_eq!(running.hardware().framebuffer.pixel(0, 0), Some(5));
    assert_eq!(running.palette().get(0), before);
    assert_eq!(shown(&running), Some(0));
}

#[test]
fn a_draw_stopped_after_its_writes_publishes_nothing_until_it_finishes() {
    let fixture = Fixture::new("running-memory-stopped-draw");
    let (mut running, _) = start_debug(luau_cartridge(&fixture, PALETTE_DRAW), vec![at(MAIN, 3)]);
    let before = running.palette().get(0);

    stopped(running.advance(&[]));
    assert_eq!(running.hardware, None, "the runtime holds it while stopped");
    assert_eq!(running.palette().get(0), before);
    assert_eq!(shown(&running), Some(0));

    assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done));
    assert_eq!(running.palette().get(0).r, 200);
    assert_eq!(shown(&running), Some(5));
}
