use super::*;
use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_TILE_BYTES};
use pretty_assertions::assert_eq;

fn defaults() -> CartridgeVideoDefaults {
    CartridgeVideoDefaults::from_rom(&crate::rom::load().unwrap())
}

/// A document whose `sprites.bin` holds `tiles`.
fn document(tiles: Option<Vec<u8>>) -> SpriteDocument {
    let files = VideoFiles {
        obj_tiles: tiles,
        ..VideoFiles::default()
    };
    SpriteDocument::from_files(&files, &defaults()).unwrap()
}

/// Tile 0 of `rows`, one string of 8 hex digits per pixel row.
fn tile(rows: [&str; 8]) -> Vec<u8> {
    let pixels: Vec<u8> = rows
        .iter()
        .flat_map(|row| row.chars().map(|c| c.to_digit(16).unwrap() as u8))
        .collect();
    pixels.chunks(2).map(|p| (p[0] << 4) | p[1]).collect()
}

fn rows(document: &SpriteDocument, tile: usize) -> Vec<String> {
    (0..8)
        .map(|y| {
            (0..8)
                .map(|x| format!("{:X}", document.pixel(tile, x, y).unwrap()))
                .collect()
        })
        .collect()
}

#[test]
fn writes_change_the_native_bank_and_mark_the_document_modified() {
    let mut doc = document(None);
    assert!(!doc.is_modified());
    assert!(doc.set_pixel(3, 1, 2, 9));
    assert_eq!(doc.pixel(3, 1, 2), Some(9));
    assert_eq!(doc.tiles().as_bytes()[3 * OBJ_TILE_BYTES + 2 * 4], 0x09);
    assert!(doc.is_modified());
}

#[test]
fn writing_the_value_already_there_changes_nothing() {
    let mut doc = document(None);
    assert!(!doc.set_pixel(0, 0, 0, 0));
    assert!(!doc.is_modified());
    assert!(doc.set_pixel(0, 0, 0, 5));
    assert!(!doc.set_pixel(0, 0, 0, 5));
    assert!(doc.set_pixel(0, 0, 0, 0));
    assert!(!doc.is_modified(), "one write counted once, then restored");
}

#[test]
fn restoring_every_edited_pixel_makes_the_document_clean_again() {
    let mut bytes = vec![0x11; 2 * OBJ_TILE_BYTES];
    bytes[OBJ_TILE_BYTES] = 0x23;
    let mut doc = document(Some(bytes));
    doc.set_pixel(0, 4, 4, 7);
    doc.set_pixel(0, 4, 4, 8);
    doc.set_pixel(1, 0, 0, 0);
    doc.set_pixel(1, 1, 0, 0);
    doc.set_pixel(900, 7, 7, 15);
    assert!(doc.is_modified());

    doc.set_pixel(0, 4, 4, 1);
    doc.set_pixel(1, 0, 0, 2);
    doc.set_pixel(900, 7, 7, 0);
    assert!(doc.is_modified(), "one pixel is still different");
    doc.set_pixel(1, 1, 0, 3);
    assert!(!doc.is_modified());
}

#[test]
fn invalid_writes_change_nothing() {
    let mut doc = document(None);
    assert!(!doc.set_pixel(0, 8, 0, 1));
    assert!(!doc.set_pixel(0, 0, 0, 16));
    assert!(!doc.set_pixel(1024, 0, 0, 1));
    assert!(!doc.is_modified());
}

#[test]
fn fill_replaces_the_region_joined_by_edges() {
    let mut doc = document(Some(tile([
        "11100000", //
        "10100000", //
        "11102000", //
        "00002000", //
        "00000000", //
        "22222222", //
        "00000000", //
        "00000001", //
    ])));
    assert!(doc.fill(0, 7, 0, 3));
    assert_eq!(
        rows(&doc, 0),
        [
            "11133333", //
            "10133333", //
            "11132333", //
            "33332333", //
            "33333333", //
            "22222222", //
            "00000000", //
            "00000001", //
        ]
    );
    // The enclosed pixel touches the fill only diagonally.
    assert!(doc.fill(0, 1, 1, 4));
    assert_eq!(doc.pixel(0, 1, 1), Some(4));
    assert_eq!(doc.pixel(0, 0, 0), Some(1));
}

