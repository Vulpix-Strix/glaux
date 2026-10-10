use super::*;
use glaux_machine::obj_palette::ObjPalettes;
use glaux_machine::palette::{PALETTE_RGB888_BYTES, Palette};
use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_TILE_BYTES};
use pretty_assertions::assert_eq;

const TILES: VideoAsset = VideoAsset::ObjTiles;
const ATTRIBUTES: VideoAsset = VideoAsset::ObjAttributes;
const OBP: VideoAsset = VideoAsset::ObjPalettes;
const PALETTE: VideoAsset = VideoAsset::MasterPalette;

const BLUE: Rgb = Rgb::from_hex(0x3978a8);
const RED: Rgb = Rgb::from_hex(0xe6482e);

fn bank(n: u8) -> ObjBank {
    ObjBank::new(n).unwrap()
}

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

// Palettes and attributes.

/// A `palette.bin` that is the provisional palette with master 42 blue.
fn blue_42() -> Vec<u8> {
    let mut palette = Palette::provisional();
    palette.set(42, BLUE);
    palette.to_rgb888().to_vec()
}

fn modified(doc: &SpriteDocument) -> Vec<VideoAsset> {
    doc.modified_assets().collect()
}

#[test]
fn every_asset_is_clean_again_once_restored_by_hand() {
    let files = VideoFiles {
        master_palette: Some(blue_42()),
        ..VideoFiles::default()
    };
    let mut doc = SpriteDocument::from_files(&files, &defaults()).unwrap();
    let obp_2_4 = doc.obj().palettes().resolve(bank(2), 4).unwrap();

    assert!(doc.set_master_color(42, RED));
    assert_eq!(modified(&doc), [PALETTE]);
    assert!(doc.set_master_color(42, BLUE));
    assert!(!doc.is_modified());

    assert!(doc.set_tile_bank(5, bank(3)));
    assert_eq!(modified(&doc), [ATTRIBUTES]);
    assert!(doc.set_tile_bank(5, bank(6)));
    assert!(doc.set_tile_bank(5, ObjBank::ZERO));
    assert!(!doc.is_modified());

    assert!(doc.set_obj_color(bank(2), 4, 200));
    assert!(doc.set_pixel(0, 0, 0, 1));
    assert_eq!(modified(&doc), [TILES, OBP]);
    assert!(doc.set_obj_color(bank(2), 4, obp_2_4));
    assert_eq!(modified(&doc), [TILES]);
    assert!(doc.can_undo(), "manual restores are edits of their own");
}

#[test]
fn an_edit_across_assets_undoes_and_redoes_as_one() {
    let mut doc = document(None);
    let original = doc.clone();
    doc.begin_edit();
    doc.set_pixel(3, 1, 1, 9);
    doc.set_tile_bank(3, bank(4));
    doc.set_obj_color(bank(4), 9, 77);
    doc.set_master_color(77, RED);
    doc.set_master_color(77, BLUE);
    doc.end_edit();
    let edited = doc.clone();
    assert_eq!(modified(&doc), [TILES, ATTRIBUTES, OBP, PALETTE]);

    assert!(doc.undo());
    assert_eq!(doc.video(), original.video());
    assert!(!doc.is_modified() && !doc.can_undo());
    assert!(doc.redo());
    assert_eq!(doc.video(), edited.video());
    assert_eq!(doc.video().palette.get(77), BLUE, "the last write wins");
    assert_eq!(modified(&doc), [TILES, ATTRIBUTES, OBP, PALETTE]);
}

