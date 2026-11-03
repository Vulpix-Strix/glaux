//! The open map among the unsaved edits: quitting, replacing the cartridge
//! and files that changed outside Glaux.

use super::code_tabs::loaded_with;
use super::*;
use crate::cartridge::map::{MapFiles, MapId, MapMeta};
use crate::system::map_document::MapDocument;
use glaux_machine::tilemap::{BgLayer, TileEntry};
use pretty_assertions::assert_eq;
use std::path::PathBuf;

fn map_dir(fixture: &Fixture) -> PathBuf {
    fixture.root().join("pong/maps/a")
}

/// `Pong` in the shell with map `a` (2x1) on disk and open in the system.
fn with_map(fixture: &Fixture) -> System {
    let meta = MapMeta::new("A", 2, 1).unwrap().to_bytes();
    let dir = map_dir(fixture);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("map.ron"), &meta).unwrap();

    let mut system = loaded_with(fixture, &[]);
    let files = MapFiles {
        meta,
        layers: [None, None, None],
    };
    system.maps = Some(MapDocument::from_files(MapId::parse("a").unwrap(), &files).unwrap());
    system
}

fn map(system: &mut System) -> &mut MapDocument {
    system.maps.as_mut().unwrap()
}

fn paint(system: &mut System) {
    let entry = TileEntry::from_raw(1).unwrap();
    assert!(map(system).set_cell(BgLayer::Bg0, 0, 0, entry));
}

fn question_message(system: &System) -> Vec<String> {
    system.unsaved_flow().unwrap().dialog().message().to_vec()
}

/// Chooses SAVE ALL, two buttons above the focused CANCEL.
fn save_all(system: System) -> System {
    let up = InputEvent::Up { select: false };
    system.update(&[up, up, InputEvent::Enter])
}

/// Chooses DISCARD, just above the focused CANCEL.
fn discard(system: System) -> System {
    system.update(&[InputEvent::Up { select: false }, InputEvent::Enter])
}

#[test]
fn a_clean_map_asks_nothing() {
    let fixture = Fixture::new("sys-map-unsaved-clean");
    let system = enter(with_map(&fixture), "unload");
    assert_eq!(unsaved_question(&system), None);
    assert!(system.maps.is_none(), "unloading closes the map");
}

#[test]
fn a_modified_map_is_named_in_the_question_before_a_command() {
    let fixture = Fixture::new("sys-map-unsaved-ask");
    let mut system = with_map(&fixture);
    paint(&mut system);
    let system = enter(system, "unload");
    assert_eq!(unsaved_question(&system), asks_about_unsaved());
    assert_eq!(question_message(&system)[0], "map a has unsaved changes.");
}

#[test]
fn cancel_keeps_the_map_and_its_edits() {
    let fixture = Fixture::new("sys-map-unsaved-cancel");
    let mut system = with_map(&fixture);
    paint(&mut system);
    let system = press_break(enter(system, "unload"));
    assert_eq!(unsaved_question(&system), None);
    assert!(system.maps.as_ref().is_some_and(MapDocument::is_modified));
    assert!(!map_dir(&fixture).join("bg0.bin").exists());
}

#[test]
fn discard_drops_the_map_without_writing() {
    let fixture = Fixture::new("sys-map-unsaved-discard");
    let mut system = with_map(&fixture);
    paint(&mut system);
    let system = discard(enter(system, "unload"));
    assert!(system.maps.is_none());
    assert!(!map_dir(&fixture).join("bg0.bin").exists());
}

#[test]
fn save_all_writes_the_map_then_runs_the_command() {
    let fixture = Fixture::new("sys-map-unsaved-save");
    let mut system = with_map(&fixture);
    paint(&mut system);
    let system = save_all(enter(system, "unload"));
    assert_eq!(unsaved_question(&system), None);
    assert_eq!(
        std::fs::read(map_dir(&fixture).join("bg0.bin")).unwrap(),
        [1, 0, 0, 0]
    );
    assert!(system.maps.is_none());
}

#[test]
fn quitting_asks_about_the_map_and_save_all_writes_it() {
    let fixture = Fixture::new("sys-map-unsaved-quit");
    let mut system = with_map(&fixture);
    paint(&mut system);
    system.request_exit();
    assert!(!system.exit_confirmed());
    let system = save_all(system.update(&[]));
    assert!(system.exit_confirmed());
    assert_eq!(
        std::fs::read(map_dir(&fixture).join("bg0.bin")).unwrap(),
        [1, 0, 0, 0]
    );
}

#[test]
fn a_map_file_changed_outside_glaux_is_settled_in_the_question() {
    let fixture = Fixture::new("sys-map-unsaved-conflict");
    let mut system = with_map(&fixture);
    paint(&mut system);
    // The layer appears on disk while the document believes it is absent.
    std::fs::write(map_dir(&fixture).join("bg0.bin"), [9, 0, 0, 0]).unwrap();

    let system = save_all(enter(system, "unload"));
    assert_eq!(
        unsaved_question(&system),
        Some((
            "FILE CONFLICT".to_owned(),
            vec!["OVERWRITE DISK".to_owned(), "CANCEL".to_owned()]
        ))
    );
    assert_eq!(
        std::fs::read(map_dir(&fixture).join("bg0.bin")).unwrap(),
        [9, 0, 0, 0],
        "nothing is written over it before the user chooses"
    );

    let system = system.update(&[InputEvent::Up { select: false }, InputEvent::Enter]);
    assert_eq!(unsaved_question(&system), None);
    assert_eq!(
        std::fs::read(map_dir(&fixture).join("bg0.bin")).unwrap(),
        [1, 0, 0, 0]
    );
    assert!(system.maps.is_none());
}

#[test]
fn cancelling_a_map_conflict_keeps_the_edits_and_the_disk() {
    let fixture = Fixture::new("sys-map-unsaved-conflict-cancel");
    let mut system = with_map(&fixture);
    paint(&mut system);
    std::fs::write(map_dir(&fixture).join("bg0.bin"), [9, 0, 0, 0]).unwrap();

    let system = save_all(enter(system, "unload"));
    let mut system = system.update(&[InputEvent::Enter]);
    assert_eq!(unsaved_question(&system), None);
    assert!(map(&mut system).is_modified());
    assert_eq!(
        std::fs::read(map_dir(&fixture).join("bg0.bin")).unwrap(),
        [9, 0, 0, 0]
    );
}
