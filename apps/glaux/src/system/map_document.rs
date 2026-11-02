//! One map of the loaded cartridge as the editors change it, with its edit
//! history and the file state it is saved against.

use std::collections::{HashMap, VecDeque};

use glaux_machine::tilemap::{BG_LAYERS, BgLayer, TileEntry, TileLayer};

use super::sprite_document::ConflictKind;
use crate::cartridge::LoadedCartridge;
use crate::cartridge::map::{MapError, MapFile, MapFiles, MapId, MapMeta, MapWriteError};

/// Edits the history keeps. Older ones are forgotten first.
pub const HISTORY_LIMIT: usize = 256;

/// Changes the undo and redo steps may hold together, about 1 MiB. Older
/// edits are forgotten first while there are more, but the newest edit is
/// always kept whatever its size.
pub const HISTORY_CHANGE_LIMIT: usize = 131_072;

/// The order saving writes files in, layers first and `map.ron` last. It is
/// fixed so that saves are reproducible. It does not keep the files consistent with
/// each other if a save stops partway.
const SAVE_ORDER: [MapFile; 4] = [
    MapFile::Layer(BgLayer::Bg0),
    MapFile::Layer(BgLayer::Bg1),
    MapFile::Layer(BgLayer::Bg2),
    MapFile::Meta,
];

/// Where a file's bytes sit in [`Origin::files`].
const fn slot(file: MapFile) -> usize {
    match file {
        MapFile::Meta => 0,
        MapFile::Layer(layer) => 1 + layer.index(),
    }
}

/// One map file as found on disk at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskFile {
    pub file: MapFile,
    /// The file's bytes, `None` when it is absent.
    pub bytes: Option<Vec<u8>>,
}

/// A map file changed outside Glaux since the document's baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub kind: ConflictKind,
    /// What the file holds now. Overwriting is only allowed over exactly
    /// this.
    pub disk: DiskFile,
}

impl Conflict {
    pub fn file(&self) -> MapFile {
        self.disk.file
    }
}

/// What a save did to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileWrite {
    /// The file now holds this many bytes.
    Written(usize),
    /// A layer that became entirely empty has no file any more.
    Removed,
}

/// One file a save changed on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedFile {
    pub file: MapFile,
    pub write: FileWrite,
}

/// A successful [`MapDocument::save`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    /// These files were changed on disk, in this order. A layer that was
    /// empty and still is has no file and is not listed.
    Written(Vec<SavedFile>),
    /// The document already matched its baseline. Nothing was written.
    NothingToSave,
}

/// Why a save stopped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SaveFailure {
    /// These files changed outside Glaux. A conflict found before writing
    /// lists every such file. One found while writing lists that file.
    #[error("{} changed outside Glaux", file_list(.0.iter().map(Conflict::file)))]
    Conflict(Vec<Conflict>),
    /// Reading or writing a file failed. That file is unchanged on disk.
    #[error(transparent)]
    Map(MapError),
}

/// A save that stopped before writing every changed file.
///
/// Each file is replaced whole or not at all, but the files of one save
/// are not atomic together. The files in `written` are saved. The rest keep
/// their unsaved changes, and a later save writes only those.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{cause}")]
pub struct SaveError {
    pub written: Vec<SavedFile>,
    pub cause: SaveFailure,
}

/// File names joined for messages, as in `bg0.bin, map.ron`.
pub fn file_list(files: impl IntoIterator<Item = MapFile>) -> String {
    files
        .into_iter()
        .map(MapFile::file_name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// What [`MapDocument::reopen`] found on disk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reopened {
    /// Files without unsaved changes that changed on disk, and were read
    /// again.
    pub reloaded: Vec<MapFile>,
    /// Whether undo or redo steps were dropped because they involved a
    /// reloaded file.
    pub history_trimmed: bool,
    /// Files that changed on disk under unsaved changes. They were not
    /// touched.
    pub conflicts: Vec<Conflict>,
}

/// The parts of `map.ron` the editors change. The dimensions are fixed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Labels {
    name: String,
    layer_names: [Option<String>; BG_LAYERS],
}

impl Labels {
    fn of(meta: &MapMeta) -> Self {
        Self {
            name: meta.name.clone(),
            layer_names: meta.layer_names.clone(),
        }
    }
}