#[test]
fn rejected_or_unchanged_palette_writes_leave_no_history() {
    let mut doc = document(None);
    let current = doc.video().palette.get(42);
    assert!(!doc.set_master_color(42, current));
    assert!(!doc.set_obj_color(bank(1), 0, 9), "local 0 is transparent");
    assert!(!doc.set_obj_color(bank(1), 16, 9));
    assert!(!doc.set_tile_bank(1024, bank(1)));
    assert!(!doc.set_tile_bank(0, ObjBank::ZERO));
    assert_eq!(doc.obj().palettes().as_bytes()[16], 0);

    doc.begin_edit();
    doc.set_master_color(42, RED);
    doc.set_tile_bank(9, bank(2));
    doc.set_master_color(42, current);
    doc.set_tile_bank(9, ObjBank::ZERO);
    doc.end_edit();
    assert!(!doc.is_modified() && !doc.can_undo());
}

#[test]
fn a_new_palette_edit_drops_the_redo_steps() {
    let mut doc = document(None);
    doc.set_pixel(0, 0, 0, 1);
    doc.undo();
    assert!(doc.can_redo());
    doc.set_master_color(1, RED);
    assert!(!doc.can_redo());
    assert!(doc.undo());
    assert!(!doc.undo(), "the undone pixel edit is gone");
}

#[test]
fn the_history_change_budget_drops_old_edits_but_keeps_the_newest() {
    let mut doc = document(None);
    doc.set_pixel(0, 0, 0, 1);
    // Every pixel of the bank and one master color: one change over the
    // budget, in one edit.
    doc.begin_edit();
    for tile in 0..1024 {
        for y in 0..8 {
            for x in 0..8 {
                doc.set_pixel(tile, x, y, 2);
            }
        }
    }
    doc.set_master_color(0, RED);
    doc.end_edit();

    assert!(doc.undo());
    assert!(!doc.undo(), "the older edit was forgotten");
    assert_eq!(doc.pixel(1023, 7, 7), Some(0));
    assert_eq!(doc.pixel(0, 0, 0), Some(1), "only the kept edit was undone");
}

// Persistence.

use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

/// A loaded cartridge on disk.
struct Disk {
    _fixture: Fixture,
    dir: PathBuf,
    cartridge: LoadedCartridge,
}

impl Disk {
    /// A cartridge whose `sprites.bin` holds `tiles`, if given, beside an
    /// `obp.bin` of all zeros.
    fn new(name: &str, tiles: Option<&[u8]>) -> Self {
        let obp = [0; 128];
        match tiles {
            Some(tiles) => Self::with(name, &[(TILES, tiles), (OBP, &obp)]),
            None => Self::with(name, &[(OBP, &obp)]),
        }
    }

    /// A cartridge holding exactly `assets` beside its manifest and entry.
    fn with(name: &str, assets: &[(VideoAsset, &[u8])]) -> Self {
        let fixture = Fixture::new(name);
        let dir = fixture.root().join("owl");
        write_cartridge(&dir, "Owl");
        for (asset, bytes) in assets {
            fs::write(dir.join(asset.file_name()), bytes).unwrap();
        }
        let cartridge = LoadedCartridge::load(&fixture.storage(), "owl").unwrap();
        Self {
            _fixture: fixture,
            dir,
            cartridge,
        }
    }

    /// A cartridge with every asset file: tile 0 solid local 1, tile 0 on
    /// OBP0 in an 8-byte `objattr.bin`, linear banks and master 42 blue.
    fn full(name: &str) -> Self {
        Self::with(
            name,
            &[
                (TILES, &[0x11; 32]),
                (ATTRIBUTES, &[0; 8]),
                (OBP, ObjPalettes::linear().as_bytes()),
                (PALETTE, &blue_42()),
            ],
        )
    }

    fn document(&self) -> SpriteDocument {
        SpriteDocument::load(&self.cartridge, &defaults()).unwrap()
    }

    fn path(&self, asset: VideoAsset) -> PathBuf {
        self.dir.join(asset.file_name())
    }

    fn read(&self, asset: VideoAsset) -> Option<Vec<u8>> {
        fs::read(self.path(asset)).ok()
    }

    fn write(&self, asset: VideoAsset, bytes: &[u8]) {
        fs::write(self.path(asset), bytes).unwrap();
    }

