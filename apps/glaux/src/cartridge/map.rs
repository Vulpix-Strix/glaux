//! Maps of a cartridge
//!
//! A map is a directory under `maps/` named by its [`MapId`]. `map.ron`
//! holds the display name, the dimensions and optional layer names. Each
//! `bgN.bin` holds one background layer as little-endian 16-bit entries,
//! row-major, exactly `width * height * 2` bytes. A layer without a file is
//! entirely empty.

use glaux_machine::tilemap::{BG_LAYERS, BgLayer, TileLayerError};
use serde::{Deserialize, Serialize};

use super::LoadedCartridge;
use crate::cartridge_fs::{CreateDirError, EntryKind, FsError, ReplaceError};

/// Version of the `map.ron` format. It evolves apart from the manifest's.
pub const MAP_FORMAT: u32 = 1;
/// Largest width or height of a map in tiles, a limit of the G1 format.
pub const MAP_MAX_SIZE: usize = 256;

const MAPS_DIR: &str = "/maps";
/// Longest map id, in characters.
pub const MAX_ID_LEN: usize = 32;

/// The name of a map's directory, and its identity within a cartridge.
///
/// Lowercase ASCII letters, digits, `-` and `_`, between 1 and 32
/// characters, starting with a letter or digit. It never changes when the
/// map's display name does.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MapId(String);

/// A rejected [`MapId`]. Holds the text as given.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "Invalid map id {0:?}: use 1 to {MAX_ID_LEN} of a-z, 0-9, - and _, starting with a letter or digit"
)]
pub struct MapIdError(String);

impl MapIdError {
    pub fn id(&self) -> &str {
        &self.0
    }
}

impl MapId {
    pub fn parse(id: &str) -> Result<Self, MapIdError> {
        let valid_char = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_".contains(c);
        let valid = (1..=MAX_ID_LEN).contains(&id.len())
            && id.chars().all(valid_char)
            && !id.starts_with(['-', '_']);
        if valid {
            Ok(Self(id.to_owned()))
        } else {
            Err(MapIdError(id.to_owned()))
        }
    }
}

impl std::fmt::Display for MapId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One file of a map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapFile {
    Meta,
    Layer(BgLayer),
}

impl MapFile {
    /// `map.ron` followed by the three layers.
    pub const ALL: [Self; 1 + BG_LAYERS] = [
        Self::Meta,
        Self::Layer(BgLayer::Bg0),
        Self::Layer(BgLayer::Bg1),
        Self::Layer(BgLayer::Bg2),
    ];

    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Meta => "map.ron",
            Self::Layer(BgLayer::Bg0) => "bg0.bin",
            Self::Layer(BgLayer::Bg1) => "bg1.bin",
            Self::Layer(BgLayer::Bg2) => "bg2.bin",
        }
    }

    fn path(self, id: &MapId) -> String {
        format!("{MAPS_DIR}/{id}/{}", self.file_name())
    }
}

impl std::fmt::Display for MapFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.file_name())
    }
}

/// The editable, validated contents of `map.ron`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapMeta {
    /// Display name, never blank.
    pub name: String,
    pub width: usize,
    pub height: usize,
    /// Optional descriptive names, never blank when present.
    pub layer_names: [Option<String>; BG_LAYERS],
}

/// A failure to read a map.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MapError {
    #[error("map.ron line {line}: {reason}")]
    Malformed { line: usize, reason: String },
    #[error("map.ron: unsupported format {0}")]
    UnsupportedFormat(u32),
    /// Holds the dotted path of the field, as in `layers.bg0`.
    #[error("map.ron: missing {0}")]
    MissingField(String),
    #[error("map.ron: unknown field {0}")]
    UnknownField(String),
    #[error("map.ron: name is blank")]
    BlankName,
    #[error("map.ron: the name of {0} is blank")]
    BlankLayerName(BgLayer),
    #[error("map.ron: size {width}x{height} is outside 1 to {MAP_MAX_SIZE} in each direction")]
    InvalidSize { width: u32, height: u32 },
    #[error("{layer}: {source}")]
    Layer {
        layer: BgLayer,
        source: TileLayerError,
    },
    #[error("Map {0} has no map.ron")]
    MissingMeta(MapId),
    #[error("{file} of map {id} is not a file")]
    NotAFile { id: MapId, file: MapFile },
    #[error(transparent)]
    Fs(#[from] FsError),
}

