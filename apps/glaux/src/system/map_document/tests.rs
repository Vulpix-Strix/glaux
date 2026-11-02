use std::fs;
use std::path::PathBuf;

use super::*;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use pretty_assertions::assert_eq;

const BG0: MapFile = MapFile::Layer(BgLayer::Bg0);
const BG1: MapFile = MapFile::Layer(BgLayer::Bg1);
const BG2: MapFile = MapFile::Layer(BgLayer::Bg2);

const META: &str =
    r#"(format: 1, name: "Overworld", width: 4, height: 3, layers: (bg0: Some("ground")))"#;

fn entry(raw: u16) -> TileEntry {
    TileEntry::from_raw(raw).unwrap()
}

fn id() -> MapId {
    MapId::parse("overworld").unwrap()
}

/// The bytes of a 4x3 layer with `raw` at each `(x, y)`.
fn layer_bytes(cells: &[(usize, usize, u16)]) -> Vec<u8> {
    let mut layer = TileLayer::empty(4, 3).unwrap();
    for &(x, y, raw) in cells {
        layer.set(x, y, entry(raw));
    }
    layer.to_bytes()
}

fn files(layers: [Option<Vec<u8>>; 3]) -> MapFiles {
    MapFiles {
        meta: META.as_bytes().to_vec(),
        layers,
    }
}

fn document() -> MapDocument {
    MapDocument::from_files(id(), &files([None, None, None])).unwrap()
}

/// A cartridge directory holding the map `overworld`.
struct Disk {
    _fixture: Fixture,
    dir: PathBuf,
    cartridge: LoadedCartridge,
}

impl Disk {
    fn new(name: &str, layers: [Option<Vec<u8>>; 3]) -> Self {
        let fixture = Fixture::new(name);
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        let map = dir.join("maps/overworld");
        fs::create_dir_all(&map).unwrap();
        fs::write(map.join("map.ron"), META).unwrap();
        for (i, bytes) in layers.iter().enumerate() {
            if let Some(bytes) = bytes {
                fs::write(map.join(format!("bg{i}.bin")), bytes).unwrap();
            }
        }
        let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
        Self {
            _fixture: fixture,
            dir: map,
            cartridge,
        }
    }

    fn empty(name: &str) -> Self {
        Self::new(name, [None, None, None])
    }

    fn document(&self) -> MapDocument {
        MapDocument::load(&self.cartridge, &id()).unwrap()
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        fs::read(self.dir.join(name)).ok()
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        fs::write(self.dir.join(name), bytes).unwrap();
    }

    fn remove(&self, name: &str) {
        fs::remove_file(self.dir.join(name)).unwrap();
    }
}

fn paint(doc: &mut MapDocument, layer: BgLayer, x: usize, y: usize, raw: u16) -> bool {
    doc.set_cell(layer, x, y, entry(raw))
}

#[test]
fn a_missing_layer_file_is_an_empty_layer() {
    let doc = document();
    assert!(BgLayer::ALL.into_iter().all(|l| doc.layer(l).is_blank()));
    assert_eq!(doc.layer(BgLayer::Bg0).width(), 4);
    assert_eq!(doc.id(), &id());
    assert!(!doc.is_modified());
}

#[test]
fn a_layer_of_the_wrong_size_is_rejected_by_layer() {
    let result = MapDocument::from_files(id(), &files([None, Some(vec![0; 23]), None]));
    assert!(matches!(
        result,
        Err(MapError::Layer {
            layer: BgLayer::Bg1,
            ..
        })
    ));
}

#[test]
fn writing_a_cell_marks_only_its_layer_and_returns_whether_it_changed() {
    let mut doc = document();
    assert!(paint(&mut doc, BgLayer::Bg1, 2, 1, 7));
    assert!(!paint(&mut doc, BgLayer::Bg1, 2, 1, 7));
    assert!(!paint(&mut doc, BgLayer::Bg1, 4, 0, 7));
    assert!(!paint(&mut doc, BgLayer::Bg1, 0, 3, 7));
    assert_eq!(doc.modified_files().collect::<Vec<_>>(), [BG1]);
    assert_eq!(doc.layer(BgLayer::Bg1).get(2, 1), Some(entry(7)));
}

#[test]
fn restoring_a_cell_by_hand_makes_the_layer_clean() {
    let mut doc = document();
    paint(&mut doc, BgLayer::Bg0, 0, 0, 5);
    paint(&mut doc, BgLayer::Bg0, 0, 0, 0);
    assert!(!doc.is_modified());
}

