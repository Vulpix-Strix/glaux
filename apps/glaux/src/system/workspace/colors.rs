//! How the workspace shows the cartridge's colors.

use super::sprites::SpriteSource;
use super::{SYSTEM_COLORS, Tab, Workspace};
use crate::system::sprite_document::{SpriteDocument, TileSet};
use crate::ui_text;
use glaux_machine::bg::BgPalettes;
use glaux_machine::obj_palette::ObjPalettes;
use glaux_machine::palette::Palette;
use glaux_video::preview::ObjPreviewPalette;

/// How the workspace shows the cartridge's colors: where the frame palette
/// holds each master color the palettes of the tiles on show use, or why
/// the cartridge's video state can't be shown.
///
/// Only one kind of tile is shown at a time, so the frame palette only
/// holds the colors of that kind's palette banks. Object and background
/// banks together could need more entries than fit before the colors of the
/// sprite editor's dialogs.
#[derive(Debug)]
// Boxed: a frame palette would make every system state as large as this
// one.
pub struct WorkspaceVideo(Result<Box<PreparedVideo>, String>);

/// A frame palette with the palettes it was built from, to tell when it no
/// longer matches the document.
#[derive(Debug)]
pub(super) struct PreparedVideo {
    preview: ObjPreviewPalette,
    /// The preview's palette with the sprite editor's dialog colors, if
    /// any, from [`super::sprites::SCRATCH_BASE`] on: what frames are presented
    /// with.
    frame: Palette,
    master: Palette,
    /// The palette banks the preview was built from, which are the object
    /// ones or the background ones according to `set`.
    banks: Banks,
    set: TileSet,
}

#[derive(Debug, PartialEq, Eq)]
enum Banks {
    Obj(ObjPalettes),
    Bg(BgPalettes),
}

impl Banks {
    fn of(document: &SpriteDocument, set: TileSet) -> Self {
        match set {
            TileSet::Obj => Self::Obj(document.obj().palettes().clone()),
            TileSet::Bg => Self::Bg(document.bg().palettes().clone()),
        }
    }
}

impl WorkspaceVideo {
    /// The video state could not be read, for the reason in `message`.
    pub fn unavailable(message: String) -> Self {
        tracing::warn!(%message, "cartridge video unavailable in the workspace");
        Self(Err(message))
    }

    /// Whether the editors can show the cartridge's video state.
    pub fn is_ready(&self) -> bool {
        self.0.is_ok()
    }

    /// The palette the workspace's frames are presented with, if the
    /// cartridge's video assets loaded.
    pub fn frame_palette(&self) -> Option<&Palette> {
        self.0.as_ref().ok().map(|video| &video.frame)
    }

    /// What the sprite editor shows of `document`, or why it shows nothing.
    pub fn source<'a>(
        &'a self,
        document: Option<&'a SpriteDocument>,
    ) -> Result<SpriteSource<'a>, String> {
        let video = self.0.as_deref().map_err(Clone::clone)?;
        // The system only prepares a preview from a document it keeps.
        let document = document
            .ok_or_else(|| ui_text::get("workspace-the-cartridge-s-sprites-are-not-loaded"))?;
        Ok(SpriteSource {
            obj: document.obj(),
            bg: document.bg(),
            master: &document.video().palette,
            preview: &video.preview,
            modified: document.is_modified(),
        })
    }

    /// Whether the frame palette was built from `document`'s palettes of
    /// the tiles `set` shows, as they are now.
    fn matches(&self, document: &SpriteDocument, set: TileSet) -> bool {
        self.0.as_ref().is_ok_and(|video| {
            video.set == set
                && video.master == document.video().palette
                && video.banks == Banks::of(document, set)
        })
    }
}

impl Workspace {
    /// Prepares to show `document` in the workspace's frame palette, for
    /// the tiles the sprite editor shows now. It holds until the document's
    /// palettes change or the editor switches tiles. See
    /// [`refresh_video`](Self::refresh_video).
    pub fn prepare_video(&self, document: &SpriteDocument) -> WorkspaceVideo {
        self.prepare_video_for(document, self.shown_tiles())
    }

    /// The tiles the open view draws: the background ones in the Map view
    /// and the sprite editor's own choice anywhere else.
    fn shown_tiles(&self) -> TileSet {
        if self.tab == Tab::Map {
            TileSet::Bg
        } else {
            self.sprites.tileset()
        }
    }

    fn prepare_video_for(&self, document: &SpriteDocument, set: TileSet) -> WorkspaceVideo {
        let video = document.video();
        let preview = match set {
            TileSet::Obj => ObjPreviewPalette::new(
                &self.system_palette,
                SYSTEM_COLORS,
                &video.palette,
                video.obj.palettes(),
            ),
            TileSet::Bg => ObjPreviewPalette::for_background(
                &self.system_palette,
                SYSTEM_COLORS,
                &video.palette,
                video.bg.palettes(),
            ),
        };
        match preview {
            Ok(preview) => WorkspaceVideo(Ok(Box::new(PreparedVideo {
                frame: preview.palette().clone(),
                preview,
                master: video.palette.clone(),
                banks: Banks::of(document, set),
                set,
            }))),
            Err(err) => {
                WorkspaceVideo::unavailable(super::super::ui_errors::video::preview_palette(&err))
            }
        }
    }

    /// Prepares `video` again if `document`'s palettes, or the tiles the
    /// sprite editor shows, changed since it was prepared, so the frame
    /// palette never shows stale colors or misses the ones on show, and gives
    /// the frame palette the colors of the sprite editor's open dialog.
    /// Video that could not be prepared stays as it is.
    pub fn refresh_video(&self, video: &mut WorkspaceVideo, document: &SpriteDocument) {
        if video.is_ready() && !video.matches(document, self.shown_tiles()) {
            *video = self.prepare_video(document);
        }
        if let Ok(prepared) = &mut video.0 {
            if let Some(palette) = self.sprites.import_frame_palette() {
                prepared.frame = palette.clone();
                return;
            }
            let mut frame = prepared.preview.palette().clone();
            if let Some(colors) = self.sprites.scratch_colors() {
                for (offset, color) in colors.into_iter().enumerate() {
                    // Scratch entries end at the palette's end, so fit u8.
                    frame.set((super::sprites::SCRATCH_BASE + offset) as u8, color);
                }
            }
            prepared.frame = frame;
        }
    }
}

#[cfg(test)]
mod tests;
