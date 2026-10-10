//! Development cartridges. A directory holding a `cartridge.ron` manifest
//! and the entry script it names, plus optional binary assets: `sprites.bin`,
//! `objattr.bin`, `obp.bin` and `palette.bin`.
//!
//! Loading only validates the cartridge; its entry script runs only when
//! asked to, through [`LoadedCartridge::entry_source`].

use std::sync::OnceLock;

use glaux_modules::{MODULE_ROOT, ModuleLoadError, ModulePath, ModuleSource};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::cartridge_fs::{CartridgeFs, CartridgeStorage, FsError, ReplaceError};

/// Path of the manifest from the cartridge root.
const MANIFEST: &str = "/cartridge.ron";

/// Path of the manifest used before `cartridge.ron`. It is never read; its
/// presence only turns a missing-manifest error into a migration hint.
const LEGACY_MANIFEST: &str = "/cart.json";

/// The only manifest format this Glaux understands.
pub(crate) const FORMAT: u32 = 1;

/// Optional native object-tile data from the cartridge root.
const OBJ_TILES: &str = "/sprites.bin";

/// Optional per-tile object attributes from the cartridge root.
const OBJ_ATTRIBUTES: &str = "/objattr.bin";

/// Optional object palette banks from the cartridge root.
const OBJ_PALETTES: &str = "/obp.bin";

/// Optional RGB888 master palette from the cartridge root.
const MASTER_PALETTE: &str = "/palette.bin";

/// Suffix an entry script's file name must have.
const ENTRY_EXTENSION: &str = ".luau";

/// A cartridge that failed to load. Messages name virtual or
/// cartridge-relative paths only, so they are safe to show in the shell.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CartridgeError {
    /// The cartridge directory itself could not be reached.
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("No cartridge.ron in {0}")]
    MissingManifest(String),
    /// The directory only has the retired `cart.json` manifest.
    #[error("cart.json is no longer supported; use cartridge.ron")]
    LegacyManifest,
    #[error("cartridge.ron is not a file")]
    ManifestNotAFile,
    #[error("Cannot read cartridge.ron")]
    ManifestUnreadable,
    /// Syntax or shape errors reported by the RON parser.
    #[error("cartridge.ron line {line}: {reason}")]
    Malformed { line: usize, reason: String },
    #[error("cartridge.ron: unsupported format {0}")]
    UnsupportedFormat(u32),
    /// A required field is absent; holds its dotted path, as in
    /// `cartridge.version`.
    #[error("cartridge.ron: missing {0}")]
    MissingField(String),
    /// A field the v1 schema does not define; holds its dotted path.
    #[error("cartridge.ron: unknown field {0}")]
    UnknownField(String),
    #[error("cartridge.ron: unsupported language {0}")]
    UnsupportedLanguage(String),
    #[error("cartridge.ron: unsupported hardware model {0}")]
    UnsupportedModel(String),
    #[error("cartridge.ron: unsupported graphics profile {0}")]
    UnsupportedGraphics(String),
    #[error("cartridge.ron: cartridge.name is empty")]
    EmptyName,
    #[error("cartridge.ron: cartridge.version is not semver")]
    InvalidVersion,
    #[error("cartridge.ron: compatibility.min_glaux is not semver")]
    InvalidMinGlaux,
    #[error("Cartridge requires Glaux {required} (this is {running})")]
    RequiresGlaux { required: Version, running: Version },
    #[error("cartridge.ron: runtime.entry must be a relative path")]
    InvalidEntryPath,
    #[error("cartridge.ron: runtime.entry must be a .luau file")]
    EntryNotLuau,
    #[error("Entry not found: {0}")]
    EntryNotFound(String),
    #[error("Entry is not a file: {0}")]
    EntryNotAFile(String),
    #[error("Cannot read entry: {0}")]
    EntryUnreadable(String),
    #[error("sprites.bin is not a file")]
    ObjTilesNotAFile,
    #[error("Cannot read sprites.bin")]
    ObjTilesUnreadable,
    #[error("Cannot write sprites.bin")]
    ObjTilesUnwritable,
    #[error("sprites.bin is read-only")]
    ObjTilesReadOnly,
    #[error("objattr.bin is not a file")]
    ObjAttributesNotAFile,
    #[error("Cannot read objattr.bin")]
    ObjAttributesUnreadable,
    #[error("obp.bin is not a file")]
    ObjPalettesNotAFile,
    #[error("Cannot read obp.bin")]
    ObjPalettesUnreadable,
    #[error("palette.bin is not a file")]
    PaletteNotAFile,
    #[error("Cannot read palette.bin")]
    PaletteUnreadable,
}