#[test]
fn fill_stays_inside_the_tile() {
    let mut doc = document(None);
    let tile = OBJ_GRID_COLUMNS + 1;
    assert!(doc.fill(tile, 7, 7, 6));
    assert!(rows(&doc, tile).iter().all(|row| row == "66666666"));
    for neighbour in [
        tile - 1,
        tile + 1,
        tile - OBJ_GRID_COLUMNS,
        tile + OBJ_GRID_COLUMNS,
    ] {
        assert!(
            rows(&doc, neighbour).iter().all(|row| row == "00000000"),
            "tile {neighbour}"
        );
    }
}

#[test]
fn filling_with_the_same_color_is_a_no_op() {
    let mut doc = document(Some(vec![0x55; OBJ_TILE_BYTES]));
    assert!(!doc.fill(0, 3, 3, 5));
    assert!(!doc.is_modified());
    assert!(!doc.fill(0, 8, 0, 1));
    assert!(!doc.fill(0, 0, 0, 16));
    assert!(!doc.is_modified());
}

#[test]
fn filling_back_to_the_original_color_is_clean() {
    let mut doc = document(Some(vec![0x55; OBJ_TILE_BYTES]));
    doc.fill(0, 0, 0, 2);
    assert!(doc.is_modified());
    doc.fill(0, 0, 0, 5);
    assert!(!doc.is_modified());
}

// History.

#[test]
fn an_edit_undoes_and_redoes_as_one() {
    let mut doc = document(None);
    doc.begin_edit();
    for x in 0..4 {
        doc.set_pixel(0, x, 0, 3);
    }
    doc.end_edit();
    assert!(doc.undo());
    assert_eq!(rows(&doc, 0)[0], "00000000");
    assert!(!doc.is_modified());
    assert!(doc.redo());
    assert_eq!(rows(&doc, 0)[0], "33330000");
    assert!(doc.is_modified());
    assert!(doc.undo());
    assert!(!doc.undo(), "one edit, one undo");
}

#[test]
fn a_pixel_written_repeatedly_in_one_edit_restores_its_first_value() {
    let mut doc = document(Some(vec![0x55; OBJ_TILE_BYTES]));
    doc.begin_edit();
    doc.set_pixel(0, 1, 1, 7);
    doc.set_pixel(0, 1, 1, 8);
    doc.set_pixel(0, 2, 1, 9);
    doc.end_edit();
    assert!(doc.undo());
    assert_eq!(rows(&doc, 0)[1], "55555555");
    assert!(doc.redo());
    assert_eq!(rows(&doc, 0)[1], "58955555");
}

#[test]
fn edits_that_change_nothing_leave_no_history() {
    let mut doc = document(Some(vec![0x55; OBJ_TILE_BYTES]));
    doc.begin_edit();
    doc.set_pixel(0, 0, 0, 5);
    doc.end_edit();
    // Changed and changed back within one edit.
    doc.begin_edit();
    doc.set_pixel(0, 0, 0, 2);
    doc.set_pixel(0, 0, 0, 5);
    doc.end_edit();
    assert!(!doc.fill(0, 0, 0, 5));
    assert!(!doc.can_undo());
    assert!(!doc.undo());
}

#[test]
fn a_fill_is_one_edit() {
    let mut doc = document(None);
    assert!(doc.fill(0, 3, 3, 4));
    assert!(doc.undo());
    assert_eq!(rows(&doc, 0).concat(), "0".repeat(64));
    assert!(!doc.can_undo());
}