/// One cell written. A map holds at most 65,536 cells per layer, so the
/// index fits 16 bits and a change stays 8 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellChange {
    layer: u8,
    index: u16,
    before: u16,
    after: u16,
}

const _: () = assert!(size_of::<CellChange>() <= 8);

#[derive(Debug, Clone, PartialEq, Eq)]
struct LabelChange {
    before: Box<Labels>,
    after: Box<Labels>,
}

/// What one user action changed. Cells are listed in the order first
/// changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Edit {
    cells: Vec<CellChange>,
    labels: Option<LabelChange>,
}

impl Edit {
    fn size(&self) -> usize {
        self.cells.len() + usize::from(self.labels.is_some())
    }

    fn touches(&self, file: MapFile) -> bool {
        match file {
            MapFile::Meta => self.labels.is_some(),
            MapFile::Layer(layer) => self
                .cells
                .iter()
                .any(|c| usize::from(c.layer) == layer.index()),
        }
    }
}

/// The edit still being recorded, with where it holds each cell's change so
/// that recording stays linear in the size of the edit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct OpenEdit {
    edit: Edit,
    positions: HashMap<(u8, u16), usize>,
}

impl OpenEdit {
    /// A cell written again keeps its first `before` and takes the new
    /// `after`.
    fn record_cell(&mut self, change: CellChange) {
        let cells = &mut self.edit.cells;
        match self.positions.get(&(change.layer, change.index)) {
            Some(&at) => cells[at].after = change.after,
            None => {
                self.positions
                    .insert((change.layer, change.index), cells.len());
                cells.push(change);
            }
        }
    }

    fn record_labels(&mut self, before: Labels, after: Labels) {
        match &mut self.edit.labels {
            Some(change) => *change.after = after,
            None => {
                self.edit.labels = Some(LabelChange {
                    before: Box::new(before),
                    after: Box::new(after),
                });
            }
        }
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
        let Some(OpenEdit { mut edit, .. }) = self.open.take() else {
            return;
        };
        edit.cells.retain(|c| c.before != c.after);
        if edit.labels.as_ref().is_some_and(|l| l.before == l.after) {
            edit.labels = None;
        }
        if edit.size() == 0 {
            return;
        }
        self.redo.clear();
        self.push_undo(edit);
    }

    /// Adds the newest undo step, forgetting the oldest ones past
    /// [`HISTORY_LIMIT`] edits or [`HISTORY_CHANGE_LIMIT`] changes.
    fn push_undo(&mut self, edit: Edit) {
        self.undo.push_back(edit);
        let mut stored: usize = self.undo.iter().chain(&self.redo).map(Edit::size).sum();
        while self.undo.len() > HISTORY_LIMIT
            || (stored > HISTORY_CHANGE_LIMIT && self.undo.len() > 1)
        {
            if let Some(oldest) = self.undo.pop_front() {
                stored -= oldest.size();
            }
        }
    }

    /// Drops the steps that no longer apply once `file` was replaced
    /// outside the history. Returns whether any were dropped.
    ///
    /// Edits are never split. The newest undo step involving `file` goes
    /// with every older one, and the first redo step involving it goes with
    /// every one after it, so the steps left lead from the current state
    /// without a gap.
    fn invalidate(&mut self, file: MapFile) -> bool {
        self.close();
        let mut dropped = false;
        if let Some(newest) = self.undo.iter().rposition(|edit| edit.touches(file)) {
            self.undo.drain(..=newest);
            dropped = true;
        }
        // The last redo step is the next one redone.
        if let Some(first) = self.redo.iter().rposition(|edit| edit.touches(file)) {
            self.redo.drain(..=first);
            dropped = true;
        }
        dropped
    }
}

/// What each file held when the document was read or last saved.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Origin {
    meta: MapMeta,
    layers: [TileLayer; BG_LAYERS],
    /// The exact bytes of `map.ron` and the layer files, `None` for a
    /// missing file. Conflicts compare against these, never against a
    /// re-encoding, so hand-edited files are compared as they are.
    files: [Option<Vec<u8>>; 1 + BG_LAYERS],
}

