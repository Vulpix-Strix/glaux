//! Importing a PNG spritesheet from the sprite editor.

use super::sprite_editor::{document, open_sprites, with_video_assets};
use super::*;
use crate::sprite_import::fixture::rgba_png;
use crate::system::video::{CartridgeVideo, VideoFiles};
use pretty_assertions::assert_eq;
use std::path::{Path, PathBuf};

const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const RED: [u8; 4] = [200, 10, 10, 255];
const BLUE: [u8; 4] = [10, 10, 200, 255];

/// A 16x8 sheet on an opaque magenta background: tile 0 solid red, tile 1
/// a blue square in its middle.
fn magenta_sheet(dir: &Path) -> PathBuf {
    let mut rgba = Vec::new();
    for y in 0..8 {
        for x in 0..16 {
            rgba.extend(match (x, y) {
                (0..8, _) => RED,
                (11..13, 3..5) => BLUE,
                _ => MAGENTA,
            });
        }
    }
    let path = dir.join("sheet.png");
    rgba_png(&path, 16, 8, &rgba);
    path
}

/// An 8x8 sheet whose one tile uses 16 opaque colors.
fn too_colorful_sheet(dir: &Path) -> PathBuf {
    let rgba: Vec<u8> = (0..64u8).flat_map(|n| [(n % 16) * 15, 7, 7, 255]).collect();
    let path = dir.join("colorful.png");
    rgba_png(&path, 8, 8, &rgba);
    path
}

fn centre(rect: workspace::Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

/// Clicks Import and takes the host request it makes.
fn press_import(system: System) -> (System, Option<HostRequest>) {
    let button = workspace::tests_support::sprite_import_button(&system.workspace);
    let mut system = system.update(&click(centre(button)));
    let request = system.take_host_request();
    (system, request)
}

/// The sprite editor on `pong` with its video assets, waiting for the
/// file dialog.
fn waiting(fixture: &Fixture) -> System {
    let (system, request) = press_import(open_sprites(with_video_assets(fixture)));
    assert!(request.is_some());
    system
}

fn chosen(mut system: System, path: PathBuf) -> System {
    system.file_picked(PickedFile::Chosen(path));
    system
}

fn preview(system: &System) -> &workspace::ImportPreview {
    workspace::tests_support::import_preview(&system.workspace).expect("IMPORT SPRITES open")
}

fn disk(fixture: &Fixture) -> Vec<Option<Vec<u8>>> {
    ["sprites.bin", "objattr.bin", "obp.bin", "palette.bin"]
        .iter()
        .map(|name| std::fs::read(fixture.root().join("pong").join(name)).ok())
        .collect()
}

#[test]
fn import_asks_the_host_for_a_png_in_the_cartridge_folder() {
    let fixture = Fixture::new("sys-import-request");
    let (system, request) = press_import(open_sprites(with_video_assets(&fixture)));
    let directory = std::fs::canonicalize(fixture.root().join("pong")).unwrap();
    assert_eq!(request, Some(HostRequest::PickPng { directory }));
    assert!(workspace::tests_support::modal(&system.workspace).is_none());
}

#[test]
fn while_waiting_nothing_reaches_the_editor_and_escape_stops_the_wait() {
    let fixture = Fixture::new("sys-import-waiting");
    let system = waiting(&fixture);
    let before = document(&system).clone();
    let pixel = workspace::tests_support::sprite_pixel(&system.workspace, 2, 2);
    let system = system.update(&click(pixel));
    assert_eq!(document(&system), &before);

    let system = press_break(system);
    assert!(is_workspace(&system), "Escape stops waiting, nothing else");
    let mut system = chosen(system, magenta_sheet(&fixture.root()));
    assert!(workspace::tests_support::import_preview(&system.workspace).is_none());
    assert_eq!(system.take_host_request(), None);
    assert!(!is_workspace(&press_break(system)));
}

#[test]
fn a_cancelled_dialog_leaves_everything_as_it_was() {
    let fixture = Fixture::new("sys-import-cancel-dialog");
    let mut system = waiting(&fixture);
    let before = document(&system).clone();
    system.file_picked(PickedFile::Cancelled);
    assert_eq!(document(&system), &before);
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("No file chosen")
    );
    assert!(!system.workspace.has_modal());
}

#[test]
fn a_file_that_is_not_a_png_is_explained_and_changes_nothing() {
    let fixture = Fixture::new("sys-import-not-png");
    let path = fixture.root().join("notes.png");
    std::fs::write(&path, "not an image").unwrap();
    let system = waiting(&fixture);
    let before = document(&system).clone();
    let system = chosen(system, path);
    let (title, labels) = workspace::tests_support::modal(&system.workspace).unwrap();
    assert_eq!((title, labels), ("CANNOT IMPORT", vec!["OK"]));
    assert_eq!(document(&system), &before);
}