    fn remove(&self, asset: VideoAsset) {
        fs::remove_file(self.path(asset)).unwrap();
    }

    fn tiles(&self) -> Option<Vec<u8>> {
        self.read(TILES)
    }

    /// Every file of the cartridge with its contents and modification
    /// time.
    fn snapshot(&self) -> Vec<(String, Vec<u8>, SystemTime)> {
        let mut out: Vec<_> = ["cartridge.ron", "code/main.luau"]
            .into_iter()
            .map(str::to_owned)
            .chain(VideoAsset::ALL.map(|asset| asset.file_name().to_owned()))
            .filter_map(|name| {
                let path = self.dir.join(&name);
                let modified = fs::metadata(&path).ok()?.modified().unwrap();
                Some((name, fs::read(path).unwrap(), modified))
            })
            .collect();
        out.sort();
        out
    }

    /// [`snapshot`](Self::snapshot) without `asset`'s file.
    fn others(&self, asset: VideoAsset) -> Vec<(String, Vec<u8>, SystemTime)> {
        let mut out = self.snapshot();
        out.retain(|(name, ..)| name != asset.file_name());
        out
    }

    fn set_readonly(&self, asset: VideoAsset, readonly: bool) {
        let path = self.path(asset);
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        // Both directions are needed: a test makes a file read-only, then
        // writable again so the save can be retried and the fixture removed.
        permissions.set_readonly(readonly);
        fs::set_permissions(&path, permissions).unwrap();
    }
}

fn written(files: &[(VideoAsset, usize)]) -> Result<Saved, SaveError> {
    Ok(Saved::Written(
        files
            .iter()
            .map(|&(asset, bytes)| SavedFile { asset, bytes })
            .collect(),
    ))
}

fn conflict(kind: ConflictKind, asset: VideoAsset, bytes: Option<&[u8]>) -> Conflict {
    Conflict {
        kind,
        disk: DiskAsset {
            asset,
            bytes: bytes.map(<[u8]>::to_vec),
        },
    }
}

#[test]
fn saving_writes_the_native_bytes_and_keeps_the_file_length() {
    let original: Vec<u8> = (0..3 * OBJ_TILE_BYTES as u8).collect();
    let disk = Disk::new("doc-save", Some(&original));
    let others = disk.others(TILES);
    let mut doc = disk.document();
    doc.set_pixel(1, 0, 0, 0xf);
    // Erasing a whole tile inside the file keeps its bytes in the file.
    doc.fill(2, 0, 0, 0);
    for y in 0..8 {
        for x in 0..8 {
            doc.set_pixel(2, x, y, 0);
        }
    }

    assert_eq!(doc.save(&disk.cartridge), written(&[(TILES, 96)]));
    let saved = disk.tiles().unwrap();
    assert_eq!(saved, doc.tiles().as_bytes()[..96]);
    assert_eq!(saved[32], 0xf0 | (original[32] & 0x0f));
    assert_eq!(saved[64..], [0; 32]);
    assert!(!doc.is_modified());
    assert_eq!(doc.origin().file_bytes(TILES).as_deref(), Some(&saved[..]));
    assert_eq!(disk.others(TILES), others, "other files untouched");

    let reloaded = disk.document();
    assert_eq!(reloaded.tiles(), doc.tiles());
    assert_eq!(doc.save(&disk.cartridge), Ok(Saved::NothingToSave));
}

#[test]
fn edits_past_the_end_grow_the_file_by_whole_tiles() {
    let disk = Disk::new("doc-grow", Some(&[0x11; OBJ_TILE_BYTES]));
    let mut doc = disk.document();
    doc.set_pixel(5, 7, 7, 2);
    assert_eq!(
        doc.save(&disk.cartridge),
        written(&[(TILES, 6 * OBJ_TILE_BYTES)])
    );
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
    assert_eq!(doc.save(&disk.cartridge), written(&[(TILES, 64)]));
    assert_eq!(disk.tiles().unwrap()[32], 0x04);
}