/// The validated contents of `cartridge.ron`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeManifest {
    /// Version of the manifest/package format, not of the game. Always
    /// [`FORMAT`] once validated.
    pub format: u32,
    pub cartridge: CartridgeInfo,
    pub runtime: RuntimeSpec,
    pub hardware: HardwareSpec,
    pub compatibility: Compatibility,
}

/// The game's own identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeInfo {
    /// Display name; never empty or whitespace-only.
    pub name: String,
    /// The game's version, independent of the manifest format.
    pub version: Version,
    pub author: Option<String>,
}

/// How the cartridge's code runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSpec {
    pub language: Language,
    /// Path of the entry script from the cartridge root: `/`-separated plain
    /// names ending in a `.luau` file name.
    pub entry: String,
}

/// The machine the cartridge targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HardwareSpec {
    pub model: HardwareModel,
    pub graphics: GraphicsProfile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compatibility {
    /// Oldest Glaux release able to run the cartridge.
    pub min_glaux: Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    Luau,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HardwareModel {
    G1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphicsProfile {
    /// G1 with 4 bpp tiles.
    #[serde(rename = "G1_4bpp")]
    G1Bpp4,
}

/// The v1 document as written. Versions stay strings here so an invalid one
/// is reported against its own field instead of as an anonymous serde error.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestFile {
    pub(crate) format: u32,
    pub(crate) cartridge: CartridgeFile,
    pub(crate) runtime: RuntimeSpec,
    pub(crate) hardware: HardwareSpec,
    pub(crate) compatibility: CompatibilityFile,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CartridgeFile {
    pub(crate) name: String,
    pub(crate) version: String,
    /// Left out of written manifests when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) author: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompatibilityFile {
    pub(crate) min_glaux: String,
}

/// Reads only `format`, so a manifest from another format version is
/// reported as such rather than as a mismatch against the v1 schema.
#[derive(Deserialize)]
struct FormatProbe {
    format: u32,
}

impl CartridgeManifest {
    /// Parses and validates a v1 manifest.
    pub fn parse(bytes: &[u8]) -> Result<Self, CartridgeError> {
        let probe: FormatProbe = ron::de::from_bytes(bytes).map_err(manifest_error)?;
        if probe.format != FORMAT {
            return Err(CartridgeError::UnsupportedFormat(probe.format));
        }
        let file: ManifestFile = ron::de::from_bytes(bytes).map_err(manifest_error)?;
        file.try_into()
    }

    /// Fails if the cartridge needs a Glaux newer than `glaux`.
    pub fn check_compatible(&self, glaux: &Version) -> Result<(), CartridgeError> {
        let required = &self.compatibility.min_glaux;
        if required > glaux {
            return Err(CartridgeError::RequiresGlaux {
                required: required.clone(),
                running: glaux.clone(),
            });
        }
        Ok(())
    }
}

impl TryFrom<ManifestFile> for CartridgeManifest {
    type Error = CartridgeError;

    fn try_from(file: ManifestFile) -> Result<Self, CartridgeError> {
        if file.cartridge.name.trim().is_empty() {
            return Err(CartridgeError::EmptyName);
        }
        let version =
            Version::parse(&file.cartridge.version).map_err(|_| CartridgeError::InvalidVersion)?;
        let min_glaux = Version::parse(&file.compatibility.min_glaux)
            .map_err(|_| CartridgeError::InvalidMinGlaux)?;
        validate_entry(&file.runtime.entry)?;
        Ok(Self {
            format: file.format,
            cartridge: CartridgeInfo {
                name: file.cartridge.name,
                version,
                author: file.cartridge.author,
            },
            runtime: file.runtime,
            hardware: file.hardware,
            compatibility: Compatibility { min_glaux },
        })
    }
}

/// Translates a RON error into a Glaux one. Schema violations surface through
/// serde's standard `missing_field`, `unknown_field` and `unknown_variant`
/// errors; `outer` names the Rust type being deserialized, which maps back to
/// a manifest section.
fn manifest_error(err: ron::error::SpannedError) -> CartridgeError {
    use ron::Error;

    match err.code {
        Error::MissingStructField { field, outer } => {
            CartridgeError::MissingField(field_path(outer.as_deref(), field))
        }
        Error::NoSuchStructField { found, outer, .. } => {
            CartridgeError::UnknownField(field_path(outer.as_deref(), &found))
        }
        Error::NoSuchEnumVariant {
            found,
            outer: Some(outer),
            ..
        } if outer == "Language" => CartridgeError::UnsupportedLanguage(found),
        Error::NoSuchEnumVariant {
            found,
            outer: Some(outer),
            ..
        } if outer == "HardwareModel" => CartridgeError::UnsupportedModel(found),
        Error::NoSuchEnumVariant {
            found,
            outer: Some(outer),
            ..
        } if outer == "GraphicsProfile" => CartridgeError::UnsupportedGraphics(found),
        code => CartridgeError::Malformed {
            line: err.span.start.line,
            reason: code.to_string(),
        },
    }
}

/// The dotted manifest path of `field` inside the struct named `outer`.
fn field_path(outer: Option<&str>, field: &str) -> String {
    let section = match outer {
        Some("CartridgeFile") => "cartridge",
        Some("RuntimeSpec") => "runtime",
        Some("HardwareSpec") => "hardware",
        Some("CompatibilityFile") => "compatibility",
        _ => return field.to_owned(),
    };
    format!("{section}.{field}")
}

/// Version of the running Glaux, the one `min_glaux` is checked against.
pub(crate) fn glaux_version() -> &'static Version {
    static VERSION: OnceLock<Version> = OnceLock::new();
    VERSION.get_or_init(|| {
        Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver")
    })
}

/// Checks that `entry` is a portable relative path to a `.luau` file.
///
/// Components are separated by `/` only and must be plain names: absolute
/// paths, empty components, `.` and `..` are rejected outright, even where
/// `..` would stay inside the cartridge. `\` and `:` are rejected as a
/// portability rule: a cartridge must name the same file on every host, and
/// those characters are separators or drive and stream markers on some of
/// them.
fn validate_entry(entry: &str) -> Result<(), CartridgeError> {
    let portable = !entry.contains(['\\', ':', '\0'])
        && entry
            .split('/')
            .all(|component| !matches!(component, "" | "." | ".."));
    if !portable {
        return Err(CartridgeError::InvalidEntryPath);
    }
    let file_name = entry.rsplit('/').next().unwrap_or(entry);
    match file_name.strip_suffix(ENTRY_EXTENSION) {
        Some(stem) if !stem.is_empty() => Ok(()),
        _ => Err(CartridgeError::EntryNotLuau),
    }
}

/// A cartridge whose manifest and entry script have been validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedCartridge {
    manifest: CartridgeManifest,
    /// The cartridge directory as a filesystem of its own. It is never
    /// navigated, so its current directory stays at the cartridge root and
    /// every path in the cartridge is relative to that root.
    files: CartridgeFs,
}