#[test]
fn a_new_edit_after_undo_drops_the_redo_steps() {
    let mut doc = document(None);
    doc.set_pixel(0, 0, 0, 1);
    doc.set_pixel(0, 1, 0, 2);
    doc.undo();
    assert!(doc.can_redo());
    doc.set_pixel(0, 2, 0, 3);
    assert!(!doc.can_redo());
    assert!(!doc.redo());
    assert_eq!(rows(&doc, 0)[0], "10300000");
}

#[test]
fn an_edit_left_open_is_closed_by_the_next_one() {
    let mut doc = document(None);
    doc.begin_edit();
    doc.set_pixel(0, 0, 0, 1);
    doc.set_pixel(0, 1, 0, 1);
    // The stroke's release was lost; a new stroke starts.
    doc.begin_edit();
    doc.set_pixel(0, 0, 1, 2);
    assert!(doc.undo(), "undo closes the open edit first");
    assert_eq!(rows(&doc, 0)[..2], ["11000000", "00000000"]);
    assert!(doc.undo());
    assert_eq!(rows(&doc, 0)[0], "00000000");
}

#[test]
fn the_history_keeps_only_the_newest_edits() {
    let mut doc = document(None);
    for n in 0..HISTORY_LIMIT + 10 {
        doc.set_pixel(n % 1024, 0, 0, (n % 15 + 1) as u8);
    }
    let mut undone = 0;
    while doc.undo() {
        undone += 1;
    }
    assert_eq!(undone, HISTORY_LIMIT);
    for n in 0..10 {
        assert_ne!(doc.pixel(n, 0, 0), Some(0), "edit {n} was forgotten");
    }
    assert_eq!(doc.pixel(10, 0, 0), Some(0));
}

// Persistence.

use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use std::fs;
use std::path::PathBuf;

/// A loaded cartridge on disk whose `sprites.bin` holds `tiles`, if
/// given, beside an `obp.bin` of all zeros.
struct Disk {
    _fixture: Fixture,
    dir: PathBuf,
    cartridge: LoadedCartridge,
}

impl Disk {
    fn new(name: &str, tiles: Option<&[u8]>) -> Self {
        let fixture = Fixture::new(name);
        let dir = fixture.root().join("owl");
        write_cartridge(&dir, "Owl");
        fs::write(dir.join("obp.bin"), [0; 128]).unwrap();
        if let Some(tiles) = tiles {
            fs::write(dir.join("sprites.bin"), tiles).unwrap();
        }
        let cartridge = LoadedCartridge::load(&fixture.storage(), "owl").unwrap();
        Self {
            _fixture: fixture,
            dir,
            cartridge,
        }
    }

    fn document(&self) -> SpriteDocument {
        SpriteDocument::load(&self.cartridge, &defaults()).unwrap()
    }

    fn tiles(&self) -> Option<Vec<u8>> {
        fs::read(self.dir.join("sprites.bin")).ok()
    }

    fn write_tiles(&self, bytes: &[u8]) {
        fs::write(self.dir.join("sprites.bin"), bytes).unwrap();
    }

    fn remove_tiles(&self) {
        fs::remove_file(self.dir.join("sprites.bin")).unwrap();
    }

    /// Every file but `sprites.bin`, with its contents.
    fn others(&self) -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<_> = ["cartridge.ron", "code/main.luau", "obp.bin"]
            .into_iter()
            .map(|name| (name.to_owned(), fs::read(self.dir.join(name)).unwrap()))
            .collect();
        out.sort();
        out
    }
}

fn written(bytes: usize) -> Result<Saved, SaveError> {
    Ok(Saved::Written { bytes })
}

