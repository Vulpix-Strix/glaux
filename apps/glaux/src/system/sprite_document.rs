//! The loaded cartridge's video state as the editors change it, with the
//! edit history and the file state it is saved against.

use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};

use glaux_machine::obj_palette::ObjBank;
use glaux_machine::palette::Rgb;
use glaux_machine::sprite::{
    OBJ_TILE_BYTES, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH, ObjTileBank, ObjVram,
};

use super::video::{
    CartridgeVideo, CartridgeVideoDefaults, VideoAssetError, VideoFiles, VideoLoadError,
    VideoOverrides,
};
use crate::cartridge::{AssetWriteError, CartridgeError, LoadedCartridge, VideoAsset};

/// Edits the history keeps. Older ones are forgotten first.
pub const HISTORY_LIMIT: usize = 256;

/// Changes the undo and redo steps may hold together, about 512 KiB. Older
/// edits are forgotten first while there are more, but the newest edit is
/// always kept whatever its size. Editor strokes and fills stay inside one
/// tile, so they hold at most 64 changes and never reach this; it bounds
/// edits grouped across many tiles or assets.
pub const HISTORY_CHANGE_LIMIT: usize = 65_536;

/// The order saving writes assets in. It is fixed so that saves are
/// reproducible; it does not keep the files consistent with each other if
/// a save stops partway.
const SAVE_ORDER: [VideoAsset; 4] = [
    VideoAsset::MasterPalette,
    VideoAsset::ObjPalettes,
    VideoAsset::ObjAttributes,
    VideoAsset::ObjTiles,
];

const ASSETS: usize = VideoAsset::ALL.len();

/// What the asset files held when the document read or last saved them:
/// the state saving compares the disk against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// Each asset as decoded from its file, or its default where the
    /// cartridge had none.
    video: CartridgeVideo,
    /// Length of each asset's file, `None` where the cartridge had none.
    file_lens: [Option<usize>; ASSETS],
}

impl Origin {
    /// The exact bytes `asset`'s file held, or `None` when it was absent.
    ///
    /// A file always holds a prefix of its asset's native encoding, so the
    /// decoded state and the length are enough to give it back.
    pub fn file_bytes(&self, asset: VideoAsset) -> Option<Cow<'_, [u8]>> {
        let len = self.file_lens[asset.index()]?;
        Some(prefix(self.video.native_bytes(asset), len))
    }
}

fn prefix(bytes: Cow<'_, [u8]>, len: usize) -> Cow<'_, [u8]> {
    match bytes {
        Cow::Borrowed(bytes) => Cow::Borrowed(&bytes[..len]),
        Cow::Owned(mut bytes) => {
            bytes.truncate(len);
            Cow::Owned(bytes)
        }
    }
}

/// One asset file as found on disk at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskAsset {
    pub asset: VideoAsset,
    /// The file's bytes, `None` when it is absent.
    pub bytes: Option<Vec<u8>>,
}

/// How an asset file on disk differs from what the document was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// The file was changed.
    Modified,
    /// The file the document was read from is gone.
    Deleted,
    /// A file appeared where the cartridge had none.
    Created,
}

/// An asset file changed outside Glaux since the document's baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub kind: ConflictKind,
    /// What the file holds now. Overwriting is only allowed over exactly
    /// this.
    pub disk: DiskAsset,
}

impl Conflict {
    pub fn asset(&self) -> VideoAsset {
        self.disk.asset
    }
}

/// One asset file a save wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedFile {
    pub asset: VideoAsset,
    /// The file's new length.
    pub bytes: usize,
}

/// A successful [`SpriteDocument::save`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    /// These files were written, in this order: every asset with unsaved
    /// changes.
    Written(Vec<SavedFile>),
    /// The document already matched its baseline; nothing was written.
    NothingToSave,
}

/// Why a save stopped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SaveFailure {
    /// These files changed outside Glaux. A conflict found before writing
    /// lists every such file; one found while writing lists that file.
    #[error("{} changed outside Glaux", file_list(.0.iter().map(Conflict::asset)))]
    Conflict(Vec<Conflict>),
    /// Reading or writing a file failed; that file is unchanged on disk.
    #[error(transparent)]
    Cartridge(CartridgeError),
}