impl LoadedCartridge {
    /// Loads the cartridge in the directory at `path`, absolute or relative
    /// to the current directory of `storage`.
    ///
    /// The manifest and the entry script are looked up only inside the
    /// cartridge directory: symlinks leading out of it count as missing.
    /// A cartridge requiring a newer Glaux than this one is rejected.
    pub fn load(storage: &CartridgeStorage, path: &str) -> Result<Self, CartridgeError> {
        let (location, files) = storage.subdir(path)?;
        Self::open(location, files)
    }

    /// Validates the cartridge whose directory is `files`, which `location`
    /// names in errors and logs. What [`load`](Self::load) does once the
    /// directory is located.
    pub(crate) fn open(location: String, files: CartridgeFs) -> Result<Self, CartridgeError> {
        let manifest = match files.read_file(MANIFEST) {
            Ok(bytes) => CartridgeManifest::parse(&bytes)?,
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => {
                if files.ensure_file(LEGACY_MANIFEST).is_ok() {
                    return Err(CartridgeError::LegacyManifest);
                }
                return Err(CartridgeError::MissingManifest(location));
            }
            Err(FsError::NotAFile(_)) => return Err(CartridgeError::ManifestNotAFile),
            Err(
                FsError::Unreadable(_)
                | FsError::AlreadyExists(_)
                | FsError::Unwritable(_)
                | FsError::ReadOnly(_)
                | FsError::Unavailable,
            ) => {
                return Err(CartridgeError::ManifestUnreadable);
            }
        };

        manifest.check_compatible(glaux_version())?;

        let entry = &manifest.runtime.entry;
        files
            .ensure_file(&format!("/{entry}"))
            .map_err(|err| entry_error(err, entry))?;

        tracing::info!(
            cartridge = %location,
            name = %manifest.cartridge.name,
            version = %manifest.cartridge.version,
            "cartridge loaded"
        );
        Ok(Self { manifest, files })
    }