#[test]
fn saving_writes_the_native_bytes_and_keeps_the_file_length() {
    let original: Vec<u8> = (0..3 * OBJ_TILE_BYTES as u8).collect();
    let disk = Disk::new("doc-save", Some(&original));
    let others = disk.others();
    let mut doc = disk.document();
    doc.set_pixel(1, 0, 0, 0xf);
    // Erasing a whole tile inside the file keeps its bytes in the file.
    doc.fill(2, 0, 0, 0);
    for y in 0..8 {
        for x in 0..8 {
            doc.set_pixel(2, x, y, 0);
        }
    }

    assert_eq!(doc.save(&disk.cartridge), written(96));
    let saved = disk.tiles().unwrap();
    assert_eq!(saved, doc.tiles().as_bytes()[..96]);
    assert_eq!(saved[32], 0xf0 | (original[32] & 0x0f));
    assert_eq!(saved[64..], [0; 32]);
    assert!(!doc.is_modified());
    assert_eq!(doc.origin().file_bytes(), Some(&saved[..]));
    assert_eq!(disk.others(), others, "other assets untouched");

    let reloaded = disk.document();
    assert_eq!(reloaded.tiles(), doc.tiles());
    assert_eq!(doc.save(&disk.cartridge), Ok(Saved::NothingToSave));
}

#[test]
fn edits_past_the_end_grow_the_file_by_whole_tiles() {
    let disk = Disk::new("doc-grow", Some(&[0x11; OBJ_TILE_BYTES]));
    let mut doc = disk.document();
    doc.set_pixel(5, 7, 7, 2);
    assert_eq!(doc.save(&disk.cartridge), written(6 * OBJ_TILE_BYTES));
    let saved = disk.tiles().unwrap();
    assert_eq!(saved[..32], [0x11; 32]);
    assert_eq!(saved[32..191], [0; 159]);
    assert_eq!(saved[191], 0x02);
}

#[test]
fn a_missing_file_is_created_only_once_there_are_edits() {
    let disk = Disk::new("doc-create", None);
    let mut doc = disk.document();
    assert_eq!(doc.save(&disk.cartridge), Ok(Saved::NothingToSave));
    assert_eq!(disk.tiles(), None);
    doc.set_pixel(0, 0, 0, 1);
    doc.set_pixel(0, 0, 0, 0);
    assert_eq!(doc.save(&disk.cartridge), Ok(Saved::NothingToSave));
    assert_eq!(disk.tiles(), None);

    doc.set_pixel(1, 1, 0, 4);
    assert_eq!(doc.save(&disk.cartridge), written(64));
    assert_eq!(disk.tiles().unwrap()[32], 0x04);
}

#[test]
fn saving_keeps_the_history_and_undo_moves_off_the_saved_state() {
    let disk = Disk::new("doc-save-history", None);
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 1);
    doc.set_pixel(0, 1, 0, 2);
    doc.save(&disk.cartridge).unwrap();
    assert!(!doc.is_modified());

    assert!(doc.undo());
    assert!(doc.is_modified(), "the saved file has the undone pixel");
    assert!(doc.redo());
    assert!(!doc.is_modified(), "back to exactly what was saved");
    assert!(doc.undo());
    assert!(doc.undo());
    assert!(doc.is_modified());
    doc.save(&disk.cartridge).unwrap();
    assert_eq!(disk.tiles(), Some(vec![0; 32]), "the saved length stays");
    assert!(doc.redo());
    assert!(doc.is_modified());
}

#[test]
fn a_failed_write_changes_nothing() {
    let disk = Disk::new("doc-save-fail", Some(&[0x33; OBJ_TILE_BYTES]));
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 9);
    let path = disk.dir.join("sprites.bin");
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions.clone()).unwrap();

    let before = doc.clone();
    assert_eq!(
        doc.save(&disk.cartridge),
        Err(SaveError::Cartridge(CartridgeError::ObjTilesReadOnly))
    );
    assert_eq!(doc, before);
    assert!(doc.is_modified() && doc.can_undo());
    assert_eq!(disk.tiles(), Some(vec![0x33; 32]));

    #[expect(
        clippy::permissions_set_readonly_false,
        reason = "restores the test file so the fixture can be removed"
    )]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).unwrap();
    assert_eq!(doc.save(&disk.cartridge), written(32));
}

