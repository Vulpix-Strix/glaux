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
use crate::cartridge_fs::{EntryKind, FsError, ReplaceError};

/// Version of the `map.ron` format. It evolves apart from the manifest's.
pub const MAP_FORMAT: u32 = 1;
/// Largest width or height of a map in tiles, a limit of the G1 format.
pub const MAP_MAX_SIZE: usize = 256;

const MAPS_DIR: &str = "/maps";
const MAX_ID_LEN: usize = 32;

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

impl MapMeta {
    /// Parses and validates a `map.ron`.
    pub fn parse(bytes: &[u8]) -> Result<Self, MapError> {
        let probe: FormatProbe = ron::de::from_bytes(bytes).map_err(ron_error)?;
        if probe.format != MAP_FORMAT {
            return Err(MapError::UnsupportedFormat(probe.format));
        }
        let file: MetaFile = ron::de::from_bytes(bytes).map_err(ron_error)?;
        if file.name.trim().is_empty() {
            return Err(MapError::BlankName);
        }
        let valid = |size: u32| (1..=MAP_MAX_SIZE as u32).contains(&size);
        if !valid(file.width) || !valid(file.height) {
            return Err(MapError::InvalidSize {
                width: file.width,
                height: file.height,
            });
        }
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

impl LoadedCartridge {
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