#[test]
fn undo_and_redo_restore_cells_and_dirty_state() {
    let mut doc = document();
    paint(&mut doc, BgLayer::Bg0, 1, 1, 3);
    assert!(doc.is_modified());
    assert!(doc.undo());
    assert!(!doc.is_modified());
    assert!(doc.layer(BgLayer::Bg0).is_blank());
    assert!(doc.can_redo());
    assert!(doc.redo());
    assert_eq!(doc.layer(BgLayer::Bg0).get(1, 1), Some(entry(3)));
    assert!(doc.is_modified());
    assert!(!doc.redo());
}

#[test]
fn an_edit_across_layers_undoes_as_one() {
    let mut doc = document();
    doc.begin_edit();
    paint(&mut doc, BgLayer::Bg0, 0, 0, 1);
    paint(&mut doc, BgLayer::Bg2, 3, 2, 2);
    doc.end_edit();
    assert!(doc.undo());
    assert!(!doc.is_modified());
    assert!(!doc.undo());
}

#[test]
fn a_cell_written_twice_in_an_edit_keeps_the_first_before() {
    let mut doc = document();
    doc.begin_edit();
    paint(&mut doc, BgLayer::Bg0, 0, 0, 1);
    paint(&mut doc, BgLayer::Bg0, 0, 0, 2);
    paint(&mut doc, BgLayer::Bg0, 0, 0, 3);
    doc.end_edit();
    assert_eq!(doc.history.undo.len(), 1);
    assert_eq!(doc.history.undo[0].cells.len(), 1);
    doc.undo();
    assert_eq!(doc.layer(BgLayer::Bg0).get(0, 0), Some(TileEntry::EMPTY));
}

#[test]
fn an_edit_that_cancels_out_leaves_no_history() {
    let mut doc = document();
    doc.begin_edit();
    paint(&mut doc, BgLayer::Bg0, 0, 0, 1);
    paint(&mut doc, BgLayer::Bg0, 0, 0, 0);
    doc.end_edit();
    assert!(!doc.can_undo());
}

#[test]
fn a_new_edit_drops_the_redo_steps() {
    let mut doc = document();
    paint(&mut doc, BgLayer::Bg0, 0, 0, 1);
    doc.undo();
    paint(&mut doc, BgLayer::Bg0, 1, 0, 1);
    assert!(!doc.can_redo());
}

#[test]
fn the_history_forgets_the_oldest_edits_past_its_limits() {
    let mut doc = document();
    for i in 0..HISTORY_LIMIT + 10 {
        paint(
            &mut doc,
            BgLayer::Bg0,
            i % 4,
            (i / 4) % 3,
            (i % 1000 + 1) as u16,
        );
    }
    assert_eq!(doc.history.undo.len(), HISTORY_LIMIT);

    let big = MapFiles {
        meta: br#"(format: 1, name: "Big", width: 256, height: 256)"#.to_vec(),
        layers: [None, None, None],
    };
    let mut doc = MapDocument::from_files(id(), &big).unwrap();
    for layer in BgLayer::ALL {
        doc.begin_edit();
        for y in 0..256 {
            for x in 0..256 {
                doc.set_cell(layer, x, y, entry(1));
            }
        }
        doc.end_edit();
    }
    // Three edits of 65,536 changes: two fit the budget, the oldest goes.
    assert_eq!(doc.history.undo.len(), 2);
    assert!(doc.undo());
    assert!(doc.undo());
    assert!(!doc.undo());
}

#[test]
fn names_are_part_of_the_history_and_the_dirty_state() {
    let mut doc = document();
    assert!(doc.set_name("World"));
    assert!(!doc.set_name("World"));
    assert!(!doc.set_name("   "));
    assert!(doc.set_layer_name(BgLayer::Bg1, Some("trees")));
    assert!(doc.set_layer_name(BgLayer::Bg0, None));
    assert!(!doc.set_layer_name(BgLayer::Bg2, Some("")));
    assert_eq!(doc.modified_files().collect::<Vec<_>>(), [MapFile::Meta]);
    assert_eq!(doc.meta().layer_names, [None, Some("trees".into()), None]);

    while doc.undo() {}
    assert!(!doc.is_modified());
    assert_eq!(doc.meta().name, "Overworld");
    assert_eq!(doc.meta().layer_names[0].as_deref(), Some("ground"));
    while doc.redo() {}
    assert_eq!(doc.meta().name, "World");
}