#[test]
fn only_modified_assets_are_written() {
    let disk = Disk::full("doc-save-one");
    let others = disk.others(PALETTE);
    let mut doc = disk.document();
    doc.set_master_color(42, RED);
    assert_eq!(
        doc.save(&disk.cartridge),
        written(&[(PALETTE, PALETTE_RGB888_BYTES)])
    );
    let saved = disk.read(PALETTE).unwrap();
    assert_eq!(saved[42 * 3..43 * 3], [0xe6, 0x48, 0x2e]);
    let mut expected = blue_42();
    expected[42 * 3..43 * 3].copy_from_slice(&saved[42 * 3..43 * 3]);
    assert_eq!(saved, expected, "every other color as it was");
    assert_eq!(disk.others(PALETTE), others, "other files untouched");
    assert!(!doc.is_modified());
}

#[test]
fn absent_palette_files_are_created_only_from_real_changes() {
    let disk = Disk::with("doc-create-palettes", &[]);
    let mut doc = disk.document();
    let defaults_doc = doc.clone();
    doc.set_master_color(42, RED);
    doc.set_obj_color(bank(1), 3, 42);
    doc.set_tile_bank(5, bank(1));
    doc.set_tile_bank(9, bank(2));
    doc.set_tile_bank(9, ObjBank::ZERO);
    assert_eq!(
        doc.save(&disk.cartridge),
        written(&[(PALETTE, PALETTE_RGB888_BYTES), (OBP, 128), (ATTRIBUTES, 6)])
    );

    let mut palette = defaults_doc.video().palette.clone();
    palette.set(42, RED);
    assert_eq!(disk.read(PALETTE), Some(palette.to_rgb888().to_vec()));
    let mut obp = defaults_doc.obj().palettes().clone();
    obp.set(bank(1), 3, 42);
    assert_eq!(disk.read(OBP), Some(obp.as_bytes().to_vec()));
    assert_eq!(disk.read(ATTRIBUTES), Some(vec![0, 0, 0, 0, 0, 1]));
    assert_eq!(disk.tiles(), None, "unchanged tiles create no file");
    assert_eq!(disk.document().video(), doc.video());
}

#[test]
fn attributes_keep_their_file_length_and_bytes() {
    let disk = Disk::with("doc-attributes", &[(ATTRIBUTES, &[1, 2, 3, 0, 0, 0, 0, 0])]);
    let mut doc = disk.document();
    doc.set_tile_bank(1, bank(7));
    assert_eq!(doc.save(&disk.cartridge), written(&[(ATTRIBUTES, 8)]));
    assert_eq!(disk.read(ATTRIBUTES), Some(vec![1, 7, 3, 0, 0, 0, 0, 0]));
}

#[test]
fn several_assets_save_together_in_a_fixed_order() {
    let disk = Disk::full("doc-save-several");
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 2);
    doc.set_obj_color(ObjBank::ZERO, 1, 42);
    assert_eq!(
        doc.save(&disk.cartridge),
        written(&[(OBP, 128), (TILES, 32)])
    );
    assert_eq!(disk.tiles().unwrap()[0], 0x21);
    assert_eq!(disk.read(OBP).unwrap()[1], 42);
    assert_eq!(disk.document().video(), doc.video());
}