#[derive(Deserialize)]
struct FormatProbe {
    format: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MetaFile {
    format: u32,
    name: String,
    width: u32,
    height: u32,
    #[serde(default)]
    layers: LayerNamesFile,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerNamesFile {
    #[serde(default)]
    bg0: Option<String>,
    #[serde(default)]
    bg1: Option<String>,
    #[serde(default)]
    bg2: Option<String>,
}

/// The rules every map obeys, whether read from a `map.ron` or created.
fn check_name_and_size(name: &str, width: usize, height: usize) -> Result<(), MapError> {
    if name.trim().is_empty() {
        return Err(MapError::BlankName);
    }
    let valid = |size: usize| (1..=MAP_MAX_SIZE).contains(&size);
    if !valid(width) || !valid(height) {
        let saturate = |size: usize| u32::try_from(size).unwrap_or(u32::MAX);
        return Err(MapError::InvalidSize {
            width: saturate(width),
            height: saturate(height),
        });
    }
    Ok(())
}

impl MapMeta {
    /// Metadata for a new map without layer names.
    pub fn new(name: &str, width: usize, height: usize) -> Result<Self, MapError> {
        check_name_and_size(name, width, height)?;
        Ok(Self {
            name: name.to_owned(),
            width,
            height,
            layer_names: [None, None, None],
        })
    }

    /// Parses and validates a `map.ron`.
    pub fn parse(bytes: &[u8]) -> Result<Self, MapError> {
        let probe: FormatProbe = ron::de::from_bytes(bytes).map_err(ron_error)?;
        if probe.format != MAP_FORMAT {
            return Err(MapError::UnsupportedFormat(probe.format));
        }
        let file: MetaFile = ron::de::from_bytes(bytes).map_err(ron_error)?;
        check_name_and_size(&file.name, file.width as usize, file.height as usize)?;
        let layer_names = [file.layers.bg0, file.layers.bg1, file.layers.bg2];
        for (layer, name) in BgLayer::ALL.into_iter().zip(&layer_names) {
            if name.as_ref().is_some_and(|name| name.trim().is_empty()) {
                return Err(MapError::BlankLayerName(layer));
            }
        }
        Ok(Self {
            name: file.name,
            width: file.width as usize,
            height: file.height as usize,
            layer_names,
        })
    }

    /// The `map.ron` text for this metadata. Always the same bytes for the
    /// same value.
    ///
    /// # Panics
    ///
    /// If the dimensions do not fit 32 bits, which [`MapMeta::parse`] never
    /// produces.
    pub fn to_bytes(&self) -> Vec<u8> {
        let [bg0, bg1, bg2] = self.layer_names.clone();
        let file = MetaFile {
            format: MAP_FORMAT,
            name: self.name.clone(),
            width: u32::try_from(self.width).expect("map width fits 32 bits"),
            height: u32::try_from(self.height).expect("map height fits 32 bits"),
            layers: LayerNamesFile { bg0, bg1, bg2 },
        };
        let config = ron::ser::PrettyConfig::default();
        let mut text = ron::ser::to_string_pretty(&file, config).expect("map metadata serializes");
        text.push('\n');
        text.into_bytes()
    }
}

fn ron_error(err: ron::error::SpannedError) -> MapError {
    use ron::Error;

    match err.code {
        Error::MissingStructField { field, outer } => {
            MapError::MissingField(field_path(outer.as_deref(), field))
        }
        Error::NoSuchStructField { found, outer, .. } => {
            MapError::UnknownField(field_path(outer.as_deref(), &found))
        }
        code => MapError::Malformed {
            line: err.span.start.line,
            reason: code.to_string(),
        },
    }
}

fn field_path(outer: Option<&str>, field: &str) -> String {
    match outer {
        Some("LayerNamesFile") => format!("layers.{field}"),
        _ => field.to_owned(),
    }
}

/// The bytes of a map's files, as read. Nothing here is validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapFiles {
    pub meta: Vec<u8>,
    /// `None` is a layer without a file, which is entirely empty.
    pub layers: [Option<Vec<u8>>; BG_LAYERS],
}

/// A failed write to a map file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapWriteError {
    /// The file could not be checked or written.
    Failed(FsError),
    /// The file no longer holds what the caller expected. Holds what it
    /// holds now, `None` when it is absent.
    Changed(Option<Vec<u8>>),
}

/// A failed [`LoadedCartridge::create_map`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MapCreateError {
    #[error("Map {0} already exists")]
    AlreadyExists(MapId),
    #[error(transparent)]
    Fs(FsError),
}