#[test]
fn renaming_back_makes_the_metadata_clean() {
    let mut doc = document();
    doc.set_name("World");
    doc.set_name("Overworld");
    assert!(!doc.is_modified());
}

#[test]
fn a_rename_and_a_stroke_in_one_edit_undo_together() {
    let mut doc = document();
    doc.begin_edit();
    doc.set_name("World");
    paint(&mut doc, BgLayer::Bg0, 0, 0, 1);
    doc.set_name("Planet");
    doc.end_edit();
    assert!(doc.undo());
    assert!(!doc.is_modified());
    assert_eq!(doc.meta().name, "Overworld");
}

#[test]
fn saving_nothing_writes_nothing() {
    let disk = Disk::empty("map-save-none");
    assert_eq!(
        disk.document().save(&disk.cartridge),
        Ok(Saved::NothingToSave)
    );
}

#[test]
fn saving_writes_only_the_modified_files_and_layers_before_the_metadata() {
    let disk = Disk::empty("map-save-order");
    let mut doc = disk.document();
    doc.set_name("World");
    paint(&mut doc, BgLayer::Bg2, 3, 2, 0x1234);
    paint(&mut doc, BgLayer::Bg0, 0, 0, 9);
    let saved = doc.save(&disk.cartridge).unwrap();
    let layer_len = 4 * 3 * 2;
    assert_eq!(
        saved,
        Saved::Written(vec![
            SavedFile {
                file: BG0,
                write: FileWrite::Written(layer_len)
            },
            SavedFile {
                file: BG2,
                write: FileWrite::Written(layer_len)
            },
            SavedFile {
                file: MapFile::Meta,
                write: FileWrite::Written(doc.meta().to_bytes().len())
            },
        ])
    );
    assert_eq!(
        disk.read("bg0.bin"),
        Some(doc.layer(BgLayer::Bg0).to_bytes())
    );
    assert_eq!(disk.read("bg1.bin"), None);
    let meta = MapMeta::parse(&disk.read("map.ron").unwrap()).unwrap();
    assert_eq!(meta.name, "World");
    assert!(!doc.is_modified());
    assert_eq!(doc.save(&disk.cartridge), Ok(Saved::NothingToSave));
}

#[test]
fn saving_keeps_the_history_and_undo_afterwards_is_unsaved() {
    let disk = Disk::empty("map-save-history");
    let mut doc = disk.document();
    paint(&mut doc, BgLayer::Bg0, 0, 0, 1);
    doc.save(&disk.cartridge).unwrap();
    assert!(doc.undo());
    assert!(doc.is_file_modified(BG0));
}

#[test]
fn a_layer_edited_to_empty_loses_its_file() {
    let disk = Disk::new(
        "map-save-remove",
        [Some(layer_bytes(&[(1, 1, 5)])), None, None],
    );
    let mut doc = disk.document();
    paint(&mut doc, BgLayer::Bg0, 1, 1, 0);
    assert_eq!(
        doc.save(&disk.cartridge),
        Ok(Saved::Written(vec![SavedFile {
            file: BG0,
            write: FileWrite::Removed
        }]))
    );
    assert_eq!(disk.read("bg0.bin"), None);
    assert!(disk.read("map.ron").is_some());
    // The next edit creates the file again.
    paint(&mut doc, BgLayer::Bg0, 2, 2, 4);
    doc.save(&disk.cartridge).unwrap();
    assert!(disk.read("bg0.bin").is_some());
}

#[test]
fn an_untouched_all_zero_file_is_left_alone() {
    let disk = Disk::new("map-keep-zero", [Some(layer_bytes(&[])), None, None]);
    let mut doc = disk.document();
    doc.set_name("World");
    doc.save(&disk.cartridge).unwrap();
    assert_eq!(disk.read("bg0.bin"), Some(layer_bytes(&[])));
}