    pub fn manifest(&self) -> &CartridgeManifest {
        &self.manifest
    }

    /// Current contents of the entry script. The file is read on every call,
    /// so edits made after loading are picked up.
    pub fn entry_source(&self) -> Result<Vec<u8>, CartridgeError> {
        let entry = &self.manifest.runtime.entry;
        self.files
            .read_file(&format!("/{entry}"))
            .map_err(|err| entry_error(err, entry))
    }

    /// The cartridge's code modules, for the runtime's `require`.
    pub fn modules(&self) -> CartridgeModules {
        CartridgeModules {
            files: self.files.clone(),
        }
    }

    /// Current native object-tile bytes, if the optional `sprites.bin` exists.
    ///
    /// Like every optional asset, the file is read on every call so a fresh
    /// cartridge run observes edits made since the cartridge was loaded.
    /// Format validation belongs to the machine representation constructed
    /// by the caller.
    pub fn read_obj_tiles(&self) -> Result<Option<Vec<u8>>, CartridgeError> {
        self.read_optional(
            OBJ_TILES,
            CartridgeError::ObjTilesNotAFile,
            CartridgeError::ObjTilesUnreadable,
        )
    }

    /// Current per-tile object attribute bytes, if `objattr.bin` exists.
    pub fn read_obj_attributes(&self) -> Result<Option<Vec<u8>>, CartridgeError> {
        self.read_optional(
            OBJ_ATTRIBUTES,
            CartridgeError::ObjAttributesNotAFile,
            CartridgeError::ObjAttributesUnreadable,
        )
    }

    /// Current object palette bank bytes, if `obp.bin` exists.
    pub fn read_obj_palettes(&self) -> Result<Option<Vec<u8>>, CartridgeError> {
        self.read_optional(
            OBJ_PALETTES,
            CartridgeError::ObjPalettesNotAFile,
            CartridgeError::ObjPalettesUnreadable,
        )
    }

    /// Current RGB888 master palette bytes, if `palette.bin` exists.
    pub fn read_master_palette(&self) -> Result<Option<Vec<u8>>, CartridgeError> {
        self.read_optional(
            MASTER_PALETTE,
            CartridgeError::PaletteNotAFile,
            CartridgeError::PaletteUnreadable,
        )
    }

