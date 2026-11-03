//! Running the cartridge with a map open: the map is saved first, and a map
//! that cannot be saved stops the run without losing its edits.

use super::code_tabs::loaded_with;
use super::map_management::{disk_name, map, map_dir, notice, open_maps, paint, write_map};
use super::*;
use crate::cartridge::map::MapFile;
use glaux_machine::tilemap::BgLayer;
use pretty_assertions::assert_eq;

#[test]
fn running_saves_the_open_map_first() {
    let fixture = Fixture::new("sys-maps-run-saves");
    write_map(&fixture, "alpha", "Alpha");
    let mut system = open_maps(loaded_with(&fixture, &[]));
    paint(&mut system);
    let system = system.update(&click(tab_centre(Tab::Code)));
    let system = system.update(&[InputEvent::Run]);
    assert!(is_running(&system));
    assert_eq!(
        std::fs::read(map_dir(&fixture, "alpha").join("bg0.bin")).unwrap(),
        [1, 0, 0, 0]
    );
}

#[test]
fn a_map_that_cannot_be_saved_cancels_the_run_and_keeps_its_edits() {
    let fixture = Fixture::new("sys-maps-run-cancelled");
    write_map(&fixture, "alpha", "Alpha");
    let mut system = open_maps(loaded_with(&fixture, &[]));
    paint(&mut system);
    // Another program wrote the layer meanwhile.
    let bg0 = map_dir(&fixture, "alpha").join("bg0.bin");
    std::fs::write(&bg0, [9, 0, 0, 0]).unwrap();

    let system = system.update(&click(tab_centre(Tab::Code)));
    let system = system.update(&[InputEvent::Run]);
    assert!(!is_running(&system), "the run did not start");
    assert!(is_workspace(&system));
    assert_eq!(notice(&system), Some("bg0.bin changed outside Glaux."));
    assert_eq!(
        std::fs::read(&bg0).unwrap(),
        [9, 0, 0, 0],
        "the disk is untouched"
    );
    let document = system.maps.as_ref().unwrap();
    assert!(document.is_modified(), "the edits are kept");
    assert!(document.is_file_modified(MapFile::Layer(BgLayer::Bg0)));
    let record = system.logs.terminal().unwrap();
    assert_eq!(record.category, crate::system::logs::Category::Save);
    assert_eq!(
        &*record.message,
        "Run cancelled: the map alpha could not be saved."
    );
}

#[test]
fn a_map_file_that_cannot_be_written_cancels_the_run_too() {
    let fixture = Fixture::new("sys-maps-run-read-only");
    write_map(&fixture, "alpha", "Alpha");
    let mut system = open_maps(loaded_with(&fixture, &[]));
    map(&mut system).set_name("Edited");
    let file = map_dir(&fixture, "alpha").join("map.ron");
    let writable = std::fs::metadata(&file).unwrap().permissions();
    let mut read_only = writable.clone();
    read_only.set_readonly(true);
    std::fs::set_permissions(&file, read_only).unwrap();

    let system = system.update(&click(tab_centre(Tab::Code)));
    let system = system.update(&[InputEvent::Run]);
    std::fs::set_permissions(&file, writable).unwrap();

    assert!(!is_running(&system));
    assert_eq!(notice(&system), Some("Read-only: /maps/alpha/map.ron"));
    assert!(system.maps.as_ref().unwrap().is_modified());
    assert_eq!(disk_name(&fixture, "alpha"), "Alpha");
}