/// A failed [`LoadedCartridge::delete_map`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MapDeleteError {
    #[error("Map {0} does not exist")]
    Missing(MapId),
    /// A file no longer holds what the caller saw. Nothing was deleted.
    #[error("{file} of map {id} changed outside Glaux")]
    Changed {
        id: MapId,
        file: MapFile,
        /// What the file holds now, `None` when it is absent.
        current: Option<Vec<u8>>,
    },
    /// The directory holds entries that are not files of the map. Nothing
    /// was deleted.
    #[error("Map {id} holds other files: {}", .names.join(", "))]
    UnknownEntries { id: MapId, names: Vec<String> },
    /// Reading the map failed before anything was deleted.
    #[error(transparent)]
    Map(#[from] MapError),
    /// Deleting began and then failed, so part of the map may be gone.
    #[error("Map {id} is only partly deleted: {source}")]
    Incomplete { id: MapId, source: FsError },
}

/// One map of a cartridge, as the map list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSummary {
    pub id: MapId,
    /// Why the map cannot be opened, when its `map.ron` is missing or invalid.
    pub meta: Result<MapMeta, MapError>,
}

impl LoadedCartridge {
    /// The maps of the cartridge with their metadata, in id order. A map
    /// whose `map.ron` cannot be read stays in the list with its error.
    pub fn map_summaries(&self) -> Result<Vec<MapSummary>, FsError> {
        Ok(self
            .list_maps()?
            .into_iter()
            .map(|id| {
                let meta = match self.read_map_file(&id, MapFile::Meta) {
                    Ok(Some(bytes)) => MapMeta::parse(&bytes),
                    Ok(None) => Err(MapError::MissingMeta(id.clone())),
                    Err(err) => Err(err),
                };
                MapSummary { id, meta }
            })
            .collect())
    }

