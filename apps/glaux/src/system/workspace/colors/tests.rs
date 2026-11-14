use super::*;
use crate::system::sprite_document::TileSet;
use crate::system::video::{CartridgeVideoDefaults, VideoFiles};
use glaux_machine::bg::BG_PALETTES_BYTES;
use glaux_machine::font::{BitmapFont, FontMetrics};
use glaux_machine::obj_palette::OBJ_PALETTES_BYTES;

/// A document whose eight object banks and four background banks map 120 and
/// 60 distinct masters, none shared, so a preview showing both kinds at once
/// needs 180 entries of the frame palette.
fn document() -> SpriteDocument {
    let mut obp = [0; OBJ_PALETTES_BYTES];
    let mut bgp = [0; BG_PALETTES_BYTES];
    let mut next = 40;
    for bytes in [&mut obp[..], &mut bgp[..]] {
        for (offset, byte) in bytes.iter_mut().enumerate() {
            if offset % 16 != 0 {
                *byte = next;
                next += 1;
            }
        }
    }
    let files = VideoFiles {
        obj_palettes: Some(obp.to_vec()),
        bg_palettes: Some(bgp.to_vec()),
        ..VideoFiles::default()
    };
    let defaults = CartridgeVideoDefaults::from_rom(&crate::rom::load().unwrap());
    SpriteDocument::from_files(&files, &defaults).unwrap()
}

fn workspace() -> Workspace {
    let font = BitmapFont::new(FontMetrics::SYSTEM);
    Workspace::new(&font, Palette::default())
}

/// The highest frame entry the prepared preview gives a master color.
fn highest_slot(video: &WorkspaceVideo) -> usize {
    let prepared = video.0.as_ref().unwrap();
    prepared
        .preview
        .slots()
        .iter()
        .map(|&slot| usize::from(slot))
        .max()
        .unwrap()
}

/// The frame entry of every master the document's palettes of `set` map.
fn shown_masters(document: &SpriteDocument, set: TileSet) -> Vec<u8> {
    match set {
        TileSet::Obj => (0..8)
            .filter_map(glaux_machine::obj_palette::ObjBank::new)
            .flat_map(|bank| {
                (1..16).filter_map(move |local| document.obj().palettes().resolve(bank, local))
            })
            .collect(),
        TileSet::Bg => (0..4)
            .filter_map(glaux_machine::tilemap::BgBank::new)
            .flat_map(|bank| {
                (1..16).filter_map(move |local| document.bg().palettes().resolve(bank, local))
            })
            .collect(),
    }
}

#[test]
fn the_preview_never_reaches_the_entries_the_color_editor_draws_with() {
    let document = document();
    for set in [TileSet::Obj, TileSet::Bg] {
        let workspace = workspace();
        let video = workspace.prepare_video_for(&document, set);
        assert!(
            highest_slot(&video) < super::super::sprites::SCRATCH_BASE,
            "{set:?}: a preview entry would be overwritten by the dialog's colors"
        );
    }
}

#[test]
fn each_preview_shows_every_color_of_the_tiles_on_show() {
    let document = document();
    let workspace = workspace();
    for set in [TileSet::Obj, TileSet::Bg] {
        let video = workspace.prepare_video_for(&document, set);
        let prepared = video.0.as_ref().unwrap();
        let masters = shown_masters(&document, set);
        assert_eq!(masters.len(), if set == TileSet::Obj { 120 } else { 60 });
        for master in masters {
            let slot = prepared.preview.slot(master).expect("a slot per master");
            assert_eq!(
                prepared.frame.get(slot),
                document.video().palette.get(master),
                "{set:?} master {master}"
            );
        }
    }
}

#[test]
fn switching_tiles_prepares_the_preview_again_and_only_then() {
    let document = document();
    let mut workspace = workspace();
    let mut video = workspace.prepare_video(&document);
    let object_slot =
        |video: &WorkspaceVideo, master| video.0.as_ref().unwrap().preview.slot(master);
    // Master 40 is the first object color and 160 the first background one.
    assert!(object_slot(&video, 40).is_some() && object_slot(&video, 160).is_none());

    workspace.refresh_video(&mut video, &document);
    assert!(video.matches(&document, TileSet::Obj));

    workspace
        .sprites
        .handle_input(glaux_input::InputEvent::Text('b'), None);
    assert!(!video.matches(&document, workspace.sprites.tileset()));
    workspace.refresh_video(&mut video, &document);
    assert!(object_slot(&video, 40).is_none() && object_slot(&video, 160).is_some());
}

#[test]
fn an_open_edit_color_dialog_leaves_every_preview_color_alone() {
    use glaux_input::InputEvent;

    let mut document = document();
    let mut workspace = workspace();
    for set in [TileSet::Obj, TileSet::Bg] {
        if workspace.sprites.tileset() != set {
            workspace.sprites.handle_input(InputEvent::Text('b'), None);
        }
        let mut video = workspace.prepare_video(&document);
        workspace
            .sprites
            .handle_input(InputEvent::Text('c'), Some(&mut document));
        assert!(workspace.sprites.has_dialog(), "{set:?}: EDIT COLOR opens");
        workspace.refresh_video(&mut video, &document);

        let frame = video.frame_palette().unwrap();
        let prepared = video.0.as_ref().unwrap();
        for master in shown_masters(&document, set) {
            let slot = prepared.preview.slot(master).unwrap();
            assert_eq!(
                frame.get(slot),
                document.video().palette.get(master),
                "{set:?}: master {master} on entry {slot}"
            );
        }
        // The dialog's colors really are in the frame, from SCRATCH_BASE on.
        let scratch = workspace.sprites.scratch_colors().unwrap();
        assert_eq!(
            frame.get(super::super::sprites::SCRATCH_BASE as u8),
            scratch[0]
        );
        workspace
            .sprites
            .handle_input(InputEvent::Break, Some(&mut document));
        assert!(!workspace.sprites.has_dialog());
    }
}