#[test]
fn an_unreadable_file_is_an_error_not_a_conflict() {
    let disk = Disk::new("doc-save-unreadable", None);
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 1);
    fs::create_dir(disk.dir.join("sprites.bin")).unwrap();
    assert_eq!(
        doc.save(&disk.cartridge),
        Err(SaveError::Cartridge(CartridgeError::ObjTilesNotAFile))
    );
    assert!(doc.is_modified());
}

#[test]
fn outside_changes_are_conflicts_by_kind() {
    /// Name, the file when read, the file at save time, and the kind.
    type Case<'a> = (&'a str, Option<&'a [u8]>, Option<&'a [u8]>, ConflictKind);
    let cases: [Case<'_>; 4] = [
        (
            "modified",
            Some(&[0x11; 32]),
            Some(&[0x22; 32]),
            ConflictKind::Modified,
        ),
        (
            "emptied",
            Some(&[0x11; 32]),
            Some(&[]),
            ConflictKind::Modified,
        ),
        ("deleted", Some(&[0x11; 32]), None, ConflictKind::Deleted),
        ("created", None, Some(&[]), ConflictKind::Created),
    ];
    for (name, original, now, kind) in cases {
        let disk = Disk::new(&format!("doc-conflict-{name}"), original);
        let mut doc = disk.document();
        doc.set_pixel(0, 0, 0, 7);
        match now {
            Some(bytes) => disk.write_tiles(bytes),
            None => disk.remove_tiles(),
        }
        let before = doc.clone();
        assert_eq!(
            doc.save(&disk.cartridge),
            Err(SaveError::Conflict(Conflict {
                kind,
                disk: DiskTiles(now.map(<[u8]>::to_vec)),
            })),
            "{name}"
        );
        assert_eq!(doc, before, "{name}");
        assert_eq!(disk.tiles().as_deref(), now, "{name}");
    }
}

#[test]
fn overwrite_replaces_exactly_the_version_shown() {
    let disk = Disk::new("doc-overwrite", Some(&[0x11; 32]));
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    disk.write_tiles(&[0x22; 64]);
    let Err(SaveError::Conflict(conflict)) = doc.save(&disk.cartridge) else {
        panic!("expected a conflict");
    };

    // A further change after the dialog appeared is not overwritten.
    disk.write_tiles(&[0x33; 64]);
    let Err(SaveError::Conflict(again)) = doc.overwrite(&disk.cartridge, &conflict.disk) else {
        panic!("expected a second conflict");
    };
    assert_eq!(again.disk, DiskTiles(Some(vec![0x33; 64])));
    assert_eq!(disk.tiles(), Some(vec![0x33; 64]));
    assert!(doc.is_modified());

    assert_eq!(doc.overwrite(&disk.cartridge, &again.disk), written(32));
    assert_eq!(disk.tiles().unwrap()[0], 0x71);
    assert!(!doc.is_modified());
    assert!(doc.can_undo(), "overwriting keeps the history");
    assert_eq!(doc.save(&disk.cartridge), Ok(Saved::NothingToSave));
}

#[test]
fn overwrite_recreates_a_deleted_file() {
    let disk = Disk::new("doc-overwrite-deleted", Some(&[0x11; 32]));
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    disk.remove_tiles();
    let Err(SaveError::Conflict(conflict)) = doc.save(&disk.cartridge) else {
        panic!("expected a conflict");
    };
    assert_eq!(conflict.kind, ConflictKind::Deleted);
    assert_eq!(doc.overwrite(&disk.cartridge, &conflict.disk), written(32));
    assert_eq!(disk.tiles().unwrap()[0], 0x71);
}