    /// Creates map `id` holding only its `map.ron`, since a layer without a
    /// file is empty. Nothing is ever replaced: an existing map of that id,
    /// or any other entry of that name under `maps/`, is
    /// [`MapCreateError::AlreadyExists`].
    pub fn create_map(&self, id: &MapId, meta: &MapMeta) -> Result<(), MapCreateError> {
        let mut maps = self.files.clone();
        maps.create_dirs(MAPS_DIR).map_err(MapCreateError::Fs)?;
        maps.change_dir(MAPS_DIR).map_err(MapCreateError::Fs)?;
        let bytes = meta.to_bytes();
        let files = [(MapFile::Meta.file_name(), bytes.as_slice())];
        maps.create_dir_with(&id.to_string(), &files, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .map(drop)
        .map_err(|err| match err {
            CreateDirError::Fs(FsError::AlreadyExists(_)) => {
                MapCreateError::AlreadyExists(id.clone())
            }
            CreateDirError::Fs(err) => MapCreateError::Fs(err),
            CreateDirError::Invalid(never) => match never {},
        })
    }

    /// Deletes map `id`, provided its files still hold what `seen` read.
    ///
    /// Everything is checked before anything is deleted: the directory must
    /// hold only the files of a map, each must match `seen` exactly (a layer
    /// that was absent must still be absent), and none may be a file the
    /// removal would refuse, such as a read-only one. Any problem leaves the
    /// map untouched. Only then are the files and the directory removed.
    ///
    /// No portable primitive makes the check and the removal one step, so a
    /// write landing in between is deleted with the map, and a failure that
    /// could not be known beforehand, such as a directory that cannot be
    /// modified, comes after the first removal as
    /// [`MapDeleteError::Incomplete`].
    pub fn delete_map(&self, id: &MapId, seen: &MapFiles) -> Result<(), MapDeleteError> {
        let dir = format!("{MAPS_DIR}/{id}");
        let names = match self.files.entry_names(&dir) {
            Ok(names) => names,
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => {
                return Err(MapDeleteError::Missing(id.clone()));
            }
            Err(err) => return Err(MapError::from(err).into()),
        };
        let unknown: Vec<String> = names
            .into_iter()
            .filter(|name| MapFile::ALL.iter().all(|file| file.file_name() != name))
            .collect();
        if !unknown.is_empty() {
            return Err(MapDeleteError::UnknownEntries {
                id: id.clone(),
                names: unknown,
            });
        }

        let expected = |file: MapFile| match file {
            MapFile::Meta => Some(seen.meta.as_slice()),
            MapFile::Layer(layer) => seen.layers[layer.index()].as_deref(),
        };
        for file in MapFile::ALL {
            let current = self.read_map_file(id, file)?;
            if current.as_deref() != expected(file) {
                return Err(MapDeleteError::Changed {
                    id: id.clone(),
                    file,
                    current,
                });
            }
            // Refusals that are known now are reported now, so they cannot
            // strand a half-deleted map.
            if expected(file).is_some() {
                self.files
                    .ensure_removable_file(&file.path(id))
                    .map_err(MapError::from)?;
            }
        }

        let incomplete = |source| MapDeleteError::Incomplete {
            id: id.clone(),
            source,
        };
        // `map.ron` goes last, so a map interrupted midway still shows up
        // in the list instead of vanishing half-deleted.
        for file in MapFile::ALL.into_iter().rev() {
            if expected(file).is_some() {
                self.files.remove_file(&file.path(id)).map_err(incomplete)?;
            }
        }
        self.files.remove_dir(&dir).map_err(incomplete)
    }

    /// The maps of the cartridge, in id order. A cartridge without a
    /// `maps/` directory has none. Directories whose names are not valid
    /// ids are left out. Only names are read, never map contents.
    pub fn list_maps(&self) -> Result<Vec<MapId>, FsError> {
        let entries = match self.files.list(Some(MAPS_DIR)) {
            Ok(entries) => entries,
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };
        let mut ids: Vec<MapId> = entries
            .iter()
            .filter(|entry| entry.kind == EntryKind::Directory)
            .filter_map(|entry| MapId::parse(&entry.name).ok())
            .collect();
        ids.sort();
        Ok(ids)
    }

    /// Current bytes of one file of map `id`, read on every call. `None`
    /// when the file is absent.
    pub fn read_map_file(&self, id: &MapId, file: MapFile) -> Result<Option<Vec<u8>>, MapError> {
        match self.files.read_file(&file.path(id)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => Ok(None),
            Err(FsError::NotAFile(_)) => Err(MapError::NotAFile {
                id: id.clone(),
                file,
            }),
            Err(err) => Err(err.into()),
        }
    }

    /// All files of map `id`. `map.ron` must exist, layers need not.
    pub fn read_map(&self, id: &MapId) -> Result<MapFiles, MapError> {
        let meta = self
            .read_map_file(id, MapFile::Meta)?
            .ok_or_else(|| MapError::MissingMeta(id.clone()))?;
        let mut layers = [None, None, None];
        for layer in BgLayer::ALL {
            layers[layer.index()] = self.read_map_file(id, MapFile::Layer(layer))?;
        }
        Ok(MapFiles { meta, layers })
    }

    /// Replaces one file of map `id` with `bytes`, provided it still holds
    /// `expected`, where `None` means the file must be absent. See
    /// [`CartridgeFs::replace_file`]. The map's directory must exist.
    ///
    /// [`CartridgeFs::replace_file`]: crate::cartridge_fs::CartridgeFs::replace_file
    pub fn replace_map_file(
        &self,
        id: &MapId,
        file: MapFile,
        bytes: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<(), MapWriteError> {
        self.files
            .replace_file(&file.path(id), bytes, expected)
            .map_err(|err| match err {
                ReplaceError::Changed(current) => MapWriteError::Changed(current),
                ReplaceError::Fs(err) => MapWriteError::Failed(err),
            })
    }

    /// Deletes one file of map `id`, provided it still holds exactly
    /// `expected`. A file that is already gone is [`MapWriteError::Changed`]
    /// with `None`, since the caller expected it to be there.
    ///
    /// No portable primitive compares and unlinks in one step, so a write
    /// landing between the comparison and the removal is deleted with the
    /// file. See [`CartridgeFs::remove_file`].
    ///
    /// [`CartridgeFs::remove_file`]: crate::cartridge_fs::CartridgeFs::remove_file
    pub fn remove_map_file(
        &self,
        id: &MapId,
        file: MapFile,
        expected: &[u8],
    ) -> Result<(), MapWriteError> {
        let path = file.path(id);
        let current = match self.files.read_file(&path) {
            Ok(bytes) => bytes,
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => {
                return Err(MapWriteError::Changed(None));
            }
            Err(err) => return Err(MapWriteError::Failed(err)),
        };
        if current != expected {
            return Err(MapWriteError::Changed(Some(current)));
        }
        match self.files.remove_file(&path) {
            Ok(()) => Ok(()),
            Err(FsError::NotFound(_)) => Err(MapWriteError::Changed(None)),
            Err(err) => Err(MapWriteError::Failed(err)),
        }
    }
}

#[cfg(test)]
mod tests;
