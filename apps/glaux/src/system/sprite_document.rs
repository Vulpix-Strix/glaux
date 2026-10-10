//! The loaded cartridge's video state as the editors change it, with the
//! edit history and the file state it is saved against.

use std::collections::VecDeque;

use glaux_machine::sprite::{
    OBJ_TILE_BYTES, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH, ObjTileBank, ObjVram,
};

use super::video::{
    CartridgeVideo, CartridgeVideoDefaults, VideoAssetError, VideoFiles, VideoLoadError,
};
use crate::cartridge::{CartridgeError, LoadedCartridge, ObjTilesWriteError};

/// Edits the history keeps. Older ones are forgotten first. Editor strokes
/// and fills stay inside one tile, so one edit holds at most 64 pixel
/// changes and a full history stays around 100 KiB.
pub const HISTORY_LIMIT: usize = 256;

/// What `sprites.bin` held when the document was read or last saved: the
/// state saving compares the disk against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileOrigin {
    bank: ObjTileBank,
    /// Length of `sprites.bin`, or `None` when the cartridge had none.
    file_len: Option<usize>,
}

impl TileOrigin {
    /// The exact bytes `sprites.bin` held, or `None` when it was absent.
    pub fn file_bytes(&self) -> Option<&[u8]> {
        self.file_len.map(|len| &self.bank.as_bytes()[..len])
    }
}

/// `sprites.bin` as found on disk at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskTiles(pub Option<Vec<u8>>);

/// How `sprites.bin` on disk differs from what the document was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// The file was changed.
    Modified,
    /// The file the document was read from is gone.
    Deleted,
    /// A file appeared where the cartridge had none.
    Created,
}

/// `sprites.bin` changed outside Glaux since the document's baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub kind: ConflictKind,
    /// What the file holds now. Overwriting is only allowed over exactly
    /// this.
    pub disk: DiskTiles,
}

/// A successful [`SpriteDocument::save`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saved {
    /// `sprites.bin` now holds this many bytes of the document's tiles.
    Written { bytes: usize },
    /// The document already matched its baseline; nothing was written.
    NothingToSave,
}

/// Why the tiles were not saved. The document is unchanged either way.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SaveError {
    #[error("sprites.bin changed outside Glaux")]
    Conflict(Conflict),
    /// Reading or writing the file failed; nothing on disk changed.
    #[error(transparent)]
    Cartridge(CartridgeError),
}

/// What [`SpriteDocument::reopen`] found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reopened {
    /// `sprites.bin` matches the baseline. The document and its history
    /// are kept; the read-only assets were read afresh.
    Unchanged,
    /// The document held no unsaved edits and was reloaded from a changed
    /// `sprites.bin`. `history_cleared` tells whether undo history was
    /// dropped with the old tiles.
    Reloaded { history_cleared: bool },
    /// `sprites.bin` changed under unsaved edits. Nothing was touched.
    Conflict(Conflict),
    /// The files could not be read or are invalid, for the reason given.
    /// The document and its history are kept as they were.
    Failed(String),
}

/// One pixel an edit changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelChange {
    tile: u16,
    x: u8,
    y: u8,
    before: u8,
    after: u8,
}

/// The pixels one user action changed, in the order first changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Edit(Vec<PixelChange>);

impl Edit {
    /// Adds a write to the edit. A pixel written again keeps its first
    /// `before` and takes the new `after`.
    fn record(&mut self, change: PixelChange) {
        let same = |c: &&mut PixelChange| (c.tile, c.x, c.y) == (change.tile, change.x, change.y);
        match self.0.iter_mut().find(same) {
            Some(existing) => existing.after = change.after,
            None => self.0.push(change),
        }
    }
}

/// Undo and redo stacks, plus the edit still being recorded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct History {
    undo: VecDeque<Edit>,
    redo: Vec<Edit>,
    open: Option<Edit>,
}

impl History {
    /// Files the open edit, if it changed anything, as the newest undo
    /// step. A new step makes the redo steps unreachable, so they go.
    fn close(&mut self) {
        let Some(Edit(mut changes)) = self.open.take() else {
            return;
        };
        changes.retain(|c| c.before != c.after);
        if changes.is_empty() {
            return;
        }
        self.redo.clear();
        self.push_undo(Edit(changes));
    }

    fn push_undo(&mut self, edit: Edit) {
        self.undo.push_back(edit);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
    }
}