/// A save that stopped before writing every changed asset.
///
/// Each file is replaced whole or not at all, but the files of one save
/// are not replaced together: the assets in `written` were saved and are
/// unmodified now, while the rest keep their unsaved changes. Until they
/// are saved too, the files on disk may hold a combination of assets that
/// never existed together in the document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{cause}")]
pub struct SaveError {
    pub written: Vec<SavedFile>,
    pub cause: SaveFailure,
}

/// File names joined for messages, as in `obp.bin, sprites.bin`.
pub fn file_list(assets: impl IntoIterator<Item = VideoAsset>) -> String {
    assets
        .into_iter()
        .map(VideoAsset::file_name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// What [`SpriteDocument::reopen`] found on disk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reopened {
    /// Assets without unsaved changes whose files changed, and which were
    /// read again.
    pub reloaded: Vec<VideoAsset>,
    /// Whether undo or redo steps were dropped because they involved a
    /// reloaded asset.
    pub history_trimmed: bool,
    /// Assets whose files changed under unsaved changes. They were not
    /// touched.
    pub conflicts: Vec<Conflict>,
}

/// What one change wrote to: a single value of one asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Target {
    Pixel { tile: u16, x: u8, y: u8 },
    TileBank { tile: u16 },
    ObjColor { bank: ObjBank, local: u8 },
    MasterColor { index: u8 },
}

impl Target {
    fn asset(self) -> VideoAsset {
        match self {
            Self::Pixel { .. } => VideoAsset::ObjTiles,
            Self::TileBank { .. } => VideoAsset::ObjAttributes,
            Self::ObjColor { .. } => VideoAsset::ObjPalettes,
            Self::MasterColor { .. } => VideoAsset::MasterPalette,
        }
    }
}

/// One value an edit changed, with what it held before and after.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "no editor control changes attributes or palettes yet"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    /// A tile-local palette index.
    Pixel {
        tile: u16,
        x: u8,
        y: u8,
        before: u8,
        after: u8,
    },
    /// A tile's default object palette bank.
    TileBank {
        tile: u16,
        before: ObjBank,
        after: ObjBank,
    },
    /// The master index a local index resolves to in an object palette
    /// bank.
    ObjColor {
        bank: ObjBank,
        local: u8,
        before: u8,
        after: u8,
    },
    /// A master palette color.
    MasterColor { index: u8, before: Rgb, after: Rgb },
}

// The history stores many changes; keep each one as small as a pixel's.
const _: () = assert!(size_of::<Change>() <= 8);

impl Change {
    fn target(self) -> Target {
        match self {
            Self::Pixel { tile, x, y, .. } => Target::Pixel { tile, x, y },
            Self::TileBank { tile, .. } => Target::TileBank { tile },
            Self::ObjColor { bank, local, .. } => Target::ObjColor { bank, local },
            Self::MasterColor { index, .. } => Target::MasterColor { index },
        }
    }

    fn is_noop(self) -> bool {
        match self {
            Self::Pixel { before, after, .. } | Self::ObjColor { before, after, .. } => {
                before == after
            }
            Self::TileBank { before, after, .. } => before == after,
            Self::MasterColor { before, after, .. } => before == after,
        }
    }

    /// The same change with `after` taken from `later`, a change of the
    /// same target.
    fn followed_by(mut self, later: Self) -> Self {
        match (&mut self, later) {
            (Self::Pixel { after, .. }, Self::Pixel { after: new, .. })
            | (Self::ObjColor { after, .. }, Self::ObjColor { after: new, .. }) => *after = new,
            (Self::TileBank { after, .. }, Self::TileBank { after: new, .. }) => *after = new,
            (Self::MasterColor { after, .. }, Self::MasterColor { after: new, .. }) => {
                *after = new;
            }
            _ => unreachable!("changes of one target are of one kind"),
        }
        self
    }

    /// The change that undoes this one.
    fn reversed(mut self) -> Self {
        match &mut self {
            Self::Pixel { before, after, .. } | Self::ObjColor { before, after, .. } => {
                std::mem::swap(before, after);
            }
            Self::TileBank { before, after, .. } => std::mem::swap(before, after),
            Self::MasterColor { before, after, .. } => std::mem::swap(before, after),
        }
        self
    }
}

