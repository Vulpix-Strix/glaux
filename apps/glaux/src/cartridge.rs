//! Development cartridges. A directory holding a `cartridge.ron` manifest
//! and the entry script it names, plus optional binary assets: `sprites.bin`,
//! `objattr.bin`, `obp.bin` and `palette.bin`.
//!
//! Loading only validates the cartridge. Its entry script runs only when
//! asked to, through [`LoadedCartridge::entry_source`].

use std::path::Path;
use std::sync::OnceLock;

use glaux_modules::{MODULE_ROOT, ModuleLoadError, ModulePath, ModuleSource};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::cartridge_fs::{CartridgeFs, CartridgeStorage, FsError, ReplaceError};

/// Path of the manifest from the cartridge root.
const MANIFEST: &str = "/cartridge.ron";

/// Path of the manifest used before `cartridge.ron`. It is never read. Its
/// presence only turns a missing-manifest error into a migration hint.
const LEGACY_MANIFEST: &str = "/cart.json";

/// The only manifest format this Glaux understands.
pub(crate) const FORMAT: u32 = 1;

/// One of a cartridge's optional video asset files, all in its root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VideoAsset {
    /// Native object-tile data, `sprites.bin`.
    ObjTiles,
    /// Per-tile object attributes, `objattr.bin`.
    ObjAttributes,
    /// Object palette banks, `obp.bin`.
    ObjPalettes,
    /// RGB888 master palette, `palette.bin`.
    MasterPalette,
}

impl VideoAsset {
    pub const ALL: [Self; 4] = [
        Self::ObjTiles,
        Self::ObjAttributes,
        Self::ObjPalettes,
        Self::MasterPalette,
    ];

    /// The asset's file name in the cartridge root.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::ObjTiles => "sprites.bin",
            Self::ObjAttributes => "objattr.bin",
            Self::ObjPalettes => "obp.bin",
            Self::MasterPalette => "palette.bin",
        }
    }

    /// Position of the asset in [`ALL`](Self::ALL).
    pub fn index(self) -> usize {
        self as usize
    }

    fn path(self) -> String {
        format!("/{}", self.file_name())
    }
}

impl std::fmt::Display for VideoAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.file_name())
    }
}

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
    #[error("cartridge.ron: runtime.entry must be a .rhai file")]
    EntryNotRhai,
    #[error("Entry not found: {0}")]
    EntryNotFound(String),
    #[error("Entry is not a file: {0}")]
    EntryNotAFile(String),
    #[error("Cannot read entry: {0}")]
    EntryUnreadable(String),
    #[error("{0} is not a file")]
    AssetNotAFile(VideoAsset),
    #[error("Cannot read {0}")]
    AssetUnreadable(VideoAsset),
    #[error("Cannot write {0}")]
    AssetUnwritable(VideoAsset),
    #[error("{0} is read-only")]
    AssetReadOnly(VideoAsset),
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
    /// names ending in a file name with the language's
    /// [extension](Language::extension).
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
    /// Experimental: its scripting API may change without notice.
    Rhai,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::Luau, Self::Rhai];

    /// Extension of the language's script files, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Luau => "luau",
            Self::Rhai => "rhai",
        }
    }

    /// The lowercase name the shell accepts for the language.
    pub fn name(self) -> &'static str {
        match self {
            Self::Luau => "luau",
            Self::Rhai => "rhai",
        }
    }

    /// The language named `name`, ignoring case.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.name().eq_ignore_ascii_case(name))
    }

    /// Whether the language's scripting API may still change without
    /// notice.
    pub fn is_experimental(self) -> bool {
        matches!(self, Self::Rhai)
    }
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
        validate_entry(&file.runtime.entry, file.runtime.language)?;
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
/// errors. `outer` names the Rust type being deserialized, which maps back to
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

/// Checks that `entry` is a portable relative path to a script file of
/// `language`. See [`is_portable_path`].
fn validate_entry(entry: &str, language: Language) -> Result<(), CartridgeError> {
    if !is_portable_path(entry) {
        return Err(CartridgeError::InvalidEntryPath);
    }
    if is_script_of(entry, language) {
        Ok(())
    } else {
        Err(match language {
            Language::Luau => CartridgeError::EntryNotLuau,
            Language::Rhai => CartridgeError::EntryNotRhai,
        })
    }
}