#[test]
fn a_changed_file_blocks_the_whole_save() {
    let disk = Disk::new(
        "map-conflict",
        [Some(layer_bytes(&[(0, 0, 1)])), None, None],
    );
    let mut doc = disk.document();
    paint(&mut doc, BgLayer::Bg0, 1, 0, 2);
    paint(&mut doc, BgLayer::Bg1, 0, 0, 3);
    disk.write("bg0.bin", &layer_bytes(&[(0, 0, 9)]));

    let err = doc.save(&disk.cartridge).unwrap_err();
    assert_eq!(err.written, vec![]);
    assert_eq!(
        err.cause,
        SaveFailure::Conflict(vec![Conflict {
            kind: ConflictKind::Modified,
            disk: DiskFile {
                file: BG0,
                bytes: Some(layer_bytes(&[(0, 0, 9)]))
            }
        }])
    );
    // bg1 was not written either.
    assert_eq!(disk.read("bg1.bin"), None);
    assert!(doc.is_modified());
}

#[test]
fn conflicts_are_told_apart_by_kind() {
    let disk = Disk::new(
        "map-conflict-kinds",
        [Some(layer_bytes(&[(0, 0, 1)])), None, None],
    );
    let mut doc = disk.document();
    doc.set_name("World");
    paint(&mut doc, BgLayer::Bg0, 1, 0, 2);
    paint(&mut doc, BgLayer::Bg1, 0, 0, 3);
    disk.remove("bg0.bin");
    disk.write("bg1.bin", &layer_bytes(&[(3, 2, 8)]));
    disk.write(
        "map.ron",
        br#"(format: 1, name: "Other", width: 4, height: 3)"#,
    );

    let Err(SaveError {
        cause: SaveFailure::Conflict(conflicts),
        ..
    }) = doc.save(&disk.cartridge)
    else {
        panic!("expected conflicts");
    };
    let kinds: Vec<_> = conflicts.iter().map(|c| (c.file(), c.kind)).collect();
    assert_eq!(
        kinds,
        [
            (BG0, ConflictKind::Deleted),
            (BG1, ConflictKind::Created),
            (MapFile::Meta, ConflictKind::Modified),
        ]
    );
}

#[test]
fn removing_a_layer_notices_a_changed_file() {
    let disk = Disk::new(
        "map-remove-conflict",
        [Some(layer_bytes(&[(1, 1, 5)])), None, None],
    );
    let mut doc = disk.document();
    paint(&mut doc, BgLayer::Bg0, 1, 1, 0);
    disk.write("bg0.bin", &layer_bytes(&[(1, 1, 6)]));
    assert!(matches!(
        doc.save(&disk.cartridge),
        Err(SaveError {
            cause: SaveFailure::Conflict(_),
            ..
        })
    ));
    assert_eq!(disk.read("bg0.bin"), Some(layer_bytes(&[(1, 1, 6)])));
}

#[test]
fn overwriting_replaces_exactly_the_file_that_was_seen() {
    let disk = Disk::new(
        "map-overwrite",
        [Some(layer_bytes(&[(0, 0, 1)])), None, None],
    );
    let mut doc = disk.document();
    paint(&mut doc, BgLayer::Bg0, 1, 0, 2);
    let outside = layer_bytes(&[(0, 0, 9)]);
    disk.write("bg0.bin", &outside);
    let seen = vec![DiskFile {
        file: BG0,
        bytes: Some(outside.clone()),
    }];

    // The file changed again after it was seen.
    disk.write("bg0.bin", &layer_bytes(&[(0, 0, 10)]));
    assert!(doc.overwrite(&disk.cartridge, &seen).is_err());

    disk.write("bg0.bin", &outside);
    assert!(doc.overwrite(&disk.cartridge, &seen).is_ok());
    assert_eq!(
        disk.read("bg0.bin"),
        Some(doc.layer(BgLayer::Bg0).to_bytes())
    );
}

#[test]
fn an_unreadable_file_stops_the_save_before_anything_is_written() {
    let disk = Disk::empty("map-unreadable");
    let mut doc = disk.document();
    paint(&mut doc, BgLayer::Bg0, 0, 0, 1);
    paint(&mut doc, BgLayer::Bg1, 0, 0, 1);
    // A directory where bg1.bin goes cannot be checked.
    fs::create_dir(disk.dir.join("bg1.bin")).unwrap();
    let err = doc.save(&disk.cartridge).unwrap_err();
    assert!(matches!(err.cause, SaveFailure::Map(_)));
    assert_eq!(err.written, vec![]);
    assert_eq!(disk.read("bg0.bin"), None);
    assert_eq!(doc.modified_files().collect::<Vec<_>>(), [BG0, BG1]);

    fs::remove_dir(disk.dir.join("bg1.bin")).unwrap();
    let saved = doc.save(&disk.cartridge).unwrap();
    assert_eq!(
        saved,
        Saved::Written(vec![
            SavedFile {
                file: BG0,
                write: FileWrite::Written(24)
            },
            SavedFile {
                file: BG1,
                write: FileWrite::Written(24)
            },
        ])
    );
    assert!(!doc.is_modified());
}