/// A cartridge's video state as the editors change it, held in memory.
///
/// Object tiles are editable; their attributes, the object palette banks and
/// the master palette are read-only. The document remembers the tiles it was
/// read with or last saved, so it knows exactly whether it differs from
/// them: restoring every changed pixel, by hand or by undo, leaves it
/// unmodified again.
///
/// Changes are grouped into edits between [`begin_edit`](Self::begin_edit)
/// and [`end_edit`](Self::end_edit), each undone and redone whole. Saving
/// keeps the history; reloading from disk clears it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteDocument {
    video: CartridgeVideo,
    origin: TileOrigin,
    /// Pixels whose local index differs from `origin`'s.
    changed_pixels: usize,
    history: History,
}

impl SpriteDocument {
    /// Reads the cartridge's video assets, falling back to `defaults` for
    /// those it lacks.
    pub fn load(
        cartridge: &LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
    ) -> Result<Self, VideoLoadError> {
        let files = VideoFiles::read(cartridge).map_err(VideoLoadError::Cartridge)?;
        Ok(Self::from_files(&files, defaults)?)
    }

    /// A document on `files` as read from a cartridge.
    pub fn from_files(
        files: &VideoFiles,
        defaults: &CartridgeVideoDefaults,
    ) -> Result<Self, VideoAssetError> {
        let video = CartridgeVideo::decode(files, defaults)?;
        let origin = TileOrigin {
            bank: video.obj.tiles().clone(),
            file_len: files.obj_tiles.as_ref().map(Vec::len),
        };
        Ok(Self {
            video,
            origin,
            changed_pixels: 0,
            history: History::default(),
        })
    }

    pub fn video(&self) -> &CartridgeVideo {
        &self.video
    }

    pub fn obj(&self) -> &ObjVram {
        &self.video.obj
    }

    pub fn tiles(&self) -> &ObjTileBank {
        self.video.obj.tiles()
    }

    /// What the tiles are saved against, to tell whether the file changed
    /// on disk since.
    #[cfg(test)]
    pub fn origin(&self) -> &TileOrigin {
        &self.origin
    }

    /// Whether any pixel differs from the tiles last read or saved.
    pub fn is_modified(&self) -> bool {
        self.changed_pixels > 0
    }

