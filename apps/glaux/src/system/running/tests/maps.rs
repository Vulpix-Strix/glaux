//! Maps read by Luau cartridges, from their files or from the editor.

use glaux_machine::tilemap::{BgBank, BgLayer, TileEntry, TileLayer};
use pretty_assertions::assert_eq;

use super::debugging::{luau_cartridge, rhai_cartridge};
use super::*;
use crate::cartridge::map::{MapFile, MapId, MapMeta};
use crate::cartridge_fs::fixture::Fixture;
use crate::system::map_document::MapDocument;
use crate::system::maps::{MapOverrides, RunOverrides};

fn town() -> MapId {
    MapId::parse("town").unwrap()
}

fn entry(tile: u16) -> TileEntry {
    TileEntry::new(tile, false, false, false, BgBank::ZERO).unwrap()
}

/// A 64 x 64 layer where cell `(x, y)` holds a distinct tile.
fn numbered() -> TileLayer {
    let mut layer = TileLayer::empty(64, 64).unwrap();
    for y in 0..64 {
        for x in 0..64 {
            layer.set(x, y, entry(1 + ((y * 64 + x) % 1000) as u16));
        }
    }
    layer
}

/// Adds map `town` with `layer` as BG0 to `cartridge`.
fn add_town(cartridge: &LoadedCartridge, layer: &TileLayer) {
    cartridge
        .create_map(&town(), &MapMeta::new("Town", 64, 64).unwrap())
        .unwrap();
    cartridge
        .replace_map_file(
            &town(),
            MapFile::Layer(BgLayer::Bg0),
            &layer.to_bytes(),
            None,
        )
        .unwrap();
}

fn start(cartridge: LoadedCartridge) -> Result<RunningCartridge, Box<FailedStart>> {
    RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default(), &font())
}

fn bg0(running: &RunningCartridge) -> Vec<u8> {
    running.hardware().tilemaps[0].as_bytes().to_vec()
}

#[test]
fn the_windows_start_empty_even_when_the_cartridge_has_maps() {
    let fixture = Fixture::new("running-maps-empty");
    let cartridge = luau_cartridge(&fixture, "function tick() end");
    add_town(&cartridge, &numbered());
    let running = start(cartridge).unwrap();
    assert!(bg0(&running).iter().all(|&byte| byte == 0));
}

#[test]
fn a_script_loads_a_map_from_the_cartridge_into_a_window() {
    let fixture = Fixture::new("running-maps-load");
    let cartridge = luau_cartridge(&fixture, "function boot() map.load('town', 0) end");
    let layer = numbered();
    add_town(&cartridge, &layer);
    let running = start(cartridge).unwrap();
    assert_eq!(bg0(&running), layer.to_bytes());
}

#[test]
fn writing_a_window_does_not_change_the_map_file() {
    let fixture = Fixture::new("running-maps-source");
    let cartridge = luau_cartridge(
        &fixture,
        "function boot() map.load('town', 0) end
        function tick() poke16(0x34000, 0x0123) end",
    );
    let layer = numbered();
    add_town(&cartridge, &layer);
    let mut running = start(cartridge).unwrap();
    running.tick(&[]).unwrap();
    assert_eq!(running.hardware().tilemaps[0].as_bytes()[..2], [0x23, 0x01]);
    let file = running
        .cartridge()
        .read_map_file(&town(), MapFile::Layer(BgLayer::Bg0))
        .unwrap();
    assert_eq!(file, Some(layer.to_bytes()));
}

#[test]
fn a_broken_map_stops_a_luau_cartridge_from_starting() {
    let fixture = Fixture::new("running-maps-broken-luau");
    let cartridge = luau_cartridge(&fixture, "function tick() end");
    add_town(&cartridge, &numbered());
    cartridge
        .replace_map_file(
            &town(),
            MapFile::Layer(BgLayer::Bg0),
            b"short",
            Some(&numbered().to_bytes()),
        )
        .unwrap();
    let failed = start(cartridge).unwrap_err();
    let StartError::Maps { id, .. } = failed.error else {
        panic!("expected a map error, got {:?}", failed.error);
    };
    assert_eq!(id, town());
}

#[test]
fn a_broken_map_does_not_stop_a_rhai_cartridge() {
    let fixture = Fixture::new("running-maps-broken-rhai");
    let cartridge = rhai_cartridge(&fixture, "fn tick() {}");
    add_town(&cartridge, &numbered());
    cartridge
        .replace_map_file(
            &town(),
            MapFile::Layer(BgLayer::Bg0),
            b"short",
            Some(&numbered().to_bytes()),
        )
        .unwrap();
    assert!(start(cartridge).is_ok());
}

#[test]
fn a_run_from_the_shell_sees_the_unsaved_edits_of_the_open_map() {
    let fixture = Fixture::new("running-maps-unsaved");
    let cartridge = luau_cartridge(&fixture, "function boot() map.load('town', 0) end");
    add_town(&cartridge, &numbered());
    let mut document = MapDocument::load(&cartridge, &town()).unwrap();
    assert!(document.set_cell(BgLayer::Bg0, 2, 1, entry(777)));

    let overrides = RunOverrides {
        video: VideoOverrides::default(),
        maps: MapOverrides::new(&document),
    };
    let running = RunningCartridge::start(cartridge, &defaults(), overrides, &font()).unwrap();
    let window = &running.hardware().tilemaps[0];
    assert_eq!(window.entry(2, 1), Some(entry(777)));
    assert_eq!(window.entry(3, 1), Some(entry(1 + (64 + 3) as u16)));
}

#[test]
fn an_unmodified_document_stands_in_for_nothing() {
    let fixture = Fixture::new("running-maps-saved");
    let cartridge = luau_cartridge(&fixture, "function tick() end");
    add_town(&cartridge, &numbered());
    let document = MapDocument::load(&cartridge, &town()).unwrap();
    assert!(MapOverrides::new(&document).is_empty());
}