#[test]
fn saving_keeps_the_history_and_undo_moves_off_the_saved_state() {
    let disk = Disk::new("doc-save-history", None);
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 1);
    doc.set_master_color(3, RED);
    doc.save(&disk.cartridge).unwrap();
    assert!(!doc.is_modified());

    assert!(doc.undo());
    assert_eq!(modified(&doc), [PALETTE], "the saved file has the color");
    assert!(doc.redo());
    assert!(!doc.is_modified(), "back to exactly what was saved");
    assert!(doc.undo());
    assert!(doc.undo());
    assert_eq!(modified(&doc), [TILES, PALETTE]);
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
    disk.set_readonly(TILES, true);

    let before = doc.clone();
    assert_eq!(
        doc.save(&disk.cartridge),
        Err(SaveError {
            written: Vec::new(),
            cause: SaveFailure::Cartridge(CartridgeError::AssetReadOnly(TILES)),
        })
    );
    assert_eq!(doc, before);
    assert!(doc.is_modified() && doc.can_undo());
    assert_eq!(disk.tiles(), Some(vec![0x33; 32]));

    disk.set_readonly(TILES, false);
    assert_eq!(doc.save(&disk.cartridge), written(&[(TILES, 32)]));
}

#[test]
fn a_failure_partway_keeps_what_was_saved_and_a_retry_saves_the_rest() {
    let disk = Disk::full("doc-save-partial");
    let mut doc = disk.document();
    doc.begin_edit();
    doc.set_master_color(42, RED);
    doc.set_obj_color(bank(1), 1, 42);
    doc.end_edit();
    disk.set_readonly(OBP, true);

    assert_eq!(
        doc.save(&disk.cartridge),
        Err(SaveError {
            written: vec![SavedFile {
                asset: PALETTE,
                bytes: PALETTE_RGB888_BYTES,
            }],
            cause: SaveFailure::Cartridge(CartridgeError::AssetReadOnly(OBP)),
        })
    );
    assert_eq!(disk.read(PALETTE).unwrap()[42 * 3], 0xe6);
    assert_eq!(
        disk.read(OBP),
        Some(ObjPalettes::linear().as_bytes().to_vec())
    );
    assert_eq!(modified(&doc), [OBP]);
    assert!(doc.can_undo(), "the history is kept");

    let palette = disk.snapshot();
    disk.set_readonly(OBP, false);
    assert_eq!(doc.save(&disk.cartridge), written(&[(OBP, 128)]));
    assert_eq!(disk.read(OBP).unwrap()[16 + 1], 42);
    assert_eq!(
        disk.others(OBP),
        palette
            .into_iter()
            .filter(|(name, ..)| name != "obp.bin")
            .collect::<Vec<_>>(),
        "the retry wrote only obp.bin"
    );
    assert!(!doc.is_modified());
}

#[test]
fn an_unreadable_file_is_an_error_not_a_conflict() {
    let disk = Disk::new("doc-save-unreadable", None);
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 1);
    fs::create_dir(disk.path(TILES)).unwrap();
    assert_eq!(
        doc.save(&disk.cartridge),
        Err(SaveError {
            written: Vec::new(),
            cause: SaveFailure::Cartridge(CartridgeError::AssetNotAFile(TILES)),
        })
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
            Some(bytes) => disk.write(TILES, bytes),
            None => disk.remove(TILES),
        }
        let before = doc.clone();
        assert_eq!(
            doc.save(&disk.cartridge),
            Err(SaveError {
                written: Vec::new(),
                cause: SaveFailure::Conflict(vec![conflict(kind, TILES, now)]),
            }),
            "{name}"
        );
        assert_eq!(doc, before, "{name}");
        assert_eq!(disk.tiles().as_deref(), now, "{name}");
    }
}

#[test]
fn any_conflict_found_before_writing_writes_nothing() {
    let disk = Disk::full("doc-conflict-several");
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    doc.set_obj_color(bank(1), 1, 9);
    doc.set_master_color(42, RED);
    disk.write(OBP, &[0; 128]);
    disk.remove(PALETTE);
    let before = disk.snapshot();

    assert_eq!(
        doc.save(&disk.cartridge),
        Err(SaveError {
            written: Vec::new(),
            cause: SaveFailure::Conflict(vec![
                conflict(ConflictKind::Deleted, PALETTE, None),
                conflict(ConflictKind::Modified, OBP, Some(&[0; 128])),
            ]),
        })
    );
    assert_eq!(
        disk.snapshot(),
        before,
        "sprites.bin was not written either"
    );
    assert_eq!(modified(&doc), [TILES, OBP, PALETTE]);
}