    /// Whether there is an edit to undo.
    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
            || self
                .history
                .open
                .as_ref()
                .is_some_and(|edit| edit.0.iter().any(|c| c.before != c.after))
    }

    /// Whether there is an undone edit to redo.
    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    /// One tile-local palette index; `None` for an invalid tile or pixel.
    pub fn pixel(&self, tile: usize, x: usize, y: usize) -> Option<u8> {
        self.tiles().pixel(tile, x, y)
    }

    /// Starts recording one undoable edit, closing any still open. Every
    /// change until [`end_edit`](Self::end_edit) is undone as one.
    pub fn begin_edit(&mut self) {
        self.history.close();
        self.history.open = Some(Edit::default());
    }

    /// Closes the edit being recorded. An edit that changed nothing, or
    /// whose changes cancel out, leaves no history.
    pub fn end_edit(&mut self) {
        self.history.close();
    }

    /// Writes local index `index` to one pixel. Returns whether the pixel
    /// changed; invalid positions and indices above 15 change nothing.
    ///
    /// Outside an open edit, the write is an edit of its own.
    pub fn set_pixel(&mut self, tile: usize, x: usize, y: usize, index: u8) -> bool {
        let Some(previous) = self.write(tile, x, y, index) else {
            return false;
        };
        // `write` only succeeds for a valid tile and pixel, which fit.
        let change = PixelChange {
            tile: tile as u16,
            x: x as u8,
            y: y as u8,
            before: previous,
            after: index,
        };
        match &mut self.history.open {
            Some(edit) => edit.record(change),
            None => {
                self.history.redo.clear();
                self.history.push_undo(Edit(vec![change]));
            }
        }
        true
    }

    /// Replaces the pixels of `tile` connected to `(x, y)` through edges,
    /// that share its local index, with `index`. The fill never leaves the
    /// tile. Returns whether any pixel changed.
    ///
    /// Outside an open edit, the fill is an edit of its own.
    pub fn fill(&mut self, tile: usize, x: usize, y: usize, index: u8) -> bool {
        let Some(target) = self.pixel(tile, x, y) else {
            return false;
        };
        if target == index || index > 0x0f {
            return false;
        }
        let standalone = self.history.open.is_none();
        if standalone {
            self.begin_edit();
        }
        let mut pending = vec![(x, y)];
        while let Some((x, y)) = pending.pop() {
            if self.pixel(tile, x, y) != Some(target) {
                continue;
            }
            self.set_pixel(tile, x, y, index);
            if x > 0 {
                pending.push((x - 1, y));
            }
            if x + 1 < OBJ_TILE_WIDTH {
                pending.push((x + 1, y));
            }
            if y > 0 {
                pending.push((x, y - 1));
            }
            if y + 1 < OBJ_TILE_HEIGHT {
                pending.push((x, y + 1));
            }
        }
        if standalone {
            self.end_edit();
        }
        true
    }

    /// Reverses the newest edit, closing any still open first. Returns
    /// whether there was one.
    pub fn undo(&mut self) -> bool {
        self.history.close();
        let Some(edit) = self.history.undo.pop_back() else {
            return false;
        };
        for change in edit.0.iter().rev() {
            self.write(
                change.tile.into(),
                change.x.into(),
                change.y.into(),
                change.before,
            );
        }
        self.history.redo.push(edit);
        true
    }

    /// Reapplies the newest undone edit. Returns whether there was one.
    pub fn redo(&mut self) -> bool {
        self.history.close();
        let Some(edit) = self.history.redo.pop() else {
            return false;
        };
        for change in &edit.0 {
            self.write(
                change.tile.into(),
                change.x.into(),
                change.y.into(),
                change.after,
            );
        }
        self.history.push_undo(edit);
        true
    }

    /// The bytes saving writes: the whole original file length, grown in
    /// whole tiles to cover every non-empty tile past it.
    pub fn save_bytes(&self) -> &[u8] {
        let bytes = self.tiles().as_bytes();
        let used = bytes
            .as_chunks::<OBJ_TILE_BYTES>()
            .0
            .iter()
            .rposition(|tile| tile.iter().any(|&b| b != 0))
            .map_or(0, |last| (last + 1) * OBJ_TILE_BYTES);
        &bytes[..used.max(self.origin.file_len.unwrap_or(0))]
    }

    /// `sprites.bin` as it is on disk now.
    pub fn disk_tiles(cartridge: &LoadedCartridge) -> Result<DiskTiles, CartridgeError> {
        cartridge.read_obj_tiles().map(DiskTiles)
    }

    /// Writes the tiles to `sprites.bin`, provided it still holds exactly
    /// what the document was read from or last saved to. A file that was
    /// absent is created only now that there are changes to save.
    ///
    /// On success the saved tiles become the baseline and the document is
    /// unmodified; the history is kept. On any error nothing changes, in
    /// the document or on disk.
    pub fn save(&mut self, cartridge: &LoadedCartridge) -> Result<Saved, SaveError> {
        self.history.close();
        if !self.is_modified() {
            return Ok(Saved::NothingToSave);
        }
        let disk = Self::disk_tiles(cartridge).map_err(SaveError::Cartridge)?;
        if let Some(conflict) = self.conflict(disk) {
            return Err(SaveError::Conflict(conflict));
        }
        let expected = self.origin.file_bytes().map(<[u8]>::to_vec);
        self.write_file(cartridge, expected.as_deref())
    }

    /// Writes the tiles over `sprites.bin` although it changed outside
    /// Glaux, provided it still holds exactly `seen`. A further change
    /// is another [`SaveError::Conflict`].
    pub fn overwrite(
        &mut self,
        cartridge: &LoadedCartridge,
        seen: &DiskTiles,
    ) -> Result<Saved, SaveError> {
        self.history.close();
        self.write_file(cartridge, seen.0.as_deref())
    }

    /// Discards the tiles and their history and reads `sprites.bin` again,
    /// or starts from empty tiles when it is gone. The other assets stay as
    /// they are. On error the document is unchanged.
    pub fn reload_tiles(&mut self, cartridge: &LoadedCartridge) -> Result<(), VideoLoadError> {
        let disk = Self::disk_tiles(cartridge).map_err(VideoLoadError::Cartridge)?;
        self.replace_tiles(disk)
    }

    /// Brings the document up to date with the cartridge's files when the
    /// workspace opens again, never discarding unsaved edits:
    ///
    /// - `sprites.bin` unchanged: the document and history are kept, and
    ///   the read-only assets are read afresh.
    /// - changed under unsaved edits: nothing is touched; the conflict is
    ///   returned for the user to settle.
    /// - changed with no unsaved edits: the tiles are reloaded and the
    ///   history, which no longer applies to them, is cleared.
    pub fn reopen(
        &mut self,
        cartridge: &LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
    ) -> Reopened {
        self.history.close();
        let files = match VideoFiles::read(cartridge) {
            Ok(files) => files,
            Err(err) => return Reopened::Failed(err.to_string()),
        };
        let disk = DiskTiles(files.obj_tiles.clone());
        match self.conflict(disk.clone()) {
            None => match self.refresh_read_only(&files, defaults) {
                Ok(()) => Reopened::Unchanged,
                Err(err) => Reopened::Failed(err.to_string()),
            },
            Some(conflict) if self.is_modified() => Reopened::Conflict(conflict),
            Some(_) => {
                let history_cleared = self.can_undo() || self.can_redo();
                if let Err(err) = self.replace_tiles(disk) {
                    return Reopened::Failed(err.to_string());
                }
                match self.refresh_read_only(&files, defaults) {
                    Ok(()) => Reopened::Reloaded { history_cleared },
                    Err(err) => Reopened::Failed(err.to_string()),
                }
            }
        }
    }

    /// How `disk` differs from the baseline, if it does.
    fn conflict(&self, disk: DiskTiles) -> Option<Conflict> {
        let kind = match (self.origin.file_bytes(), disk.0.as_deref()) {
            (a, b) if a == b => return None,
            (Some(_), None) => ConflictKind::Deleted,
            (None, Some(_)) => ConflictKind::Created,
            _ => ConflictKind::Modified,
        };
        Some(Conflict { kind, disk })
    }

    /// Replaces `sprites.bin`, provided it holds `expected`, and makes the
    /// written tiles the baseline.
    fn write_file(
        &mut self,
        cartridge: &LoadedCartridge,
        expected: Option<&[u8]>,
    ) -> Result<Saved, SaveError> {
        let bytes = self.save_bytes();
        let len = bytes.len();
        match cartridge.replace_obj_tiles(bytes, expected) {
            Ok(()) => {}
            Err(ObjTilesWriteError::Failed(err)) => return Err(SaveError::Cartridge(err)),
            Err(ObjTilesWriteError::Changed(current)) => {
                let disk = DiskTiles(current);
                return Err(SaveError::Conflict(self.conflict(disk.clone()).unwrap_or(
                    Conflict {
                        kind: ConflictKind::Modified,
                        disk,
                    },
                )));
            }
        }
        self.origin = TileOrigin {
            bank: self.tiles().clone(),
            file_len: Some(len),
        };
        self.changed_pixels = 0;
        tracing::info!(bytes = len, "sprites.bin saved");
        Ok(Saved::Written { bytes: len })
    }

    /// Makes `disk` the tiles and the baseline, clearing the history.
    fn replace_tiles(&mut self, disk: DiskTiles) -> Result<(), VideoLoadError> {
        let bank = match &disk.0 {
            Some(bytes) => ObjTileBank::from_bytes(bytes).map_err(VideoAssetError::from)?,
            None => ObjTileBank::new(),
        };
        *self.video.obj.tiles_mut() = bank.clone();
        self.origin = TileOrigin {
            bank,
            file_len: disk.0.as_ref().map(Vec::len),
        };
        self.changed_pixels = 0;
        self.history = History::default();
        tracing::info!("sprites.bin reloaded");
        Ok(())
    }

    /// Takes the read-only assets from `files`, keeping the tiles.
    fn refresh_read_only(
        &mut self,
        files: &VideoFiles,
        defaults: &CartridgeVideoDefaults,
    ) -> Result<(), VideoAssetError> {
        let fresh = CartridgeVideo::decode(
            &VideoFiles {
                obj_tiles: None,
                ..files.clone()
            },
            defaults,
        )?;
        self.video.palette = fresh.palette;
        self.video.obj = ObjVram::new(
            self.tiles().clone(),
            fresh.obj.attributes().clone(),
            fresh.obj.palettes().clone(),
        );
        Ok(())
    }

    /// Writes one pixel, keeping the count of changed pixels. Returns the
    /// previous index if the pixel changed.
    fn write(&mut self, tile: usize, x: usize, y: usize, index: u8) -> Option<u8> {
        let previous = self.video.obj.tiles_mut().set_pixel(tile, x, y, index)?;
        if previous == index {
            return None;
        }
        let original = self.origin.bank.pixel(tile, x, y);
        if original == Some(previous) {
            self.changed_pixels += 1;
        } else if original == Some(index) {
            self.changed_pixels -= 1;
        }
        Some(previous)
    }
}

#[cfg(test)]
mod tests;