/// The values one user action changed, in the order first changed. One
/// edit may change several assets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Edit(Vec<Change>);

impl Edit {
    fn touches(&self, asset: VideoAsset) -> bool {
        self.0.iter().any(|c| c.target().asset() == asset)
    }
}

/// The edit still being recorded, with where it holds each target's change
/// so that recording stays linear in the size of the edit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct OpenEdit {
    edit: Edit,
    positions: HashMap<Target, usize>,
}

impl OpenEdit {
    /// Adds a write to the edit. A value written again keeps its first
    /// `before` and takes the new `after`.
    fn record(&mut self, change: Change) {
        let changes = &mut self.edit.0;
        match self.positions.get(&change.target()) {
            Some(&at) => changes[at] = changes[at].followed_by(change),
            None => {
                self.positions.insert(change.target(), changes.len());
                changes.push(change);
            }
        }
    }

    #[cfg(test)]
    fn changes_anything(&self) -> bool {
        self.edit.0.iter().any(|c| !c.is_noop())
    }
}

/// Undo and redo stacks, plus the edit still being recorded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct History {
    undo: VecDeque<Edit>,
    redo: Vec<Edit>,
    open: Option<OpenEdit>,
}

impl History {
    /// Files the open edit, if it changed anything, as the newest undo
    /// step. A new step makes the redo steps unreachable, so they go.
    fn close(&mut self) {
        let Some(OpenEdit {
            edit: Edit(mut changes),
            ..
        }) = self.open.take()
        else {
            return;
        };
        changes.retain(|c| !c.is_noop());
        if changes.is_empty() {
            return;
        }
        self.redo.clear();
        self.push_undo(Edit(changes));
    }

    /// Adds the newest undo step, forgetting the oldest ones past
    /// [`HISTORY_LIMIT`] edits or [`HISTORY_CHANGE_LIMIT`] changes.
    fn push_undo(&mut self, edit: Edit) {
        self.undo.push_back(edit);
        let mut stored: usize = self.undo.iter().chain(&self.redo).map(|e| e.0.len()).sum();
        while self.undo.len() > HISTORY_LIMIT
            || (stored > HISTORY_CHANGE_LIMIT && self.undo.len() > 1)
        {
            if let Some(oldest) = self.undo.pop_front() {
                stored -= oldest.0.len();
            }
        }
    }

    /// Drops the steps that no longer apply once `asset` was replaced
    /// outside the history. Returns whether any were dropped.
    ///
    /// Edits are never split, since an edit's changes only make sense
    /// together. The newest undo step involving `asset` goes with every
    /// older one, and the first redo step involving it goes with every one
    /// after it. The steps left never involve `asset` and lead from the
    /// current state without a gap, so undoing and redoing them stays
    /// exact.
    fn invalidate(&mut self, asset: VideoAsset) -> bool {
        self.close();
        let mut dropped = false;
        if let Some(newest) = self.undo.iter().rposition(|edit| edit.touches(asset)) {
            self.undo.drain(..=newest);
            dropped = true;
        }
        // The last redo step is the next one redone.
        if let Some(first) = self.redo.iter().rposition(|edit| edit.touches(asset)) {
            self.redo.drain(..=first);
            dropped = true;
        }
        dropped
    }
}