    /// Replaces `sprites.bin` with `bytes`, provided it still holds
    /// `expected` (`None`: provided it is absent). See
    /// [`CartridgeFs::replace_file`]: the file is replaced whole or not at
    /// all, and a read-only file is never replaced.
    pub fn replace_obj_tiles(
        &self,
        bytes: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<(), ObjTilesWriteError> {
        self.files
            .replace_file(OBJ_TILES, bytes, expected)
            .map_err(|err| match err {
                ReplaceError::Changed(current) => ObjTilesWriteError::Changed(current),
                ReplaceError::Fs(FsError::ReadOnly(_)) => {
                    ObjTilesWriteError::Failed(CartridgeError::ObjTilesReadOnly)
                }
                ReplaceError::Fs(FsError::NotAFile(_)) => {
                    ObjTilesWriteError::Failed(CartridgeError::ObjTilesNotAFile)
                }
                ReplaceError::Fs(FsError::Unreadable(_)) => {
                    ObjTilesWriteError::Failed(CartridgeError::ObjTilesUnreadable)
                }
                ReplaceError::Fs(
                    FsError::NotFound(_)
                    | FsError::NotADirectory(_)
                    | FsError::AlreadyExists(_)
                    | FsError::Unwritable(_)
                    | FsError::Unavailable,
                ) => ObjTilesWriteError::Failed(CartridgeError::ObjTilesUnwritable),
            })
    }

    /// Reads an optional root file: `None` when it does not exist.
    fn read_optional(
        &self,
        path: &str,
        not_a_file: CartridgeError,
        unreadable: CartridgeError,
    ) -> Result<Option<Vec<u8>>, CartridgeError> {
        match self.files.read_file(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => Ok(None),
            Err(FsError::NotAFile(_)) => Err(not_a_file),
            Err(
                FsError::Unreadable(_)
                | FsError::AlreadyExists(_)
                | FsError::Unwritable(_)
                | FsError::ReadOnly(_)
                | FsError::Unavailable,
            ) => Err(unreadable),
        }
    }
}

/// A failed [`LoadedCartridge::replace_obj_tiles`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjTilesWriteError {
    /// The file could not be checked or written.
    Failed(CartridgeError),
    /// `sprites.bin` no longer holds what the caller expected. Holds what
    /// it holds now, `None` when it is absent.
    Changed(Option<Vec<u8>>),
}

/// Module files of one cartridge, confined to its real `code/` directory.
///
/// Each load resolves `code/` again, so a run sees the files as they are
/// when required. `code/` itself must not be a symlink, and links inside it
/// are followed only to targets inside it: a module cannot resolve to
/// another file of the cartridge, such as its manifest or assets, nor to
/// anything outside the cartridge.
#[derive(Debug, Clone)]
pub struct CartridgeModules {
    files: CartridgeFs,
}

impl ModuleSource for CartridgeModules {
    fn load(&self, path: &ModulePath) -> Result<Vec<u8>, ModuleLoadError> {
        let read = self
            .files
            .real_subdir(MODULE_ROOT)
            .and_then(|code| code.read_file(path.in_root()));
        read.map_err(|err| match err {
            FsError::NotFound(_) | FsError::NotADirectory(_) => ModuleLoadError::NotFound,
            FsError::NotAFile(_)
            | FsError::Unreadable(_)
            | FsError::AlreadyExists(_)
            | FsError::Unwritable(_)
            | FsError::ReadOnly(_)
            | FsError::Unavailable => ModuleLoadError::Unreadable,
        })
    }
}

/// The entry-script error for a failed lookup of `entry`.
fn entry_error(err: FsError, entry: &str) -> CartridgeError {
    match err {
        FsError::NotFound(_) | FsError::NotADirectory(_) => {
            CartridgeError::EntryNotFound(entry.to_owned())
        }
        FsError::NotAFile(_) => CartridgeError::EntryNotAFile(entry.to_owned()),
        FsError::Unreadable(_)
        | FsError::AlreadyExists(_)
        | FsError::Unwritable(_)
        | FsError::ReadOnly(_)
        | FsError::Unavailable => CartridgeError::EntryUnreadable(entry.to_owned()),
    }
}

#[cfg(test)]
pub(crate) mod fixture;

#[cfg(test)]
mod tests;