/// A map as the editors change it, held in memory.
///
/// The document remembers what each file held when it was read or last
/// saved, so it knows exactly which files differ. Restoring every changed
/// value, by hand or by undo, leaves a file unmodified again.
///
/// Cell and name changes are grouped into edits between
/// [`begin_edit`](Self::begin_edit) and [`end_edit`](Self::end_edit), each
/// undone and redone whole. Saving keeps the history. Reading a file from
/// disk again drops the steps involving it.
///
/// A layer with no non-empty cell has no file. Saving removes it instead of
/// writing zeros.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapDocument {
    id: MapId,
    meta: MapMeta,
    layers: [TileLayer; BG_LAYERS],
    origin: Origin,
    /// Per layer, how many cells differ from `origin`'s.
    changed: [usize; BG_LAYERS],
    history: History,
}

impl MapDocument {
    /// Reads map `id` of the cartridge.
    pub fn load(cartridge: &LoadedCartridge, id: &MapId) -> Result<Self, MapError> {
        Self::from_files(id.clone(), &cartridge.read_map(id)?)
    }

    /// A document on `files` as read from a cartridge.
    pub fn from_files(id: MapId, files: &MapFiles) -> Result<Self, MapError> {
        let meta = MapMeta::parse(&files.meta)?;
        let mut layers = Vec::with_capacity(BG_LAYERS);
        for layer in BgLayer::ALL {
            let parsed = match &files.layers[layer.index()] {
                Some(bytes) => TileLayer::from_bytes(meta.width, meta.height, bytes),
                None => blank(&meta),
            };
            layers.push(parsed.map_err(|source| MapError::Layer { layer, source })?);
        }
        let layers: [TileLayer; BG_LAYERS] = layers
            .try_into()
            .expect("one layer was read for each background layer");
        let [bg0, bg1, bg2] = files.layers.clone();
        let origin = Origin {
            meta: meta.clone(),
            layers: layers.clone(),
            files: [Some(files.meta.clone()), bg0, bg1, bg2],
        };
        Ok(Self {
            id,
            meta,
            layers,
            origin,
            changed: [0; BG_LAYERS],
            history: History::default(),
        })
    }

    pub fn id(&self) -> &MapId {
        &self.id
    }

    pub fn meta(&self) -> &MapMeta {
        &self.meta
    }

    pub fn layer(&self, layer: BgLayer) -> &TileLayer {
        &self.layers[layer.index()]
    }

    /// Whether any file differs from what it held when last read or saved.
    pub fn is_modified(&self) -> bool {
        MapFile::ALL
            .into_iter()
            .any(|file| self.is_file_modified(file))
    }

    /// Whether `file` differs from what it held when last read or saved.
    pub fn is_file_modified(&self, file: MapFile) -> bool {
        match file {
            MapFile::Meta => self.meta != self.origin.meta,
            MapFile::Layer(layer) => self.changed[layer.index()] > 0,
        }
    }

