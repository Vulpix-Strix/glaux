//! Development cartridges. A directory holding a `cartridge.ron` manifest
//! and the entry script it names, plus optional binary assets such as
//! `sprites.bin`.
//!
//! Loading only validates the cartridge; its entry script runs only when
//! asked to, through [`LoadedCartridge::entry_source`].

use std::sync::OnceLock;

use glaux_modules::{MODULE_ROOT, ModuleLoadError, ModulePath, ModuleSource};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::cartridge_fs::{CartridgeFs, CartridgeStorage, FsError};

/// Path of the manifest from the cartridge root.
const MANIFEST: &str = "/cartridge.ron";

/// Path of the manifest used before `cartridge.ron`. It is never read; its
/// presence only turns a missing-manifest error into a migration hint.
const LEGACY_MANIFEST: &str = "/cart.json";

/// The only manifest format this Glaux understands.
const FORMAT: u32 = 1;

/// Optional native object-tile data from the cartridge root.
const OBJ_TILES: &str = "/sprites.bin";

/// Suffix an entry script's file name must have.
const ENTRY_EXTENSION: &str = ".luau";

/// A cartridge that failed to load. Messages name virtual or
/// cartridge-relative paths only, so they are safe to show in the shell.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CartridgeError {
    /// The cartridge directory itself could not be reached.
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("NO CARTRIDGE.RON IN {0}")]
    MissingManifest(String),
    /// The directory only has the retired `cart.json` manifest.
    #[error("CART.JSON IS NO LONGER SUPPORTED; USE CARTRIDGE.RON")]
    LegacyManifest,
    #[error("CARTRIDGE.RON IS NOT A FILE")]
    ManifestNotAFile,
    #[error("CANNOT READ CARTRIDGE.RON")]
    ManifestUnreadable,
    /// Syntax or shape errors reported by the RON parser.
    #[error("CARTRIDGE.RON LINE {line}: {reason}")]
    Malformed { line: usize, reason: String },
    #[error("CARTRIDGE.RON: UNSUPPORTED FORMAT {0}")]
    UnsupportedFormat(u32),
    /// A required field is absent; holds its dotted path, as in
    /// `cartridge.version`.
    #[error("CARTRIDGE.RON: MISSING {0}")]
    MissingField(String),
    /// A field the v1 schema does not define; holds its dotted path.
    #[error("CARTRIDGE.RON: UNKNOWN FIELD {0}")]
    UnknownField(String),
    #[error("CARTRIDGE.RON: UNSUPPORTED LANGUAGE {0}")]
    UnsupportedLanguage(String),
    #[error("CARTRIDGE.RON: UNSUPPORTED HARDWARE MODEL {0}")]
    UnsupportedModel(String),
    #[error("CARTRIDGE.RON: UNSUPPORTED GRAPHICS PROFILE {0}")]
    UnsupportedGraphics(String),
    #[error("CARTRIDGE.RON: cartridge.name IS EMPTY")]
    EmptyName,
    #[error("CARTRIDGE.RON: cartridge.version IS NOT SEMVER")]
    InvalidVersion,
    #[error("CARTRIDGE.RON: compatibility.min_glaux IS NOT SEMVER")]
    InvalidMinGlaux,
    #[error("CARTRIDGE REQUIRES GLAUX {required} (THIS IS {running})")]
    RequiresGlaux { required: Version, running: Version },
    #[error("CARTRIDGE.RON: runtime.entry MUST BE A RELATIVE PATH")]
    InvalidEntryPath,
    #[error("CARTRIDGE.RON: runtime.entry MUST BE A .luau FILE")]
    EntryNotLuau,
    #[error("ENTRY NOT FOUND: {0}")]
    EntryNotFound(String),
    #[error("ENTRY IS NOT A FILE: {0}")]
    EntryNotAFile(String),
    #[error("CANNOT READ ENTRY: {0}")]
    EntryUnreadable(String),
    #[error("SPRITES.BIN IS NOT A FILE")]
    ObjTilesNotAFile,
    #[error("CANNOT READ SPRITES.BIN")]
    ObjTilesUnreadable,
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
struct ManifestFile {
    format: u32,
    cartridge: CartridgeFile,
    runtime: RuntimeSpec,
    hardware: HardwareSpec,
    compatibility: CompatibilityFile,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CartridgeFile {
    name: String,
    version: String,
    author: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompatibilityFile {
    min_glaux: String,
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
fn glaux_version() -> &'static Version {
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
        let manifest = match files.read_file(MANIFEST) {
            Ok(bytes) => CartridgeManifest::parse(&bytes)?,
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => {
                if files.ensure_file(LEGACY_MANIFEST).is_ok() {
                    return Err(CartridgeError::LegacyManifest);
                }
                return Err(CartridgeError::MissingManifest(location));
            }
            Err(FsError::NotAFile(_)) => return Err(CartridgeError::ManifestNotAFile),
            Err(FsError::Unreadable(_) | FsError::Unavailable) => {
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
    /// The file is read on every call so a fresh cartridge run observes edits
    /// made since the cartridge was loaded. Format validation belongs to the
    /// machine representation constructed by the caller.
    pub fn read_obj_tiles(&self) -> Result<Option<Vec<u8>>, CartridgeError> {
        match self.files.read_file(OBJ_TILES) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => Ok(None),
            Err(FsError::NotAFile(_)) => Err(CartridgeError::ObjTilesNotAFile),
            Err(FsError::Unreadable(_) | FsError::Unavailable) => {
                Err(CartridgeError::ObjTilesUnreadable)
            }
        }
    }
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
            FsError::NotAFile(_) | FsError::Unreadable(_) | FsError::Unavailable => {
                ModuleLoadError::Unreadable
            }
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
        FsError::Unreadable(_) | FsError::Unavailable => {
            CartridgeError::EntryUnreadable(entry.to_owned())
        }
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    use std::fs;
    use std::path::Path;

    /// A v1 manifest for a cartridge named `name` whose entry is `entry`.
    pub(crate) fn manifest_ron(name: &str, entry: &str) -> String {
        format!(
            r#"(
    format: 1,
    cartridge: (name: "{name}", version: "0.1.0", author: None),
    runtime: (language: Luau, entry: "{entry}"),
    hardware: (model: G1, graphics: G1_4bpp),
    compatibility: (min_glaux: "0.1.0"),
)"#
        )
    }

    /// Writes a cartridge directory at `dir` named `name`, version `0.1.0`,
    /// with an empty entry at `code/main.luau`.
    pub(crate) fn write_cartridge(dir: &Path, name: &str) {
        write_manifest(dir, &manifest_ron(name, "code/main.luau"));
        fs::create_dir_all(dir.join("code")).unwrap();
        fs::write(dir.join("code/main.luau"), "").unwrap();
    }

    /// Writes `cartridge.ron` with `ron` into `dir`, creating `dir` if needed.
    pub(crate) fn write_manifest(dir: &Path, ron: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("cartridge.ron"), ron).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::fixture::{manifest_ron, write_cartridge, write_manifest};
    use super::*;
    use crate::cartridge_fs::fixture::Fixture;
    use pretty_assertions::assert_eq;

    const MOCHI: &str = r#"(
    format: 1,

    cartridge: (
        name: "Mochi Adventure",
        version: "0.1.0",
        author: Some("cuervolu"),
    ),

    runtime: (
        language: Luau,
        entry: "code/main.luau",
    ),

    hardware: (
        model: G1,
        graphics: G1_4bpp,
    ),

    compatibility: (
        min_glaux: "0.1.0",
    ),
)"#;

    fn manifest(ron: &str) -> Result<CartridgeManifest, CartridgeError> {
        CartridgeManifest::parse(ron.as_bytes())
    }

    /// `MOCHI` with the first occurrence of `from` replaced by `to`.
    fn mochi_with(from: &str, to: &str) -> Result<CartridgeManifest, CartridgeError> {
        assert!(MOCHI.contains(from), "{from:?} not in the template");
        manifest(&MOCHI.replacen(from, to, 1))
    }

    fn with_entry(entry: &str) -> Result<CartridgeManifest, CartridgeError> {
        manifest(&manifest_ron("Pong", entry))
    }

    #[test]
    fn parses_the_specified_v1_manifest() {
        assert_eq!(
            manifest(MOCHI),
            Ok(CartridgeManifest {
                format: 1,
                cartridge: CartridgeInfo {
                    name: "Mochi Adventure".into(),
                    version: Version::new(0, 1, 0),
                    author: Some("cuervolu".into()),
                },
                runtime: RuntimeSpec {
                    language: Language::Luau,
                    entry: "code/main.luau".into(),
                },
                hardware: HardwareSpec {
                    model: HardwareModel::G1,
                    graphics: GraphicsProfile::G1Bpp4,
                },
                compatibility: Compatibility {
                    min_glaux: Version::new(0, 1, 0),
                },
            })
        );
    }

    #[test]
    fn author_is_optional() {
        let author = |m: CartridgeManifest| m.cartridge.author;
        assert_eq!(
            mochi_with(r#"author: Some("cuervolu"),"#, "author: None,").map(author),
            Ok(None)
        );
        assert_eq!(
            mochi_with(r#"author: Some("cuervolu"),"#, "").map(author),
            Ok(None)
        );
    }

    #[test]
    fn comments_and_trailing_commas_are_plain_ron() {
        let commented = r#"// Mochi
            (
                format: 1, /* manifest format, not the game */
                cartridge: (name: "Mochi", version: "1.2.3-beta.1",),
                runtime: (language: Luau, entry: "code/main.luau",), // entry
                hardware: (model: G1, graphics: G1_4bpp,),
                compatibility: (min_glaux: "0.1.0",),
            )
        "#;
        let parsed = manifest(commented).unwrap();
        assert_eq!(
            parsed.cartridge.version,
            Version::parse("1.2.3-beta.1").unwrap()
        );
        assert_eq!(parsed.runtime.entry, "code/main.luau");
    }

    #[test]
    fn malformed_ron_reports_the_line() {
        assert_eq!(
            manifest("(\n    format: 1,\n    cartridge: ("),
            Err(CartridgeError::Malformed {
                line: 3,
                reason: "Unexpected end of RON".into(),
            })
        );
        let json = r#"{"name": "Pong", "version": "0.1.0", "entry": "main.luau"}"#;
        assert!(matches!(
            manifest(json),
            Err(CartridgeError::Malformed { line: 1, .. })
        ));
        assert!(matches!(
            CartridgeManifest::parse(b"\xff"),
            Err(CartridgeError::Malformed { line: 1, .. })
        ));
        assert_eq!(
            mochi_with("model: G1,", "model: G1"),
            Err(CartridgeError::Malformed {
                line: 17,
                reason: "Expected comma".into(),
            })
        );
        let shown = CartridgeError::Malformed {
            line: 3,
            reason: "Expected closing `)`".into(),
        };
        assert_eq!(
            shown.to_string(),
            "CARTRIDGE.RON LINE 3: Expected closing `)`"
        );
    }

    #[test]
    fn only_format_1_is_supported() {
        assert_eq!(
            mochi_with("format: 1,", "format: 2,"),
            Err(CartridgeError::UnsupportedFormat(2))
        );
        // Another format may have another shape: the format is still what
        // gets reported.
        assert_eq!(
            manifest("(format: 0, title: \"Pong\")"),
            Err(CartridgeError::UnsupportedFormat(0))
        );
        assert_eq!(
            mochi_with("format: 1,", ""),
            Err(CartridgeError::MissingField("format".into()))
        );
        assert_eq!(
            CartridgeError::UnsupportedFormat(2).to_string(),
            "CARTRIDGE.RON: UNSUPPORTED FORMAT 2"
        );
    }

    #[test]
    fn missing_and_unknown_fields_name_their_section() {
        assert_eq!(
            mochi_with(r#"version: "0.1.0","#, ""),
            Err(CartridgeError::MissingField("cartridge.version".into()))
        );
        assert_eq!(
            mochi_with(r#"entry: "code/main.luau","#, ""),
            Err(CartridgeError::MissingField("runtime.entry".into()))
        );
        assert_eq!(
            mochi_with("graphics: G1_4bpp,", ""),
            Err(CartridgeError::MissingField("hardware.graphics".into()))
        );
        assert_eq!(
            mochi_with(
                r#"author: Some("cuervolu"),"#,
                r#"auther: Some("cuervolu"),"#
            ),
            Err(CartridgeError::UnknownField("cartridge.auther".into()))
        );
        assert_eq!(
            mochi_with("format: 1,", "format: 1, api: 2,"),
            Err(CartridgeError::UnknownField("api".into()))
        );
        assert_eq!(
            CartridgeError::MissingField("runtime.entry".into()).to_string(),
            "CARTRIDGE.RON: MISSING runtime.entry"
        );
    }

    #[test]
    fn unsupported_enum_values_are_glaux_errors() {
        assert_eq!(
            mochi_with("language: Luau", "language: Rhai"),
            Err(CartridgeError::UnsupportedLanguage("Rhai".into()))
        );
        assert_eq!(
            mochi_with("model: G1", "model: G2"),
            Err(CartridgeError::UnsupportedModel("G2".into()))
        );
        assert_eq!(
            mochi_with("graphics: G1_4bpp", "graphics: G1_2bpp"),
            Err(CartridgeError::UnsupportedGraphics("G1_2bpp".into()))
        );
        assert_eq!(
            CartridgeError::UnsupportedLanguage("Rhai".into()).to_string(),
            "CARTRIDGE.RON: UNSUPPORTED LANGUAGE Rhai"
        );
    }

    #[test]
    fn name_must_not_be_blank() {
        for name in ["", "   ", "\t\n"] {
            assert_eq!(
                manifest(&manifest_ron(name, "main.luau")),
                Err(CartridgeError::EmptyName),
                "{name:?}"
            );
        }
        assert_eq!(
            manifest(&manifest_ron(" Pong ", "main.luau")).map(|m| m.cartridge.name),
            Ok(" Pong ".into())
        );
    }

    #[test]
    fn versions_must_be_semver() {
        for version in ["", "1", "1.0", "v1.0.0", "1.0.0.0", "01.0.0"] {
            assert_eq!(
                mochi_with(r#"version: "0.1.0""#, &format!("version: {version:?}")),
                Err(CartridgeError::InvalidVersion),
                "{version:?}"
            );
            assert_eq!(
                mochi_with(r#"min_glaux: "0.1.0""#, &format!("min_glaux: {version:?}")),
                Err(CartridgeError::InvalidMinGlaux),
                "{version:?}"
            );
        }
    }

    #[test]
    fn min_glaux_must_not_exceed_the_running_glaux() {
        let parsed = mochi_with(r#"min_glaux: "0.1.0""#, r#"min_glaux: "0.2.0""#).unwrap();
        assert_eq!(parsed.check_compatible(&Version::new(0, 2, 0)), Ok(()));
        assert_eq!(parsed.check_compatible(&Version::new(1, 0, 0)), Ok(()));
        assert_eq!(
            parsed.check_compatible(&Version::new(0, 1, 9)),
            Err(CartridgeError::RequiresGlaux {
                required: Version::new(0, 2, 0),
                running: Version::new(0, 1, 9),
            })
        );
        assert_eq!(
            parsed
                .check_compatible(&Version::new(0, 1, 9))
                .unwrap_err()
                .to_string(),
            "CARTRIDGE REQUIRES GLAUX 0.2.0 (THIS IS 0.1.9)"
        );
    }

    #[test]
    fn entry_must_be_a_portable_relative_path() {
        for entry in [
            "",
            "/main.luau",
            "//main.luau",
            "/code/main.luau",
            "../main.luau",
            "code/../../main.luau",
            "a/../main.luau",
            "src/..",
            "./main.luau",
            "a/./main.luau",
            "a//main.luau",
            "src/",
            r"src\main.luau",
            r"\main.luau",
            "C:main.luau",
            "C:/main.luau",
            "main.luau:stream",
            "a\0.luau",
        ] {
            assert_eq!(
                with_entry(&entry.escape_default().to_string()),
                Err(CartridgeError::InvalidEntryPath),
                "{entry:?}"
            );
        }
    }

    #[test]
    fn entry_must_be_a_luau_file() {
        for entry in [
            "main.lua",
            "main.LUAU",
            "main",
            ".luau",
            "code/.luau",
            "main.luau.txt",
        ] {
            assert_eq!(
                with_entry(entry),
                Err(CartridgeError::EntryNotLuau),
                "{entry:?}"
            );
        }
        for entry in [
            "main.luau",
            "code/main.luau",
            "code/game/main.luau",
            "a.b.luau",
            "..luau",
        ] {
            assert_eq!(with_entry(entry).map(|m| m.runtime.entry), Ok(entry.into()));
        }
    }

    fn load(storage: &CartridgeStorage, path: &str) -> Result<CartridgeManifest, CartridgeError> {
        LoadedCartridge::load(storage, path).map(|loaded| loaded.manifest().clone())
    }

    #[test]
    fn loads_relative_to_the_current_directory_or_root() {
        let fixture = Fixture::new("load-ok");
        write_cartridge(&fixture.root().join("games/pong"), "Pong");
        let mut storage = fixture.storage();

        let pong = load(&storage, "/games/pong").unwrap();
        assert_eq!(pong.cartridge.name, "Pong");
        assert_eq!(pong.cartridge.version, Version::new(0, 1, 0));
        assert_eq!(pong.runtime.entry, "code/main.luau");
        storage.change_dir("games/owl").unwrap();
        assert_eq!(load(&storage, "../pong"), Ok(pong.clone()));
        assert_eq!(load(&storage, "/games/./owl/../pong/"), Ok(pong));
        assert_eq!(storage.cwd(), Ok("/games/owl".into()), "load does not move");
    }

    #[test]
    fn entry_may_live_at_the_root_or_nested_under_code() {
        let fixture = Fixture::new("load-entry-dirs");
        for entry in ["main.luau", "code/main.luau", "code/game/main.luau"] {
            let dir = fixture.root().join("pong");
            let _ = fs::remove_dir_all(&dir);
            write_manifest(&dir, &manifest_ron("Pong", entry));
            fs::create_dir_all(dir.join(entry).parent().unwrap()).unwrap();
            fs::write(dir.join(entry), "").unwrap();
            assert_eq!(
                load(&fixture.storage(), "pong").map(|m| m.runtime.entry),
                Ok(entry.into())
            );
        }
    }

    #[test]
    fn a_cartridge_for_a_newer_glaux_does_not_load() {
        let fixture = Fixture::new("load-min-glaux");
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        let newer = manifest_ron("Pong", "code/main.luau")
            .replace(r#"min_glaux: "0.1.0""#, r#"min_glaux: "99.0.0""#);
        write_manifest(&dir, &newer);
        assert!(matches!(
            load(&fixture.storage(), "pong"),
            Err(CartridgeError::RequiresGlaux { required, .. }) if required == Version::new(99, 0, 0)
        ));
    }

    #[test]
    fn entry_source_reads_the_current_entry() {
        let fixture = Fixture::new("load-entry-source");
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        let entry = dir.join("code/main.luau");
        fs::write(&entry, "print(1)").unwrap();
        let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
        assert_eq!(cartridge.entry_source(), Ok(b"print(1)".to_vec()));

        fs::write(&entry, "print(2)").unwrap();
        assert_eq!(cartridge.entry_source(), Ok(b"print(2)".to_vec()));

        fs::remove_file(&entry).unwrap();
        assert_eq!(
            cartridge.entry_source(),
            Err(CartridgeError::EntryNotFound("code/main.luau".into()))
        );
    }

    /// Loads module `name` of `cartridge` as the Luau runtime would.
    fn load_module(cartridge: &LoadedCartridge, name: &str) -> Result<Vec<u8>, ModuleLoadError> {
        let path = glaux_modules::ModuleName::parse(name).unwrap().path("luau");
        cartridge.modules().load(&path)
    }

    #[test]
    fn modules_are_read_fresh_from_code() {
        let fixture = Fixture::new("modules-code");
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        fs::create_dir_all(dir.join("code/game")).unwrap();
        fs::write(dir.join("code/game/enemies.luau"), "return 1").unwrap();
        let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

        assert_eq!(
            load_module(&cartridge, "game.enemies"),
            Ok(b"return 1".to_vec())
        );
        assert_eq!(load_module(&cartridge, "main"), Ok(Vec::new()));
        fs::write(dir.join("code/game/enemies.luau"), "return 2").unwrap();
        assert_eq!(
            load_module(&cartridge, "game.enemies"),
            Ok(b"return 2".to_vec())
        );
        assert_eq!(
            load_module(&cartridge, "nope"),
            Err(ModuleLoadError::NotFound)
        );
        assert_eq!(
            load_module(&cartridge, "game"),
            Err(ModuleLoadError::NotFound)
        );
        fs::create_dir(dir.join("code/dir.luau")).unwrap();
        assert_eq!(
            load_module(&cartridge, "dir"),
            Err(ModuleLoadError::Unreadable)
        );
    }

    #[test]
    fn modules_cannot_name_files_outside_code() {
        let fixture = Fixture::new("modules-outside-code");
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        // Files a module path could only reach by leaving `code/`.
        fs::write(dir.join("secret.luau"), "").unwrap();
        fs::write(fixture.root().join("games/owl/main.luau"), "").unwrap();
        let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
        assert_eq!(
            load_module(&cartridge, "secret"),
            Err(ModuleLoadError::NotFound)
        );
    }

    #[test]
    fn a_cartridge_without_code_has_no_modules() {
        let fixture = Fixture::new("modules-no-code");
        let dir = fixture.root().join("pong");
        write_manifest(&dir, &manifest_ron("Pong", "main.luau"));
        fs::write(dir.join("main.luau"), "").unwrap();
        let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
        assert_eq!(
            load_module(&cartridge, "main"),
            Err(ModuleLoadError::NotFound)
        );

        fs::write(dir.join("code"), "").unwrap();
        assert_eq!(
            load_module(&cartridge, "main"),
            Err(ModuleLoadError::NotFound)
        );
    }

    #[test]
    fn one_cartridge_cannot_load_another_cartridges_modules() {
        let fixture = Fixture::new("modules-other-cartridge");
        let games = fixture.root().join("games");
        write_cartridge(&games.join("a"), "A");
        write_cartridge(&games.join("b"), "B");
        fs::write(games.join("b/code/only_b.luau"), "").unwrap();
        let a = LoadedCartridge::load(&fixture.storage(), "games/a").unwrap();
        let b = LoadedCartridge::load(&fixture.storage(), "games/b").unwrap();
        assert_eq!(load_module(&a, "only_b"), Err(ModuleLoadError::NotFound));
        assert_eq!(load_module(&b, "only_b"), Ok(Vec::new()));
    }

    #[test]
    fn object_tiles_are_optional_and_read_fresh() {
        let fixture = Fixture::new("load-obj-tiles");
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
        assert_eq!(cartridge.read_obj_tiles(), Ok(None));

        fs::write(dir.join("sprites.bin"), [0x12; 32]).unwrap();
        assert_eq!(cartridge.read_obj_tiles(), Ok(Some(vec![0x12; 32])));
        fs::write(dir.join("sprites.bin"), [0x34; 64]).unwrap();
        assert_eq!(cartridge.read_obj_tiles(), Ok(Some(vec![0x34; 64])));
    }

    #[test]
    fn object_tiles_must_be_a_regular_readable_file() {
        let fixture = Fixture::new("load-obj-tiles-file");
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        fs::create_dir(dir.join("sprites.bin")).unwrap();
        let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

        assert_eq!(
            cartridge.read_obj_tiles(),
            Err(CartridgeError::ObjTilesNotAFile)
        );
        assert_eq!(
            CartridgeError::ObjTilesNotAFile.to_string(),
            "SPRITES.BIN IS NOT A FILE"
        );
    }

    #[test]
    fn cartridge_directory_must_exist() {
        let fixture = Fixture::new("load-dir");
        let storage = fixture.storage();
        assert_eq!(
            load(&storage, "games/nope"),
            Err(CartridgeError::Fs(FsError::NotFound("/games/nope".into())))
        );
        assert_eq!(
            load(&storage, "b.txt"),
            Err(CartridgeError::Fs(FsError::NotADirectory("/b.txt".into())))
        );
        assert_eq!(
            load(&CartridgeStorage::Unavailable, "games"),
            Err(CartridgeError::Fs(FsError::Unavailable))
        );
        assert_eq!(
            CartridgeError::Fs(FsError::Unavailable).to_string(),
            "CARTRIDGE STORAGE UNAVAILABLE"
        );
    }

    #[test]
    fn manifest_must_be_a_file() {
        let fixture = Fixture::new("load-manifest");
        let storage = fixture.storage();
        assert_eq!(
            load(&storage, "games/owl"),
            Err(CartridgeError::MissingManifest("/games/owl".into()))
        );
        assert_eq!(
            CartridgeError::MissingManifest("/games/owl".into()).to_string(),
            "NO CARTRIDGE.RON IN /games/owl"
        );
        fs::create_dir(fixture.root().join("games/owl/cartridge.ron")).unwrap();
        assert_eq!(
            load(&storage, "games/owl"),
            Err(CartridgeError::ManifestNotAFile)
        );
    }

    #[test]
    fn a_legacy_cart_json_alone_is_rejected() {
        let fixture = Fixture::new("load-legacy");
        let dir = fixture.root().join("pong");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("cart.json"),
            r#"{"name": "Pong", "version": "0.1.0", "entry": "main.luau"}"#,
        )
        .unwrap();
        fs::write(dir.join("main.luau"), "").unwrap();
        assert_eq!(
            load(&fixture.storage(), "pong"),
            Err(CartridgeError::LegacyManifest)
        );

        // A leftover cart.json is ignored once cartridge.ron exists.
        write_cartridge(&dir, "Pong");
        assert!(load(&fixture.storage(), "pong").is_ok());
    }

    #[test]
    fn manifest_errors_surface() {
        let fixture = Fixture::new("load-bad-manifest");
        let dir = fixture.root().join("games/owl");
        write_manifest(&dir, "(");
        assert!(matches!(
            load(&fixture.storage(), "games/owl"),
            Err(CartridgeError::Malformed { .. })
        ));
        write_manifest(
            &dir,
            &manifest_ron("Owl", "code/main.luau").replace(r#""0.1.0", author"#, r#""1", author"#),
        );
        assert_eq!(
            load(&fixture.storage(), "games/owl"),
            Err(CartridgeError::InvalidVersion)
        );
        write_manifest(&dir, &manifest_ron("Owl", "/code/main.luau"));
        assert_eq!(
            load(&fixture.storage(), "games/owl"),
            Err(CartridgeError::InvalidEntryPath)
        );
        write_manifest(&dir, &manifest_ron("Owl", "../main.luau"));
        assert_eq!(
            load(&fixture.storage(), "games/owl"),
            Err(CartridgeError::InvalidEntryPath)
        );
    }

    #[test]
    fn entry_must_exist_as_a_regular_file() {
        let fixture = Fixture::new("load-entry");
        let dir = fixture.root().join("games/owl");
        write_manifest(&dir, &manifest_ron("Owl", "code/main.luau"));
        let storage = fixture.storage();
        let missing = Err(CartridgeError::EntryNotFound("code/main.luau".into()));
        assert_eq!(load(&storage, "games/owl"), missing);

        // A file where a directory is expected.
        fs::write(dir.join("code"), "").unwrap();
        assert_eq!(load(&storage, "games/owl"), missing);

        fs::remove_file(dir.join("code")).unwrap();
        fs::create_dir_all(dir.join("code/main.luau")).unwrap();
        assert_eq!(
            load(&storage, "games/owl"),
            Err(CartridgeError::EntryNotAFile("code/main.luau".into()))
        );
        assert_eq!(
            CartridgeError::EntryNotAFile("code/main.luau".into()).to_string(),
            "ENTRY IS NOT A FILE: code/main.luau"
        );
    }

    #[test]
    fn shipped_examples_load_with_their_assets() {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let root = fs::canonicalize(examples).unwrap();
        let storage = CartridgeStorage::Available(CartridgeFs::new(root));
        for example in ["sprites", "tile-viewer"] {
            let cartridge = LoadedCartridge::load(&storage, example).unwrap();
            assert_eq!(
                cartridge.manifest().runtime.entry,
                "code/main.luau",
                "{example}"
            );
            assert!(!cartridge.entry_source().unwrap().is_empty(), "{example}");
            assert!(cartridge.read_obj_tiles().unwrap().is_some(), "{example}");
        }
    }

    #[cfg(unix)]
    mod symlinks {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::os::unix::fs::symlink;

        fn entry_not_found() -> Result<CartridgeManifest, CartridgeError> {
            Err(CartridgeError::EntryNotFound("main.luau".into()))
        }

        #[test]
        fn entry_links_leaving_the_cartridge_are_not_found() {
            let fixture = Fixture::new("load-link-entry");
            let root = fixture.root();
            write_cartridge(&root.join("games/other"), "Other");
            fs::write(fixture.outside().join("main.luau"), "").unwrap();
            let pong = root.join("games/pong");
            write_manifest(&pong, &manifest_ron("Pong", "main.luau"));
            let storage = fixture.storage();

            let targets = [
                // Outside storage, absolute and relative.
                fixture.outside().join("main.luau"),
                "../../../../outside/main.luau".into(),
                // Inside storage, but in a sibling cartridge.
                root.join("games/other/code/main.luau"),
                "../other/code/main.luau".into(),
            ];
            for target in targets {
                let _ = fs::remove_file(pong.join("main.luau"));
                symlink(&target, pong.join("main.luau")).unwrap();
                assert_eq!(
                    load(&storage, "games/pong"),
                    entry_not_found(),
                    "{target:?}"
                );
            }
        }

        #[test]
        fn entry_through_a_linked_directory_leaving_the_cartridge_is_not_found() {
            let fixture = Fixture::new("load-link-entry-dir");
            let root = fixture.root();
            write_cartridge(&root.join("games/other"), "Other");
            let pong = root.join("games/pong");
            write_manifest(&pong, &manifest_ron("Pong", "code/main.luau"));
            symlink("../other/code", pong.join("code")).unwrap();
            assert_eq!(
                load(&fixture.storage(), "games/pong"),
                Err(CartridgeError::EntryNotFound("code/main.luau".into()))
            );
        }

        #[test]
        fn manifest_links_leaving_the_cartridge_are_missing() {
            let fixture = Fixture::new("load-link-manifest");
            let root = fixture.root();
            write_cartridge(&root.join("games/other"), "Other");
            write_manifest(&fixture.outside(), &manifest_ron("X", "main.luau"));
            let pong = root.join("games/pong");
            fs::create_dir_all(&pong).unwrap();
            fs::write(pong.join("main.luau"), "").unwrap();
            let storage = fixture.storage();

            for target in [
                fixture.outside().join("cartridge.ron"),
                "../other/cartridge.ron".into(),
            ] {
                let _ = fs::remove_file(pong.join("cartridge.ron"));
                symlink(&target, pong.join("cartridge.ron")).unwrap();
                assert_eq!(
                    load(&storage, "games/pong"),
                    Err(CartridgeError::MissingManifest("/games/pong".into())),
                    "{target:?}"
                );
            }
        }

        #[test]
        fn links_inside_the_cartridge_are_followed() {
            let fixture = Fixture::new("load-link-inside");
            let pong = fixture.root().join("games/pong");
            write_manifest(&pong, &manifest_ron("Pong", "main.luau"));
            fs::create_dir_all(pong.join("code")).unwrap();
            fs::write(pong.join("code/game.luau"), "").unwrap();
            symlink("code/game.luau", pong.join("main.luau")).unwrap();
            assert!(load(&fixture.storage(), "games/pong").is_ok());
        }

        #[test]
        fn a_linked_cartridge_is_confined_to_its_target() {
            let fixture = Fixture::new("load-link-cartridge");
            let root = fixture.root();
            write_cartridge(&root.join("games/pong"), "Pong");
            symlink("games/pong", root.join("pong-link")).unwrap();
            let storage = fixture.storage();
            assert_eq!(
                load(&storage, "pong-link").map(|m| m.cartridge.name),
                Ok("Pong".into())
            );

            // From the target's point of view `games/` is outside, even though
            // the link that reached it sits at the storage root.
            let entry = root.join("games/pong/code/main.luau");
            fs::remove_file(&entry).unwrap();
            fs::write(root.join("main.luau"), "").unwrap();
            symlink("../../../main.luau", &entry).unwrap();
            assert_eq!(
                load(&storage, "pong-link"),
                Err(CartridgeError::EntryNotFound("code/main.luau".into()))
            );
        }

        #[test]
        fn module_links_must_stay_inside_code() {
            let fixture = Fixture::new("modules-links");
            let dir = fixture.root().join("pong");
            write_cartridge(&dir, "Pong");
            let code = dir.join("code");
            fs::create_dir_all(dir.join("assets")).unwrap();
            fs::write(dir.join("assets/x.luau"), "").unwrap();
            fs::write(code.join("real.luau"), "return 1").unwrap();
            symlink("real.luau", code.join("inner.luau")).unwrap();
            symlink("../assets/x.luau", code.join("asset.luau")).unwrap();
            symlink("../cartridge.ron", code.join("manifest.luau")).unwrap();
            symlink(dir.join("cartridge.ron"), code.join("manifest_abs.luau")).unwrap();
            fs::write(fixture.outside().join("out.luau"), "").unwrap();
            symlink(fixture.outside().join("out.luau"), code.join("out.luau")).unwrap();
            let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

            assert_eq!(load_module(&cartridge, "inner"), Ok(b"return 1".to_vec()));
            for name in ["asset", "manifest", "manifest_abs", "out"] {
                assert_eq!(
                    load_module(&cartridge, name),
                    Err(ModuleLoadError::NotFound),
                    "{name}"
                );
            }
        }

        #[test]
        fn a_linked_code_directory_has_no_modules() {
            let fixture = Fixture::new("modules-code-link");
            let games = fixture.root().join("games");
            write_cartridge(&games.join("a"), "A");
            write_cartridge(&games.join("b"), "B");
            let a = games.join("a");
            fs::rename(a.join("code"), a.join("src")).unwrap();
            symlink("../b/code", a.join("code")).unwrap();
            write_manifest(&a, &manifest_ron("A", "src/main.luau"));
            let cartridge = LoadedCartridge::load(&fixture.storage(), "games/a").unwrap();
            assert_eq!(
                load_module(&cartridge, "main"),
                Err(ModuleLoadError::NotFound)
            );

            fs::remove_file(a.join("code")).unwrap();
            symlink("src", a.join("code")).unwrap();
            assert_eq!(
                load_module(&cartridge, "main"),
                Err(ModuleLoadError::NotFound)
            );
        }

        #[test]
        fn cartridge_links_leaving_storage_are_not_found() {
            let fixture = Fixture::new("load-link-out");
            write_cartridge(&fixture.outside(), "Out");
            symlink(fixture.outside(), fixture.root().join("out")).unwrap();
            assert_eq!(
                load(&fixture.storage(), "out"),
                Err(CartridgeError::Fs(FsError::NotFound("/out".into())))
            );
        }
    }
}