#[test]
fn overwrite_replaces_exactly_the_version_shown() {
    let disk = Disk::new("doc-overwrite", Some(&[0x11; 32]));
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    disk.write(TILES, &[0x22; 64]);
    let Err(SaveError {
        cause: SaveFailure::Conflict(conflicts),
        ..
    }) = doc.save(&disk.cartridge)
    else {
        panic!("expected a conflict");
    };
    let seen: Vec<DiskAsset> = conflicts.into_iter().map(|c| c.disk).collect();

    // A further change after the dialog appeared is not overwritten.
    disk.write(TILES, &[0x33; 64]);
    let Err(SaveError {
        cause: SaveFailure::Conflict(again),
        ..
    }) = doc.overwrite(&disk.cartridge, &seen)
    else {
        panic!("expected a second conflict");
    };
    assert_eq!(
        again,
        [conflict(ConflictKind::Modified, TILES, Some(&[0x33; 64]))]
    );
    assert_eq!(disk.tiles(), Some(vec![0x33; 64]));
    assert!(doc.is_modified());

    let seen: Vec<DiskAsset> = again.into_iter().map(|c| c.disk).collect();
    assert_eq!(
        doc.overwrite(&disk.cartridge, &seen),
        written(&[(TILES, 32)])
    );
    assert_eq!(disk.tiles().unwrap()[0], 0x71);
    assert!(!doc.is_modified());
    assert!(doc.can_undo(), "overwriting keeps the history");
    assert_eq!(doc.save(&disk.cartridge), Ok(Saved::NothingToSave));
}

#[test]
fn overwrite_saves_the_other_modified_assets_against_their_baselines() {
    let disk = Disk::full("doc-overwrite-mixed");
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    doc.set_master_color(42, RED);
    disk.remove(TILES);
    let seen = [DiskAsset {
        asset: TILES,
        bytes: None,
    }];

    // palette.bin was not part of the conflict: an outside change to it is
    // a new conflict, and nothing is written.
    disk.write(PALETTE, &[0; PALETTE_RGB888_BYTES]);
    let Err(SaveError {
        written: saved,
        cause: SaveFailure::Conflict(conflicts),
    }) = doc.overwrite(&disk.cartridge, &seen)
    else {
        panic!("expected a conflict");
    };
    assert!(saved.is_empty());
    assert_eq!(
        conflicts,
        [conflict(
            ConflictKind::Modified,
            PALETTE,
            Some(&[0; PALETTE_RGB888_BYTES])
        )]
    );
    assert_eq!(disk.tiles(), None);

    disk.write(PALETTE, &blue_42());
    assert_eq!(
        doc.overwrite(&disk.cartridge, &seen),
        written(&[(PALETTE, PALETTE_RGB888_BYTES), (TILES, 32)])
    );
    assert_eq!(
        disk.tiles().unwrap()[0],
        0x71,
        "the deleted file is recreated"
    );
}

#[test]
fn reloading_discards_edits_and_history() {
    let disk = Disk::new("doc-reload", Some(&[0x11; 32]));
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    disk.write(TILES, &[0x22; 64]);
    doc.reload(&disk.cartridge, &[TILES], &defaults()).unwrap();
    assert_eq!(rows(&doc, 1)[0], "22222222");
    assert!(!doc.is_modified() && !doc.can_undo() && !doc.can_redo());
    assert_eq!(
        doc.origin().file_bytes(TILES).as_deref(),
        Some(&[0x22; 64][..])
    );

    doc.set_pixel(0, 0, 0, 7);
    disk.remove(TILES);
    doc.reload(&disk.cartridge, &[TILES], &defaults()).unwrap();
    assert_eq!(
        rows(&doc, 0)[0],
        "00000000",
        "a deleted file reads as empty tiles"
    );
    assert_eq!(doc.origin().file_bytes(TILES), None);
    assert_eq!(doc.obj().palettes(), disk.document().obj().palettes());
}