/// Whether `path` is a portable path relative to the cartridge root.
///
/// Components are separated by `/` only and must be plain names: absolute
/// paths, empty components, `.` and `..` are rejected outright, even where
/// `..` would stay inside the cartridge. `\` and `:` are rejected as a
/// portability rule: a cartridge must name the same file on every host, and
/// those characters are separators or drive and stream markers on some of
/// them.
fn is_portable_path(path: &str) -> bool {
    !path.contains(['\\', ':', '\0'])
        && path
            .split('/')
            .all(|component| !matches!(component, "" | "." | ".."))
}

/// Whether the last component of `path` is a non-empty stem followed by
/// `language`'s extension, compared case-sensitively.
fn is_script_of(path: &str, language: Language) -> bool {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    file_name
        .strip_suffix(language.extension())
        .and_then(|rest| rest.strip_suffix('.'))
        .is_some_and(|stem| !stem.is_empty())
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
    #[hotpath::measure(label = "cartridge::load", impl_type = "LoadedCartridge")]
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

    /// The cartridge directory's host path, for host file dialogs to open
    /// in. See [`CartridgeFs::host_root`].
    pub fn host_dir(&self) -> &Path {
        self.files.host_root()
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

    /// Current bytes of the optional video asset file `asset`, or `None`
    /// when it does not exist.
    ///
    /// The file is read on every call, so a fresh cartridge run observes
    /// edits made since the cartridge was loaded. Format validation belongs
    /// to the machine representation constructed by the caller.
    pub fn read_asset(&self, asset: VideoAsset) -> Result<Option<Vec<u8>>, CartridgeError> {
        match self.files.read_file(&asset.path()) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => Ok(None),
            Err(FsError::NotAFile(_)) => Err(CartridgeError::AssetNotAFile(asset)),
            Err(
                FsError::Unreadable(_)
                | FsError::AlreadyExists(_)
                | FsError::Unwritable(_)
                | FsError::ReadOnly(_)
                | FsError::Unavailable,
            ) => Err(CartridgeError::AssetUnreadable(asset)),
        }
    }

    /// Replaces the file of `asset` with `bytes`, provided it still holds
    /// `expected`, where `None` means the file must be absent. See
    /// [`CartridgeFs::replace_file`]. The file is replaced whole or not at
    /// all, and a read-only file is never replaced.
    pub fn replace_asset(
        &self,
        asset: VideoAsset,
        bytes: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<(), AssetWriteError> {
        self.files
            .replace_file(&asset.path(), bytes, expected)
            .map_err(|err| match err {
                ReplaceError::Changed(current) => AssetWriteError::Changed(current),
                ReplaceError::Fs(FsError::ReadOnly(_)) => {
                    AssetWriteError::Failed(CartridgeError::AssetReadOnly(asset))
                }
                ReplaceError::Fs(FsError::NotAFile(_)) => {
                    AssetWriteError::Failed(CartridgeError::AssetNotAFile(asset))
                }
                ReplaceError::Fs(FsError::Unreadable(_)) => {
                    AssetWriteError::Failed(CartridgeError::AssetUnreadable(asset))
                }
                ReplaceError::Fs(
                    FsError::NotFound(_)
                    | FsError::NotADirectory(_)
                    | FsError::AlreadyExists(_)
                    | FsError::Unwritable(_)
                    | FsError::Unavailable,
                ) => AssetWriteError::Failed(CartridgeError::AssetUnwritable(asset)),
            })
    }
}

/// A failed [`LoadedCartridge::replace_asset`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetWriteError {
    /// The file could not be checked or written.
    Failed(CartridgeError),
    /// The file no longer holds what the caller expected. Holds what it
    /// holds now, `None` when it is absent.
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

pub(crate) mod source;

#[cfg(test)]
pub(crate) mod fixture;

#[cfg(test)]
mod tests;