#[test]
fn reopening_unchanged_files_keeps_everything() {
    let disk = Disk::empty("map-reopen-same");
    let mut doc = disk.document();
    paint(&mut doc, BgLayer::Bg0, 0, 0, 1);
    assert_eq!(doc.reopen(&disk.cartridge), Ok(Reopened::default()));
    assert!(doc.can_undo());
}

#[test]
fn reopening_reloads_a_changed_clean_file_and_drops_its_history() {
    let disk = Disk::empty("map-reopen-reload");
    let mut doc = disk.document();
    paint(&mut doc, BgLayer::Bg0, 0, 0, 1);
    doc.save(&disk.cartridge).unwrap();
    paint(&mut doc, BgLayer::Bg1, 0, 0, 2);
    doc.save(&disk.cartridge).unwrap();

    disk.write("bg0.bin", &layer_bytes(&[(3, 2, 6)]));
    let reopened = doc.reopen(&disk.cartridge).unwrap();
    assert_eq!(reopened.reloaded, vec![BG0]);
    assert!(reopened.history_trimmed);
    assert!(reopened.conflicts.is_empty());
    assert_eq!(doc.layer(BgLayer::Bg0).get(3, 2), Some(entry(6)));
    assert!(!doc.is_modified());
    // The bg1 edit came after the bg0 one, so only the older step goes.
    assert!(doc.undo());
    assert!(!doc.undo());
}

#[test]
fn reopening_never_drops_unsaved_edits() {
    let disk = Disk::empty("map-reopen-conflict");
    let mut doc = disk.document();
    paint(&mut doc, BgLayer::Bg0, 0, 0, 1);
    let outside = layer_bytes(&[(3, 2, 6)]);
    disk.write("bg0.bin", &outside);
    disk.write("bg1.bin", &outside);

    let reopened = doc.reopen(&disk.cartridge).unwrap();
    assert_eq!(reopened.reloaded, vec![BG1]);
    assert_eq!(
        reopened.conflicts,
        vec![Conflict {
            kind: ConflictKind::Created,
            disk: DiskFile {
                file: BG0,
                bytes: Some(outside)
            }
        }]
    );
    assert_eq!(doc.layer(BgLayer::Bg0).get(0, 0), Some(entry(1)));
    assert!(doc.is_file_modified(BG0));
}

#[test]
fn a_resize_on_disk_replaces_a_clean_document_and_conflicts_with_a_dirty_one() {
    let disk = Disk::empty("map-reopen-resize");
    let resized = br#"(format: 1, name: "Overworld", width: 2, height: 2)"#;

    let mut dirty = disk.document();
    paint(&mut dirty, BgLayer::Bg0, 0, 0, 1);
    disk.write("map.ron", resized);
    let reopened = dirty.reopen(&disk.cartridge).unwrap();
    assert_eq!(reopened.conflicts.len(), 1);
    assert_eq!(reopened.conflicts[0].file(), MapFile::Meta);
    assert_eq!(dirty.meta().width, 4);

    let mut clean = document();
    let reopened = clean.reopen(&disk.cartridge).unwrap();
    assert_eq!(reopened.reloaded, vec![MapFile::Meta]);
    assert_eq!((clean.meta().width, clean.meta().height), (2, 2));
    assert_eq!(clean.layer(BgLayer::Bg0).cell_count(), 4);
}

#[test]
fn reopening_an_invalid_file_changes_nothing() {
    let disk = Disk::empty("map-reopen-invalid");
    let mut doc = disk.document();
    paint(&mut doc, BgLayer::Bg0, 0, 0, 1);
    disk.write("bg1.bin", &[1, 2, 3]);
    assert!(matches!(
        doc.reopen(&disk.cartridge),
        Err(MapError::Layer {
            layer: BgLayer::Bg1,
            ..
        })
    ));
    assert!(doc.layer(BgLayer::Bg1).is_blank());
    assert!(doc.is_file_modified(BG0));
}