#[test]
fn reloading_one_asset_keeps_the_changes_to_the_others() {
    let disk = Disk::full("doc-reload-one");
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    doc.set_master_color(42, RED);
    doc.reload(&disk.cartridge, &[PALETTE], &defaults())
        .unwrap();
    assert_eq!(doc.video().palette.get(42), BLUE);
    assert_eq!(modified(&doc), [TILES]);
    assert_eq!(doc.pixel(0, 0, 0), Some(7));
}

#[test]
fn a_failed_reload_keeps_the_document() {
    let disk = Disk::full("doc-reload-invalid");
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    doc.set_master_color(42, RED);
    disk.write(TILES, &[0x22; 33]);
    let before = doc.clone();
    assert!(
        doc.reload(&disk.cartridge, &[PALETTE, TILES], &defaults())
            .is_err()
    );
    assert_eq!(doc, before, "not even the valid palette.bin was taken");
}

// History across reloads.

#[test]
fn reloading_an_asset_drops_its_undo_steps_and_every_older_one() {
    let disk = Disk::full("doc-reload-undo");
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 2);
    doc.begin_edit();
    doc.set_pixel(0, 1, 0, 3);
    doc.set_master_color(42, RED);
    doc.end_edit();
    doc.set_pixel(0, 2, 0, 4);

    doc.reload(&disk.cartridge, &[PALETTE], &defaults())
        .unwrap();
    assert_eq!(rows(&doc, 0)[0], "23411111", "the edits themselves stay");
    assert!(doc.undo());
    assert_eq!(rows(&doc, 0)[0], "23111111");
    assert!(
        !doc.undo(),
        "the grouped edit and the one before it are gone"
    );
    assert_eq!(modified(&doc), [TILES]);
}

#[test]
fn reopening_drops_redo_steps_that_involve_a_reloaded_asset() {
    let disk = Disk::full("doc-reopen-redo");
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 2);
    doc.begin_edit();
    doc.set_pixel(0, 1, 0, 3);
    doc.set_master_color(42, RED);
    doc.end_edit();
    doc.set_pixel(0, 2, 0, 4);
    doc.undo();
    doc.undo();
    assert_eq!(modified(&doc), [TILES]);

    let mut outside = Palette::from_rgb888(&blue_42()).unwrap();
    outside.set(7, RED);
    disk.write(PALETTE, &outside.to_rgb888());
    assert_eq!(
        doc.reopen(&disk.cartridge, &defaults()),
        Ok(Reopened {
            reloaded: vec![PALETTE],
            history_trimmed: true,
            conflicts: Vec::new(),
        })
    );
    assert_eq!(doc.video().palette, outside);
    assert!(
        !doc.can_redo(),
        "both undone edits would be redone after it"
    );
    assert!(doc.undo(), "the unrelated older edit stays");
    assert!(!doc.is_modified());
}

// Reopening the workspace.

#[test]
fn reopening_over_unchanged_files_keeps_edits_and_history() {
    let disk = Disk::new("doc-reopen-same", Some(&[0x11; 32]));
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    doc.undo();
    doc.set_pixel(0, 1, 0, 7);
    let obp: [u8; 128] = std::array::from_fn(|i| if i % 16 == 0 { 0 } else { 40 });
    disk.write(OBP, &obp);
    assert_eq!(
        doc.reopen(&disk.cartridge, &defaults()),
        Ok(Reopened {
            reloaded: vec![OBP],
            history_trimmed: false,
            conflicts: Vec::new(),
        })
    );
    assert!(doc.is_modified() && doc.can_undo());
    assert_eq!(rows(&doc, 0)[0], "17111111");
    assert_eq!(
        doc.obj().palettes(),
        disk.document().obj().palettes(),
        "a changed unmodified asset is read afresh"
    );
}