#[test]
fn reloading_discards_edits_and_history() {
    let disk = Disk::new("doc-reload", Some(&[0x11; 32]));
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    disk.write_tiles(&[0x22; 64]);
    doc.reload_tiles(&disk.cartridge).unwrap();
    assert_eq!(rows(&doc, 1)[0], "22222222");
    assert!(!doc.is_modified() && !doc.can_undo() && !doc.can_redo());
    assert_eq!(doc.origin().file_bytes(), Some(&[0x22; 64][..]));

    doc.set_pixel(0, 0, 0, 7);
    disk.remove_tiles();
    doc.reload_tiles(&disk.cartridge).unwrap();
    assert_eq!(
        rows(&doc, 0)[0],
        "00000000",
        "a deleted file reads as empty tiles"
    );
    assert_eq!(doc.origin().file_bytes(), None);
    assert_eq!(doc.obj().palettes(), disk.document().obj().palettes());
}

#[test]
fn a_failed_reload_keeps_the_document() {
    let disk = Disk::new("doc-reload-invalid", Some(&[0x11; 32]));
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    disk.write_tiles(&[0x22; 33]);
    let before = doc.clone();
    assert!(doc.reload_tiles(&disk.cartridge).is_err());
    assert_eq!(doc, before);
}

// Reopening the workspace.

#[test]
fn reopening_over_an_unchanged_file_keeps_edits_and_history() {
    let disk = Disk::new("doc-reopen-same", Some(&[0x11; 32]));
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    doc.undo();
    doc.set_pixel(0, 1, 0, 7);
    let obp: [u8; 128] = std::array::from_fn(|i| if i % 16 == 0 { 0 } else { 40 });
    fs::write(disk.dir.join("obp.bin"), obp).unwrap();
    assert_eq!(
        doc.reopen(&disk.cartridge, &defaults()),
        Reopened::Unchanged
    );
    assert!(doc.is_modified() && doc.can_undo());
    assert_eq!(rows(&doc, 0)[0], "17111111");
    assert_eq!(
        doc.obj().palettes(),
        disk.document().obj().palettes(),
        "read-only assets are read afresh"
    );
}

#[test]
fn reopening_never_drops_unsaved_edits() {
    let disk = Disk::new("doc-reopen-conflict", Some(&[0x11; 32]));
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    disk.write_tiles(&[0x22; 32]);
    let before = doc.clone();
    assert_eq!(
        doc.reopen(&disk.cartridge, &defaults()),
        Reopened::Conflict(Conflict {
            kind: ConflictKind::Modified,
            disk: DiskTiles(Some(vec![0x22; 32])),
        })
    );
    assert_eq!(doc, before);
}

#[test]
fn reopening_a_clean_document_over_a_changed_file_reloads_it() {
    let disk = Disk::new("doc-reopen-clean", Some(&[0x11; 32]));
    let mut doc = disk.document();
    disk.write_tiles(&[0x22; 32]);
    assert_eq!(
        doc.reopen(&disk.cartridge, &defaults()),
        Reopened::Reloaded {
            history_cleared: false
        }
    );
    assert_eq!(rows(&doc, 0)[0], "22222222");

    // Saved edits leave history that no longer fits a changed file.
    doc.set_pixel(0, 0, 0, 7);
    doc.save(&disk.cartridge).unwrap();
    disk.write_tiles(&[0x33; 32]);
    assert_eq!(
        doc.reopen(&disk.cartridge, &defaults()),
        Reopened::Reloaded {
            history_cleared: true
        }
    );
    assert!(!doc.can_undo());
    assert_eq!(rows(&doc, 0)[0], "33333333");
}

#[test]
fn reopening_over_unreadable_or_invalid_files_keeps_everything() {
    let disk = Disk::new("doc-reopen-fail", Some(&[0x11; 32]));
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    doc.save(&disk.cartridge).unwrap();
    let before = doc.clone();

    disk.write_tiles(&[0x22; 33]);
    assert!(matches!(
        doc.reopen(&disk.cartridge, &defaults()),
        Reopened::Failed(_)
    ));
    assert_eq!(doc, before);

    disk.remove_tiles();
    fs::create_dir(disk.dir.join("sprites.bin")).unwrap();
    assert!(matches!(
        doc.reopen(&disk.cartridge, &defaults()),
        Reopened::Failed(_)
    ));
    assert_eq!(doc, before);
}
