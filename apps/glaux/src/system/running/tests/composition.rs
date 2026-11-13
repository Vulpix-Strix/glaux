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

/// `boot()` gives tile 0 of the object bank local index 2 and tile 1 local
/// index 4, which the default palette bank maps to master indexes 2 and 4. It
/// then enables object 0, an 8 by 8 object at (5, 6), and nothing else.
const OBJECT_SETUP: &str = "
function boot()
  memset(0x2C000, 0x22, 32)
  memset(0x2C000 + 32, 0x44, 32)
  poke16(0x3B000, 5)
  poke16(0x3B002, 6)
  poke(0x3B007, 0x08)
  poke(0x3C000, 8)
end";

fn start_objects(fixture: &Fixture, script: &str) -> RunningCartridge {
    RunningCartridge::start(
        luau_cartridge(fixture, &format!("{script}{OBJECT_SETUP}")),
        &defaults(),
        VideoOverrides::default(),
        &font(),
    )
    .unwrap()
}

#[test]
fn objects_set_up_in_boot_are_published_under_what_draw_paints() {
    let fixture = Fixture::new("running-compose-objects");
    let mut running = start_objects(
        &fixture,
        "function draw()
  pset(6, 7, 9)
  spr(1, 8, 9)
end",
    );
    assert_eq!(shown(&running, 5, 6), Some(0), "nothing before a draw");

    running.tick(&[]).unwrap();
    assert_eq!(shown(&running, 5, 6), Some(2));
    assert_eq!(shown(&running, 5, 13), Some(2));
    assert_eq!(shown(&running, 13, 6), Some(0), "outside the object");
    // Immediate drawing goes over the objects and does not use the table.
    assert_eq!(shown(&running, 6, 7), Some(9));
    assert_eq!(shown(&running, 9, 10), Some(4), "spr over the object");
    assert_eq!(shown(&running, 14, 15), Some(4));
}

#[test]
fn an_object_table_write_during_draw_waits_for_the_next_frame() {
    let fixture = Fixture::new("running-compose-oam-in-draw");
    let mut running = start_objects(&fixture, "function draw() poke(0x3B007, 0) end");

    running.tick(&[]).unwrap();
    assert_eq!(shown(&running, 5, 6), Some(2), "composed before draw() ran");

    running.tick(&[]).unwrap();
    assert_eq!(shown(&running, 5, 6), Some(0), "composed after the change");
}

#[test]
fn resuming_a_stopped_draw_does_not_compose_the_objects_again() {
    // From its second tick on, draw() disables the object before the line the
    // test stops at.
    let script = "local n = 0
function tick() n += 1 end
function draw()
  if n >= 2 then poke(0x3B007, 0) end
  pset(20, 20, 9)
  pset(21, 21, 8)
end";
    let fixture = Fixture::new("running-compose-resume-objects");
    let (mut running, _) = start_debug(
        luau_cartridge(&fixture, &format!("{script}{OBJECT_SETUP}")),
        Vec::new(),
    );
    assert_eq!(running.advance(&[]), Ok(Tick::Done));
    assert_eq!(shown(&running, 5, 6), Some(2));

    running.set_breakpoints(vec![at(MAIN, STOP_IN_DRAW)]);
    stopped(running.advance(&[]));
    assert_eq!(running.resume(Resume::Continue), Ok(Tick::Done));
    assert_eq!(shown(&running, 5, 6), Some(2), "prepared before draw()");
    assert_eq!(shown(&running, 21, 21), Some(8));
    assert!(!running.hardware().oam.entry(0).unwrap().enabled);

    running.set_breakpoints(Vec::new());
    assert_eq!(running.advance(&[]), Ok(Tick::Done));
    assert_eq!(shown(&running, 5, 6), Some(0), "the next tick composes it");
}

/// A run whose script only fills BG0 with tile 5 and enables the layer, so
/// everything on screen comes from the background tiles and palettes the
/// run starts with.
const TILE_5_ONLY: &str = "
function boot()
  for i = 0, 4095 do poke16(0x34000 + 2 * i, 5) end
  poke(0x3C000, 1)
end
function draw() end";

#[test]
fn unsaved_background_edits_are_what_a_run_draws_and_the_files_stay() {
    use crate::cartridge::VideoAsset;
    use crate::system::sprite_document::{SpriteDocument, TileSet};
    use crate::system::video::VideoFiles;

    let fixture = Fixture::new("running-compose-unsaved-bg");
    let cartridge = luau_cartridge(&fixture, TILE_5_ONLY);

    // Tile 5 is solid local 2, and BGP0 maps local 2 to master 7, but only
    // in the editor's memory.
    let mut document = SpriteDocument::from_files(&VideoFiles::default(), &defaults()).unwrap();
    document.fill_in(TileSet::Bg, 5, 0, 0, 2);
    document.set_bg_color(BgBank::ZERO, 2, 7);
    assert_eq!(
        document.modified_assets().collect::<Vec<_>>(),
        [VideoAsset::BgTiles, VideoAsset::BgPalettes]
    );

    let mut running =
        RunningCartridge::start(cartridge, &defaults(), document.unsaved(), &font()).unwrap();
    running.tick(&[]).unwrap();
    assert_eq!(shown(&running, 0, 0), Some(7));
    assert_eq!(shown(&running, 319, 239), Some(7));

    let dir = fixture.root().join("pong");
    assert!(!dir.join("bgtiles.bin").exists());
    assert!(!dir.join("bgp.bin").exists());
}

#[test]
fn without_unsaved_edits_the_same_run_shows_the_backdrop() {
    let fixture = Fixture::new("running-compose-saved-bg");
    let mut running = RunningCartridge::start(
        luau_cartridge(&fixture, TILE_5_ONLY),
        &defaults(),
        VideoOverrides::default(),
        &font(),
    )
    .unwrap();
    running.tick(&[]).unwrap();
    assert_eq!(shown(&running, 0, 0), Some(0), "tile 5 is empty");
}