/// A cartridge's video state as the editors change it, held in memory.
///
/// Every asset is editable: object tiles, their attributes, the object
/// palette banks and the master palette. The document remembers what each
/// asset's file held when it was read or last saved, so it knows exactly
/// which assets differ from their files: restoring every changed value, by
/// hand or by undo, leaves an asset unmodified again.
///
/// Changes are grouped into edits between [`begin_edit`](Self::begin_edit)
/// and [`end_edit`](Self::end_edit), each undone and redone whole, whatever
/// assets it involves. Saving keeps the history; reading an asset from disk
/// again drops the steps involving it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteDocument {
    video: CartridgeVideo,
    origin: Origin,
    /// Per asset, how many of its values differ from `origin`'s: pixels,
    /// tile attributes, object palette entries or master colors.
    changed: [usize; ASSETS],
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
        let origin = Origin {
            video: video.clone(),
            file_lens: VideoAsset::ALL.map(|asset| files.get(asset).map(<[u8]>::len)),
        };
        Ok(Self {
            video,
            origin,
            changed: [0; ASSETS],
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

    /// What the assets are saved against, to tell whether their files
    /// changed on disk since.
    #[cfg(test)]
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// Whether any asset differs from what its file held when last read or
    /// saved.
    pub fn is_modified(&self) -> bool {
        self.changed.iter().any(|&n| n > 0)
    }

    /// Whether `asset` differs from what its file held when last read or
    /// saved, so saving would write it.
    pub fn is_asset_modified(&self, asset: VideoAsset) -> bool {
        self.changed[asset.index()] > 0
    }

    /// The assets saving would write.
    pub fn modified_assets(&self) -> impl Iterator<Item = VideoAsset> + '_ {
        VideoAsset::ALL
            .into_iter()
            .filter(|&asset| self.is_asset_modified(asset))
    }

    /// The modified assets, to stand in for the cartridge's files in a run.
    pub fn unsaved(&self) -> VideoOverrides<'_> {
        VideoOverrides::new(&self.video, self.modified_assets())
    }

    /// Whether there is an edit to undo.
    #[cfg(test)]
    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
            || self
                .history
                .open
                .as_ref()
                .is_some_and(OpenEdit::changes_anything)
    }

    /// Whether there is an undone edit to redo.
    #[cfg(test)]
    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    /// One tile-local palette index; `None` for an invalid tile or pixel.
    pub fn pixel(&self, tile: usize, x: usize, y: usize) -> Option<u8> {
        self.tiles().pixel(tile, x, y)
    }

    /// Starts recording one undoable edit, closing any still open. Every
    /// change until [`end_edit`](Self::end_edit) is undone as one, whatever
    /// assets it involves.
    pub fn begin_edit(&mut self) {
        self.history.close();
        self.history.open = Some(OpenEdit::default());
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
        let Some(previous) = self.write_pixel(tile, x, y, index) else {
            return false;
        };
        // `write_pixel` only succeeds for a valid tile and pixel, which fit.
        self.record(Change::Pixel {
            tile: tile as u16,
            x: x as u8,
            y: y as u8,
            before: previous,
            after: index,
        });
        true
    }

    /// Makes `bank` the default object palette bank of `tile`. Returns
    /// whether it changed; tiles past the bank change nothing.
    ///
    /// Outside an open edit, the write is an edit of its own.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "no editor control changes attributes or palettes yet"
        )
    )]
    pub fn set_tile_bank(&mut self, tile: usize, bank: ObjBank) -> bool {
        let Some(previous) = self.write_tile_bank(tile, bank) else {
            return false;
        };
        // `write_tile_bank` only succeeds for a valid tile, which fits.
        self.record(Change::TileBank {
            tile: tile as u16,
            before: previous,
            after: bank,
        });
        true
    }

    /// Maps `local` in object palette `bank` to master index `master`.
    /// Returns whether the mapping changed; the transparent local 0 and
    /// locals above 15 change nothing.
    ///
    /// Outside an open edit, the write is an edit of its own.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "no editor control changes attributes or palettes yet"
        )
    )]
    pub fn set_obj_color(&mut self, bank: ObjBank, local: u8, master: u8) -> bool {
        let Some(previous) = self.write_obj_color(bank, local, master) else {
            return false;
        };
        self.record(Change::ObjColor {
            bank,
            local,
            before: previous,
            after: master,
        });
        true
    }

    /// Sets master palette entry `index` to `color`. Returns whether it
    /// changed.
    ///
    /// Outside an open edit, the write is an edit of its own.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "no editor control changes attributes or palettes yet"
        )
    )]
    pub fn set_master_color(&mut self, index: u8, color: Rgb) -> bool {
        let Some(previous) = self.write_master_color(index, color) else {
            return false;
        };
        self.record(Change::MasterColor {
            index,
            before: previous,
            after: color,
        });
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
            self.apply(change.reversed());
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
        for &change in &edit.0 {
            self.apply(change);
        }
        self.history.push_undo(edit);
        true
    }

    /// The bytes saving writes for `asset`. The palettes are written whole.
    /// The tiles and their attributes keep the whole original file length,
    /// grown in whole tiles to cover every non-empty tile past it.
    pub fn save_bytes(&self, asset: VideoAsset) -> Cow<'_, [u8]> {
        let bytes = self.video.native_bytes(asset);
        let unit = match asset {
            VideoAsset::ObjTiles => OBJ_TILE_BYTES,
            VideoAsset::ObjAttributes => 1,
            VideoAsset::ObjPalettes | VideoAsset::MasterPalette => return bytes,
        };
        let used = bytes
            .chunks(unit)
            .rposition(|tile| tile.iter().any(|&b| b != 0))
            .map_or(0, |last| (last + 1) * unit);
        let original = self.origin.file_lens[asset.index()].unwrap_or(0);
        prefix(bytes, used.max(original))
    }

    /// Writes every modified asset to its file, provided each still holds
    /// exactly what the document was read from or last saved to. A file
    /// that was absent is created only once its asset differs from the
    /// default it stood in for.
    ///
    /// Every file to write is checked first, and a conflict or read error
    /// there writes nothing. The files are then replaced one at a time, in
    /// a fixed order; each saved asset becomes its new baseline at once.
    /// The first failure stops the save, reporting what was written: the
    /// assets after it keep their unsaved changes, and saving again writes
    /// only those. The history is kept either way.
    ///
    /// Each replacement compares its file once more just before it takes
    /// effect, so a change made after the first check is a conflict for
    /// that file. No lock is taken: a change in the instant between that
    /// last comparison and the replacement is overwritten.
    pub fn save(&mut self, cartridge: &LoadedCartridge) -> Result<Saved, SaveError> {
        self.write_files(cartridge, &[])
    }

    /// Like [`save`](Self::save), but writes the assets in `seen` although
    /// their files changed outside Glaux, provided each still holds exactly
    /// what was seen. A further change is another conflict.
    pub fn overwrite(
        &mut self,
        cartridge: &LoadedCartridge,
        seen: &[DiskAsset],
    ) -> Result<Saved, SaveError> {
        self.write_files(cartridge, seen)
    }

    /// Discards the changes to `assets` and the history steps involving
    /// them, and reads their files again; an absent file reads as the
    /// asset's default. The other assets keep their changes. On error the
    /// document is unchanged.
    pub fn reload(
        &mut self,
        cartridge: &LoadedCartridge,
        assets: &[VideoAsset],
        defaults: &CartridgeVideoDefaults,
    ) -> Result<(), VideoLoadError> {
        self.history.close();
        let mut fresh = self.video.clone();
        let mut lens = self.origin.file_lens;
        for &asset in assets {
            let bytes = cartridge
                .read_asset(asset)
                .map_err(VideoLoadError::Cartridge)?;
            fresh.decode_asset(asset, bytes.as_deref(), defaults)?;
            lens[asset.index()] = bytes.as_ref().map(Vec::len);
        }
        for &asset in assets {
            self.adopt(&fresh, asset, lens[asset.index()]);
            self.history.invalidate(asset);
            tracing::info!(file = asset.file_name(), "asset reloaded");
        }
        Ok(())
    }

    /// Brings the document up to date with the cartridge's files when the
    /// workspace opens again, never discarding unsaved changes. Each asset
    /// is treated on its own:
    ///
    /// - file unchanged: the asset and its history are kept.
    /// - file changed, asset unmodified: the asset is read again, and the
    ///   history steps involving it are dropped.
    /// - file changed under unsaved changes: the asset is not touched; the
    ///   conflict is returned for the user to settle.
    ///
    /// When a file can't be read, or a changed file to read again is
    /// invalid, nothing changes and the reason is returned.
    pub fn reopen(
        &mut self,
        cartridge: &LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
    ) -> Result<Reopened, String> {
        self.history.close();
        let files = VideoFiles::read(cartridge).map_err(|err| err.to_string())?;
        let mut fresh = self.video.clone();
        let mut reopened = Reopened::default();
        for asset in VideoAsset::ALL {
            let disk = files.get(asset);
            let Some(kind) = self.conflict_kind(asset, disk) else {
                continue;
            };
            if self.is_asset_modified(asset) {
                reopened.conflicts.push(Conflict {
                    kind,
                    disk: DiskAsset {
                        asset,
                        bytes: disk.map(<[u8]>::to_vec),
                    },
                });
            } else {
                fresh
                    .decode_asset(asset, disk, defaults)
                    .map_err(|err| err.to_string())?;
                reopened.reloaded.push(asset);
            }
        }
        for &asset in &reopened.reloaded {
            self.adopt(&fresh, asset, files.get(asset).map(<[u8]>::len));
            reopened.history_trimmed |= self.history.invalidate(asset);
            tracing::info!(file = asset.file_name(), "asset changed on disk, reloaded");
        }
        Ok(reopened)
    }

    /// How `disk` differs from `asset`'s baseline, if it does.
    fn conflict_kind(&self, asset: VideoAsset, disk: Option<&[u8]>) -> Option<ConflictKind> {
        Some(match (self.origin.file_bytes(asset).as_deref(), disk) {
            (a, b) if a == b => return None,
            (Some(_), None) => ConflictKind::Deleted,
            (None, Some(_)) => ConflictKind::Created,
            _ => ConflictKind::Modified,
        })
    }

    /// The conflict `disk` makes with `asset`'s baseline. A file that
    /// matches the baseline but not what an overwrite expected still
    /// changed since it was seen.
    fn conflict(&self, asset: VideoAsset, disk: Option<Vec<u8>>) -> Conflict {
        Conflict {
            kind: self
                .conflict_kind(asset, disk.as_deref())
                .unwrap_or(ConflictKind::Modified),
            disk: DiskAsset { asset, bytes: disk },
        }
    }

    /// Saves every modified asset, expecting its file to hold what `seen`
    /// says, or else its baseline. See [`save`](Self::save).
    fn write_files(
        &mut self,
        cartridge: &LoadedCartridge,
        seen: &[DiskAsset],
    ) -> Result<Saved, SaveError> {
        self.history.close();
        let dirty: Vec<VideoAsset> = SAVE_ORDER
            .into_iter()
            .filter(|&asset| self.is_asset_modified(asset))
            .collect();
        if dirty.is_empty() {
            return Ok(Saved::NothingToSave);
        }
        let expected = |asset: VideoAsset| -> Option<Vec<u8>> {
            match seen.iter().find(|disk| disk.asset == asset) {
                Some(disk) => disk.bytes.clone(),
                None => self.origin.file_bytes(asset).map(Cow::into_owned),
            }
        };
        let expected: Vec<Option<Vec<u8>>> = dirty.iter().map(|&asset| expected(asset)).collect();

        let mut conflicts = Vec::new();
        for (&asset, expected) in dirty.iter().zip(&expected) {
            let disk = cartridge.read_asset(asset).map_err(|err| SaveError {
                written: Vec::new(),
                cause: SaveFailure::Cartridge(err),
            })?;
            if disk != *expected {
                conflicts.push(self.conflict(asset, disk));
            }
        }
        if !conflicts.is_empty() {
            return Err(SaveError {
                written: Vec::new(),
                cause: SaveFailure::Conflict(conflicts),
            });
        }

        let mut written = Vec::new();
        for (asset, expected) in dirty.into_iter().zip(expected) {
            let bytes = self.save_bytes(asset);
            let len = bytes.len();
            let result = cartridge.replace_asset(asset, &bytes, expected.as_deref());
            drop(bytes);
            let cause = match result {
                Ok(()) => {
                    self.mark_saved(asset, len);
                    tracing::info!(file = asset.file_name(), bytes = len, "asset saved");
                    written.push(SavedFile { asset, bytes: len });
                    continue;
                }
                Err(AssetWriteError::Failed(err)) => SaveFailure::Cartridge(err),
                Err(AssetWriteError::Changed(current)) => {
                    SaveFailure::Conflict(vec![self.conflict(asset, current)])
                }
            };
            if !written.is_empty() {
                tracing::warn!(
                    saved = %file_list(written.iter().map(|file| file.asset)),
                    failed = asset.file_name(),
                    "save stopped partway"
                );
            }
            return Err(SaveError { written, cause });
        }
        Ok(Saved::Written(written))
    }

    /// Makes the current state of `asset`, just written as `len` bytes, its
    /// baseline.
    fn mark_saved(&mut self, asset: VideoAsset, len: usize) {
        self.origin.video.copy_asset(&self.video, asset);
        self.origin.file_lens[asset.index()] = Some(len);
        self.changed[asset.index()] = 0;
    }

    /// Makes `asset` as `fresh` holds it, read from a file of `len` bytes,
    /// both the current state and the baseline.
    fn adopt(&mut self, fresh: &CartridgeVideo, asset: VideoAsset, len: Option<usize>) {
        self.video.copy_asset(fresh, asset);
        self.origin.video.copy_asset(fresh, asset);
        self.origin.file_lens[asset.index()] = len;
        self.changed[asset.index()] = 0;
    }

    /// Adds a change to the open edit, or makes it an edit of its own.
    fn record(&mut self, change: Change) {
        match &mut self.history.open {
            Some(edit) => edit.record(change),
            None => {
                self.history.redo.clear();
                self.history.push_undo(Edit(vec![change]));
            }
        }
    }

    /// Writes the `after` value of `change`, as undo and redo do.
    fn apply(&mut self, change: Change) {
        match change {
            Change::Pixel {
                tile, x, y, after, ..
            } => {
                self.write_pixel(tile.into(), x.into(), y.into(), after);
            }
            Change::TileBank { tile, after, .. } => {
                self.write_tile_bank(tile.into(), after);
            }
            Change::ObjColor {
                bank, local, after, ..
            } => {
                self.write_obj_color(bank, local, after);
            }
            Change::MasterColor { index, after, .. } => {
                self.write_master_color(index, after);
            }
        }
    }

    /// Counts a value of `asset` going from `previous` to `new` against
    /// its baseline value `original`.
    fn track<T: PartialEq>(&mut self, asset: VideoAsset, original: T, previous: T, new: T) {
        let changed = &mut self.changed[asset.index()];
        if original == previous {
            *changed += 1;
        } else if original == new {
            *changed -= 1;
        }
    }

    /// Writes one pixel. Returns the previous index if the pixel changed.
    fn write_pixel(&mut self, tile: usize, x: usize, y: usize, index: u8) -> Option<u8> {
        let previous = self.video.obj.tiles_mut().set_pixel(tile, x, y, index)?;
        if previous == index {
            return None;
        }
        let original = self.origin.video.obj.tiles().pixel(tile, x, y);
        self.track(VideoAsset::ObjTiles, original, Some(previous), Some(index));
        Some(previous)
    }

    /// Writes one tile's bank. Returns the previous bank if it changed.
    fn write_tile_bank(&mut self, tile: usize, bank: ObjBank) -> Option<ObjBank> {
        let previous = self.video.obj.attributes_mut().set_bank(tile, bank)?;
        if previous == bank {
            return None;
        }
        let original = self.origin.video.obj.attributes().bank(tile);
        self.track(VideoAsset::ObjAttributes, original, previous, bank);
        Some(previous)
    }

    /// Writes one object palette entry. Returns the previous master index
    /// if it changed.
    fn write_obj_color(&mut self, bank: ObjBank, local: u8, master: u8) -> Option<u8> {
        let previous = self.video.obj.palettes_mut().set(bank, local, master)?;
        if previous == master {
            return None;
        }
        let original = self.origin.video.obj.palettes().resolve(bank, local);
        self.track(
            VideoAsset::ObjPalettes,
            original,
            Some(previous),
            Some(master),
        );
        Some(previous)
    }

    /// Writes one master color. Returns the previous color if it changed.
    fn write_master_color(&mut self, index: u8, color: Rgb) -> Option<Rgb> {
        let previous = self.video.palette.set(index, color);
        if previous == color {
            return None;
        }
        let original = self.origin.video.palette.get(index);
        self.track(VideoAsset::MasterPalette, original, previous, color);
        Some(previous)
    }
}

#[cfg(test)]
mod tests;
