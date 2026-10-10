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
mod tests {
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
    fn the_eraser_index_reads_back_as_transparent() {
        let mut doc = document(Some(vec![0xff; OBJ_TILE_BYTES]));
        assert!(doc.set_pixel(0, 6, 1, 0));
        assert_eq!(doc.pixel(0, 6, 1), Some(0));
        let resolved = doc
            .obj()
            .palettes()
            .resolve(doc.obj().attributes().bank(0), 0);
        assert_eq!(resolved, None, "local 0 is transparent in every bank");
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
    fn the_origin_keeps_the_bytes_read_through_edits() {
        let bytes = vec![0x12; OBJ_TILE_BYTES * 3];
        let mut doc = document(Some(bytes.clone()));
        doc.set_pixel(0, 0, 0, 9);
        doc.set_pixel(5, 0, 0, 9);
        assert_eq!(doc.origin().file_bytes(), Some(&bytes[..]));

        assert_eq!(document(None).origin().file_bytes(), None);
        assert_eq!(
            document(Some(Vec::new())).origin().file_bytes(),
            Some(&[][..])
        );
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
}