#[test]
fn a_sheet_g1_cannot_show_is_explained_and_cannot_be_imported() {
    let fixture = Fixture::new("sys-import-fit-error");
    let system = waiting(&fixture);
    let before = document(&system).clone();
    let system = chosen(system, too_colorful_sheet(&fixture.root()));
    let error = preview(&system).error().unwrap().to_owned();
    assert!(error.contains("uses 16 colors"), "{error}");

    let (_, _, _, import) = workspace::tests_support::import_preview_parts(&system.workspace);
    let system = system
        .update(&[InputEvent::Enter])
        .update(&click(centre(import)));
    assert!(workspace::tests_support::import_preview(&system.workspace).is_some());
    let system = press_break(system);
    assert!(workspace::tests_support::import_preview(&system.workspace).is_none());
    assert!(is_workspace(&system));
    assert_eq!(document(&system), &before);
}

#[test]
fn cancel_closes_the_preview_without_touching_the_document() {
    let fixture = Fixture::new("sys-import-cancel");
    let system = chosen(waiting(&fixture), magenta_sheet(&fixture.root()));
    let before = document(&system).clone();
    let (_, _, cancel, _) = workspace::tests_support::import_preview_parts(&system.workspace);
    let system = system.update(&click(centre(cancel)));
    assert!(workspace::tests_support::import_preview(&system.workspace).is_none());
    assert_eq!(document(&system), &before);
}

#[test]
fn the_top_left_color_can_be_made_transparent() {
    let fixture = Fixture::new("sys-import-key");
    let system = chosen(waiting(&fixture), magenta_sheet(&fixture.root()));
    assert_eq!(preview(&system).summary().unwrap().colors, 3);
    let (_, transparency, _, _) = workspace::tests_support::import_preview_parts(&system.workspace);
    let system = system.update(&click(centre(transparency)));
    assert!(preview(&system).is_keyed());
    assert_eq!(preview(&system).summary().unwrap().colors, 2);
    let system = system.update(&[InputEvent::Text('t')]);
    assert!(!preview(&system).is_keyed());
}

#[test]
fn the_preview_shows_the_converted_colors() {
    let fixture = Fixture::new("sys-import-colors");
    let rom = fake_rom();
    let system = chosen(waiting(&fixture), magenta_sheet(&fixture.root()));
    let (sheet, _, _, _) = workspace::tests_support::import_preview_parts(&system.workspace);
    let fb = render(&system);
    let shown = |x: i32, y: i32| system.frame_palette(&rom).get(fb.pixel(x, y).unwrap());
    let [r, g, b, _] = RED;
    assert_eq!(
        shown(sheet.x + 1, sheet.y + 1),
        glaux_machine::palette::Rgb { r, g, b }
    );
    // The 16x8 sheet is shown at 2x: tile 1's blue square starts at its
    // pixel (3, 3).
    let [r, g, b, _] = BLUE;
    assert_eq!(
        shown(sheet.x + 16 + 7, sheet.y + 7),
        glaux_machine::palette::Rgb { r, g, b }
    );
}

#[test]
fn importing_is_one_unsaved_edit_that_undoes_and_redoes() {
    let fixture = Fixture::new("sys-import-undo");
    let system = chosen(waiting(&fixture), magenta_sheet(&fixture.root()));
    let before = document(&system).video().clone();
    let files = disk(&fixture);
    let system = system.update(&[InputEvent::Enter]);
    assert!(workspace::tests_support::import_preview(&system.workspace).is_none());
    assert!(document(&system).is_modified());
    assert_ne!(document(&system).video(), &before);
    assert_eq!(disk(&fixture), files, "nothing is written before Save");
    let imported = document(&system).video().clone();

    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(document(&system).video(), &before);
    assert!(!document(&system).is_modified());
    let system = system.update(&[InputEvent::Redo, InputEvent::Save]);
    assert_eq!(document(&system).video(), &imported);
    assert!(!document(&system).is_modified());
    assert_ne!(disk(&fixture), files);
}

/// Importing an example's PNG in the editor and saving writes the assets
/// `glaux import sprites` committed for it, as the cartridge reads them.
#[test]
fn saving_an_import_gives_the_assets_the_cli_import_gives() {
    let fixture = Fixture::new("sys-import-like-cli");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    let defaults = CartridgeVideoDefaults::from_rom(&crate::rom::load().unwrap());
    let mut system = System::new(
        info(),
        sounds(),
        defaults.clone(),
        fixture.storage(),
        fake_rom().font(),
        fake_rom().palette().clone(),
    );
    while matches!(system.state(), SystemState::Boot { .. }) {
        system = system.update(&[]);
    }
    let system = open_sprites(enter(system, "load pong"));
    let (system, _) = press_import(system);
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/palettes");
    let system = chosen(system, example.join("sprites.png"));
    assert!(preview(&system).can_import());
    system.update(&[InputEvent::Enter, InputEvent::Save]);

    let decoded = |dir: &Path| {
        let read = |name: &str| std::fs::read(dir.join(name)).ok();
        let files = VideoFiles {
            obj_tiles: read("sprites.bin"),
            obj_attributes: read("objattr.bin"),
            obj_palettes: read("obp.bin"),
            master_palette: read("palette.bin"),
        };
        CartridgeVideo::decode(&files, &defaults).unwrap()
    };
    assert_eq!(decoded(&dir), decoded(&example));
}