#[test]
fn reopening_never_drops_unsaved_edits() {
    let disk = Disk::full("doc-reopen-conflict");
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    doc.set_obj_color(bank(3), 2, 9);
    disk.write(TILES, &[0x22; 32]);
    disk.remove(OBP);
    let before = doc.clone();
    assert_eq!(
        doc.reopen(&disk.cartridge, &defaults()),
        Ok(Reopened {
            reloaded: Vec::new(),
            history_trimmed: false,
            conflicts: vec![
                conflict(ConflictKind::Modified, TILES, Some(&[0x22; 32])),
                conflict(ConflictKind::Deleted, OBP, None),
            ],
        })
    );
    assert_eq!(doc, before);
}

#[test]
fn reopening_reloads_unmodified_assets_beside_a_conflict() {
    let disk = Disk::full("doc-reopen-mixed");
    let mut doc = disk.document();
    doc.set_obj_color(bank(3), 2, 9);
    doc.set_pixel(0, 0, 0, 7);
    disk.write(OBP, &[0; 128]);
    disk.write(PALETTE, &[0; PALETTE_RGB888_BYTES]);
    assert_eq!(
        doc.reopen(&disk.cartridge, &defaults()),
        Ok(Reopened {
            reloaded: vec![PALETTE],
            history_trimmed: false,
            conflicts: vec![conflict(ConflictKind::Modified, OBP, Some(&[0; 128]))],
        })
    );
    assert_eq!(doc.video().palette.get(42), Rgb::from_hex(0));
    assert_eq!(doc.obj().palettes().resolve(bank(3), 2), Some(9));
    assert_eq!(modified(&doc), [TILES, OBP]);
    assert!(doc.undo() && doc.undo(), "their history is kept");
}

#[test]
fn reopening_a_clean_document_over_a_changed_file_reloads_it() {
    let disk = Disk::new("doc-reopen-clean", Some(&[0x11; 32]));
    let mut doc = disk.document();
    disk.write(TILES, &[0x22; 32]);
    assert_eq!(
        doc.reopen(&disk.cartridge, &defaults()),
        Ok(Reopened {
            reloaded: vec![TILES],
            history_trimmed: false,
            conflicts: Vec::new(),
        })
    );
    assert_eq!(rows(&doc, 0)[0], "22222222");

    // Saved edits leave history that no longer fits a changed file.
    doc.set_pixel(0, 0, 0, 7);
    doc.save(&disk.cartridge).unwrap();
    disk.write(TILES, &[0x33; 32]);
    assert_eq!(
        doc.reopen(&disk.cartridge, &defaults()),
        Ok(Reopened {
            reloaded: vec![TILES],
            history_trimmed: true,
            conflicts: Vec::new(),
        })
    );
    assert!(!doc.can_undo());
    assert_eq!(rows(&doc, 0)[0], "33333333");
}

#[test]
fn reopening_over_unreadable_or_invalid_files_keeps_everything() {
    let disk = Disk::full("doc-reopen-fail");
    let mut doc = disk.document();
    doc.set_pixel(0, 0, 0, 7);
    doc.save(&disk.cartridge).unwrap();
    let before = doc.clone();

    // A valid change to one file is not taken while another is invalid.
    disk.write(PALETTE, &[0; PALETTE_RGB888_BYTES]);
    disk.write(TILES, &[0x22; 33]);
    assert!(doc.reopen(&disk.cartridge, &defaults()).is_err());
    assert_eq!(doc, before);

    disk.remove(TILES);
    fs::create_dir(disk.path(TILES)).unwrap();
    assert!(doc.reopen(&disk.cartridge, &defaults()).is_err());
    assert_eq!(doc, before);
}