    /// The files with unsaved changes, in the order a save writes them.
    pub fn modified_files(&self) -> impl Iterator<Item = MapFile> + '_ {
        SAVE_ORDER
            .into_iter()
            .filter(|&file| self.is_file_modified(file))
    }

    /// Opens an edit. The changes up to [`end_edit`](Self::end_edit) are
    /// undone and redone together. Closes an edit still open.
    pub fn begin_edit(&mut self) {
        self.history.close();
        self.history.open = Some(OpenEdit::default());
    }

    /// Closes the edit being recorded. An edit that changed nothing, or
    /// whose changes cancel out, leaves no history.
    pub fn end_edit(&mut self) {
        self.history.close();
    }

    /// Writes `entry` to cell `(x, y)` of `layer`. Returns whether the cell
    /// changed. Positions outside the map change nothing.
    ///
    /// Outside an open edit, the write is an edit of its own.
    pub fn set_cell(&mut self, layer: BgLayer, x: usize, y: usize, entry: TileEntry) -> bool {
        let Some(index) = self.layers[layer.index()].index_of(x, y) else {
            return false;
        };
        let Some(previous) = self.write_cell(layer, index, entry) else {
            return false;
        };
        // A layer has at most 256 x 256 cells and a layer index is below 3.
        self.record_cell(CellChange {
            layer: layer.index() as u8,
            index: index as u16,
            before: previous.raw(),
            after: entry.raw(),
        });
        true
    }

    /// Renames the map. Returns whether the name changed. A blank name
    /// changes nothing.
    ///
    /// Outside an open edit, the change is an edit of its own.
    pub fn set_name(&mut self, name: &str) -> bool {
        if name.trim().is_empty() || name == self.meta.name {
            return false;
        }
        let before = Labels::of(&self.meta);
        self.meta.name = name.to_owned();
        self.record_labels(before);
        true
    }

    /// Names `layer`, or removes its name with `None`. Returns whether it
    /// changed. A blank name changes nothing.
    ///
    /// Outside an open edit, the change is an edit of its own.
    pub fn set_layer_name(&mut self, layer: BgLayer, name: Option<&str>) -> bool {
        if name.is_some_and(|name| name.trim().is_empty()) {
            return false;
        }
        let name = name.map(str::to_owned);
        if self.meta.layer_names[layer.index()] == name {
            return false;
        }
        let before = Labels::of(&self.meta);
        self.meta.layer_names[layer.index()] = name;
        self.record_labels(before);
        true
    }

    /// Undoes the newest edit. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        self.history.close();
        let Some(edit) = self.history.undo.pop_back() else {
            return false;
        };
        for change in edit.cells.iter().rev() {
            self.apply_cell(change.layer, change.index, change.before);
        }
        if let Some(labels) = &edit.labels {
            self.apply_labels(&labels.before);
        }
        self.history.redo.push(edit);
        true
    }

    /// Redoes the newest undone edit. Returns whether there was one.
    pub fn redo(&mut self) -> bool {
        self.history.close();
        let Some(edit) = self.history.redo.pop() else {
            return false;
        };
        for change in &edit.cells {
            self.apply_cell(change.layer, change.index, change.after);
        }
        if let Some(labels) = &edit.labels {
            self.apply_labels(&labels.after);
        }
        self.history.undo.push_back(edit);
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
            || self
                .history
                .open
                .as_ref()
                .is_some_and(|o| o.edit.size() > 0)
    }

    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    /// Writes every file with unsaved changes, each only if it still holds
    /// what it held when the document was read or last saved.
    ///
    /// Files changed outside Glaux are reported as conflicts before
    /// anything is written. A layer that is entirely empty is removed
    /// rather than written. `map.ron` is written last.
    pub fn save(&mut self, cartridge: &LoadedCartridge) -> Result<Saved, SaveError> {
        self.write_files(cartridge, &[])
    }

    /// Like [`save`](Self::save), but each file in `seen` is expected to
    /// hold exactly what `seen` says, as found in an earlier conflict. This
    /// is how a user overwrites outside changes after looking at them.
    pub fn overwrite(
        &mut self,
        cartridge: &LoadedCartridge,
        seen: &[DiskFile],
    ) -> Result<Saved, SaveError> {
        self.write_files(cartridge, seen)
    }

    /// Brings the document up to date with the cartridge's files, as when
    /// the map is opened again.
    ///
    /// - file unchanged: the file and its history are kept.
    /// - file changed, no unsaved changes in it: it is read again, and the
    ///   history steps involving it are dropped.
    /// - file changed under unsaved changes: it is not touched, and the
    ///   conflict is returned.
    ///
    /// A `map.ron` that changes the dimensions replaces the layers too, so
    /// it is only read again when nothing in the document is modified.
    /// When a file can't be read or a changed file is invalid, nothing
    /// changes and the reason is returned.
    pub fn reopen(&mut self, cartridge: &LoadedCartridge) -> Result<Reopened, MapError> {
        self.history.close();
        let files = cartridge.read_map(&self.id)?;
        let fresh = Self::from_files(self.id.clone(), &files)?;
        let disk = |file: MapFile| -> Option<&[u8]> {
            match file {
                MapFile::Meta => Some(&files.meta),
                MapFile::Layer(layer) => files.layers[layer.index()].as_deref(),
            }
        };
        let mut reopened = Reopened::default();

        let resized = (fresh.meta.width, fresh.meta.height) != (self.meta.width, self.meta.height);
        if resized {
            if self.is_modified() {
                reopened
                    .conflicts
                    .push(self.conflict(MapFile::Meta, Some(files.meta.clone())));
                return Ok(reopened);
            }
            reopened.reloaded = MapFile::ALL
                .into_iter()
                .filter(|&file| self.conflict_kind(file, disk(file)).is_some())
                .collect();
            reopened.history_trimmed = self.history != History::default();
            *self = fresh;
            return Ok(reopened);
        }

        for file in MapFile::ALL {
            let Some(kind) = self.conflict_kind(file, disk(file)) else {
                continue;
            };
            if self.is_file_modified(file) {
                reopened.conflicts.push(Conflict {
                    kind,
                    disk: DiskFile {
                        file,
                        bytes: disk(file).map(<[u8]>::to_vec),
                    },
                });
                continue;
            }
            self.adopt(&fresh, file);
            reopened.reloaded.push(file);
            reopened.history_trimmed |= self.history.invalidate(file);
            tracing::info!(
                file = file.file_name(),
                "map file changed on disk, reloaded"
            );
        }
        Ok(reopened)
    }

    /// Writes `entry` into the cell and keeps `changed` current. Returns
    /// the entry it replaced, or `None` when nothing changed.
    fn write_cell(&mut self, layer: BgLayer, index: usize, entry: TileEntry) -> Option<TileEntry> {
        let current = self.layers[layer.index()].get_index(index)?;
        if current == entry {
            return None;
        }
        self.layers[layer.index()].set_index(index, entry);
        let original = self.origin.layers[layer.index()].get_index(index);
        let changed = &mut self.changed[layer.index()];
        if original == Some(current) {
            *changed += 1;
        } else if original == Some(entry) {
            *changed -= 1;
        }
        Some(current)
    }

    /// Writes a value taken from the history, which is always valid.
    fn apply_cell(&mut self, layer: u8, index: u16, raw: u16) {
        let layer = BgLayer::from_index(layer.into()).expect("history holds valid layers");
        let entry = TileEntry::from_raw(raw).expect("history holds valid entries");
        self.write_cell(layer, index.into(), entry);
    }

    fn apply_labels(&mut self, labels: &Labels) {
        self.meta.name.clone_from(&labels.name);
        self.meta.layer_names.clone_from(&labels.layer_names);
    }

    /// Adds a change to the open edit, or makes it an edit of its own.
    fn record_cell(&mut self, change: CellChange) {
        match &mut self.history.open {
            Some(open) => open.record_cell(change),
            None => {
                self.history.redo.clear();
                self.history.push_undo(Edit {
                    cells: vec![change],
                    labels: None,
                });
            }
        }
    }

    /// Records that the labels changed from `before` to the current ones.
    fn record_labels(&mut self, before: Labels) {
        let after = Labels::of(&self.meta);
        match &mut self.history.open {
            Some(open) => open.record_labels(before, after),
            None => {
                self.history.redo.clear();
                self.history.push_undo(Edit {
                    cells: Vec::new(),
                    labels: Some(LabelChange {
                        before: Box::new(before),
                        after: Box::new(after),
                    }),
                });
            }
        }
    }

    /// How `disk` differs from `file`'s baseline, if it does.
    fn conflict_kind(&self, file: MapFile, disk: Option<&[u8]>) -> Option<ConflictKind> {
        Some(match (self.origin.files[slot(file)].as_deref(), disk) {
            (a, b) if a == b => return None,
            (Some(_), None) => ConflictKind::Deleted,
            (None, Some(_)) => ConflictKind::Created,
            _ => ConflictKind::Modified,
        })
    }

    /// The conflict `disk` makes with `file`'s baseline. A file that
    /// matches the baseline but not what an overwrite expected still
    /// changed since it was seen.
    fn conflict(&self, file: MapFile, disk: Option<Vec<u8>>) -> Conflict {
        Conflict {
            kind: self
                .conflict_kind(file, disk.as_deref())
                .unwrap_or(ConflictKind::Modified),
            disk: DiskFile { file, bytes: disk },
        }
    }

    /// What `file` holds on disk once saved. `None` is no file, which is
    /// what an entirely empty layer is.
    fn disk_bytes(&self, file: MapFile) -> Option<Vec<u8>> {
        match file {
            MapFile::Meta => Some(self.meta.to_bytes()),
            MapFile::Layer(layer) => {
                let layer = &self.layers[layer.index()];
                (!layer.is_blank()).then(|| layer.to_bytes())
            }
        }
    }

    /// Saves every modified file, expecting it to hold what `seen` says, or
    /// else its baseline. See [`save`](Self::save).
    fn write_files(
        &mut self,
        cartridge: &LoadedCartridge,
        seen: &[DiskFile],
    ) -> Result<Saved, SaveError> {
        self.history.close();
        let dirty: Vec<MapFile> = self.modified_files().collect();
        if dirty.is_empty() {
            return Ok(Saved::NothingToSave);
        }
        let expected: Vec<Option<Vec<u8>>> = dirty
            .iter()
            .map(|&file| match seen.iter().find(|disk| disk.file == file) {
                Some(disk) => disk.bytes.clone(),
                None => self.origin.files[slot(file)].clone(),
            })
            .collect();

        let mut conflicts = Vec::new();
        for (&file, expected) in dirty.iter().zip(&expected) {
            let disk = cartridge
                .read_map_file(&self.id, file)
                .map_err(|err| SaveError {
                    written: Vec::new(),
                    cause: SaveFailure::Map(err),
                })?;
            if disk != *expected {
                conflicts.push(self.conflict(file, disk));
            }
        }
        if !conflicts.is_empty() {
            return Err(SaveError {
                written: Vec::new(),
                cause: SaveFailure::Conflict(conflicts),
            });
        }

        let mut written = Vec::new();
        for (file, expected) in dirty.into_iter().zip(expected) {
            let bytes = self.disk_bytes(file);
            let result = match (&bytes, expected.as_deref()) {
                (Some(bytes), expected) => cartridge
                    .replace_map_file(&self.id, file, bytes, expected)
                    .map(|()| Some(FileWrite::Written(bytes.len()))),
                (None, Some(expected)) => cartridge
                    .remove_map_file(&self.id, file, expected)
                    .map(|()| Some(FileWrite::Removed)),
                (None, None) => Ok(None),
            };
            let cause = match result {
                Ok(write) => {
                    self.mark_saved(file, bytes);
                    if let Some(write) = write {
                        tracing::info!(file = file.file_name(), ?write, "map file saved");
                        written.push(SavedFile { file, write });
                    }
                    continue;
                }
                Err(MapWriteError::Failed(err)) => SaveFailure::Map(err.into()),
                Err(MapWriteError::Changed(current)) => {
                    SaveFailure::Conflict(vec![self.conflict(file, current)])
                }
            };
            if !written.is_empty() {
                tracing::warn!(
                    saved = %file_list(written.iter().map(|saved| saved.file)),
                    failed = file.file_name(),
                    "map save stopped partway"
                );
            }
            return Err(SaveError { written, cause });
        }
        Ok(Saved::Written(written))
    }

    /// Makes the current state of `file`, just saved as `bytes`, its
    /// baseline.
    fn mark_saved(&mut self, file: MapFile, bytes: Option<Vec<u8>>) {
        match file {
            MapFile::Meta => self.origin.meta = self.meta.clone(),
            MapFile::Layer(layer) => {
                self.origin.layers[layer.index()] = self.layers[layer.index()].clone();
                self.changed[layer.index()] = 0;
            }
        }
        self.origin.files[slot(file)] = bytes;
    }

    /// Makes `file` as `fresh` holds it both the current state and the
    /// baseline.
    fn adopt(&mut self, fresh: &Self, file: MapFile) {
        match file {
            MapFile::Meta => {
                self.meta = fresh.meta.clone();
                self.origin.meta = fresh.meta.clone();
            }
            MapFile::Layer(layer) => {
                let i = layer.index();
                self.layers[i] = fresh.layers[i].clone();
                self.origin.layers[i] = fresh.layers[i].clone();
                self.changed[i] = 0;
            }
        }
        self.origin.files[slot(file)] = fresh.origin.files[slot(file)].clone();
    }
}

fn blank(meta: &MapMeta) -> Result<TileLayer, glaux_machine::tilemap::TileLayerError> {
    TileLayer::empty(meta.width, meta.height).ok_or(
        glaux_machine::tilemap::TileLayerError::TooLarge {
            width: meta.width,
            height: meta.height,
        },
    )
}

#[cfg(test)]
mod tests;
