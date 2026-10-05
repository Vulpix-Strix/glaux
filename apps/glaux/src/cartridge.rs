//! Development cartridges. A directory holding a `cart.json` manifest and
//! the entry script it names.
//!
//! Loading only validates the cartridge; its entry script runs only when
//! asked to, through [`LoadedCartridge::entry_source`].

use semver::Version;
use serde_json::{Map, Value};

use crate::cartridge_fs::{CartridgeFs, CartridgeStorage, FsError};

/// Path of the manifest from the cartridge root.
const MANIFEST: &str = "/cart.json";

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
    #[error("NO CART.JSON IN {0}")]
    MissingManifest(String),
    #[error("CART.JSON IS NOT A FILE")]
    ManifestNotAFile,
    #[error("CANNOT READ CART.JSON")]
    ManifestUnreadable,
    #[error("CART.JSON IS NOT VALID JSON (LINE {line})")]
    InvalidJson { line: usize },
    #[error("CART.JSON MUST BE AN OBJECT")]
    NotAnObject,
    #[error("CART.JSON: MISSING {0}")]
    MissingField(&'static str),
    #[error("CART.JSON: {0} MUST BE A STRING")]
    NotAString(&'static str),
    #[error("CART.JSON: name IS EMPTY")]
    EmptyName,
    #[error("CART.JSON: version IS NOT SEMVER")]
    InvalidVersion,
    #[error("CART.JSON: entry MUST BE A RELATIVE PATH")]
    InvalidEntryPath,
    #[error("CART.JSON: entry MUST BE A .luau FILE")]
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

/// The validated contents of `cart.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeManifest {
    /// Display name; never empty or whitespace-only.
    pub name: String,
    pub version: Version,
    /// Path of the entry script from the cartridge root: `/`-separated plain
    /// names ending in a `.luau` file name.
    pub entry: String,
}

impl CartridgeManifest {
    /// Parses and validates a manifest. Fields other than `name`, `version`
    /// and `entry` are ignored.
    pub fn parse(bytes: &[u8]) -> Result<Self, CartridgeError> {
        Self::validate(&structure(bytes)?)
    }

    /// Checks the manifest fields of an already well-formed JSON object.
    fn validate(fields: &Map<String, Value>) -> Result<Self, CartridgeError> {
        let name = string_field(fields, "name")?;
        if name.trim().is_empty() {
            return Err(CartridgeError::EmptyName);
        }
        let version = Version::parse(string_field(fields, "version")?)
            .map_err(|_| CartridgeError::InvalidVersion)?;
        let entry = string_field(fields, "entry")?;
        validate_entry(entry)?;
        Ok(Self {
            name: name.to_owned(),
            version,
            entry: entry.to_owned(),
        })
    }
}

/// Parses `bytes` as JSON and requires an object at the top level.
fn structure(bytes: &[u8]) -> Result<Map<String, Value>, CartridgeError> {
    match serde_json::from_slice(bytes) {
        Ok(Value::Object(fields)) => Ok(fields),
        Ok(_) => Err(CartridgeError::NotAnObject),
        Err(err) => Err(CartridgeError::InvalidJson { line: err.line() }),
    }
}

fn string_field<'a>(
    fields: &'a Map<String, Value>,
    name: &'static str,
) -> Result<&'a str, CartridgeError> {
    match fields.get(name) {
        Some(Value::String(value)) => Ok(value),
        Some(_) => Err(CartridgeError::NotAString(name)),
        None => Err(CartridgeError::MissingField(name)),
    }
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
    pub fn load(storage: &CartridgeStorage, path: &str) -> Result<Self, CartridgeError> {
        let (location, files) = storage.subdir(path)?;
        let manifest = match files.read_file(MANIFEST) {
            Ok(bytes) => CartridgeManifest::parse(&bytes)?,
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => {
                return Err(CartridgeError::MissingManifest(location));
            }
            Err(FsError::NotAFile(_)) => return Err(CartridgeError::ManifestNotAFile),
            Err(FsError::Unreadable(_) | FsError::Unavailable) => {
                return Err(CartridgeError::ManifestUnreadable);
            }
        };

        let entry = &manifest.entry;
        files
            .ensure_file(&format!("/{entry}"))
            .map_err(|err| entry_error(err, entry))?;

        tracing::info!(
            cartridge = %location,
            name = %manifest.name,
            version = %manifest.version,
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
        let entry = &self.manifest.entry;
        self.files
            .read_file(&format!("/{entry}"))
            .map_err(|err| entry_error(err, entry))
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

    /// Writes a cartridge directory at `dir` named `name`, version `0.1.0`,
    /// with its entry at `main.luau`.
    pub(crate) fn write_cartridge(dir: &Path, name: &str) {
        write_manifest(
            dir,
            &format!(r#"{{"name": "{name}", "version": "0.1.0", "entry": "main.luau"}}"#),
        );
        fs::write(dir.join("main.luau"), "").unwrap();
    }

    /// Writes `cart.json` with `json` into `dir`, creating `dir` if needed.
    pub(crate) fn write_manifest(dir: &Path, json: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("cart.json"), json).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::fixture::{write_cartridge, write_manifest};
    use super::*;
    use crate::cartridge_fs::fixture::Fixture;
    use pretty_assertions::assert_eq;

    fn manifest(json: &str) -> Result<CartridgeManifest, CartridgeError> {
        CartridgeManifest::parse(json.as_bytes())
    }

    fn with_entry(entry: &str) -> Result<CartridgeManifest, CartridgeError> {
        manifest(
            &serde_json::json!({ "name": "Pong", "version": "0.1.0", "entry": entry }).to_string(),
        )
    }

    #[test]
    fn parses_a_valid_manifest() {
        assert_eq!(
            manifest(r#"{"name": "Pong", "version": "0.1.0-rc.1", "entry": "src/main.luau"}"#),
            Ok(CartridgeManifest {
                name: "Pong".into(),
                version: Version::parse("0.1.0-rc.1").unwrap(),
                entry: "src/main.luau".into(),
            })
        );
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let parsed = manifest(
            r#"{"name": "Pong", "version": "1.2.3", "entry": "main.luau",
                "author": "x", "assets": {"a": [1, 2]}}"#,
        );
        assert_eq!(parsed.map(|m| m.name), Ok("Pong".into()));
    }

    #[test]
    fn structural_errors() {
        assert_eq!(manifest(""), Err(CartridgeError::InvalidJson { line: 1 }));
        assert_eq!(
            manifest("{\n  \"name\": \"Pong\",\n}"),
            Err(CartridgeError::InvalidJson { line: 3 })
        );
        assert_eq!(
            CartridgeManifest::parse(b"\xff"),
            Err(CartridgeError::InvalidJson { line: 1 })
        );
        for json in ["[]", "\"Pong\"", "null", "3"] {
            assert_eq!(manifest(json), Err(CartridgeError::NotAnObject), "{json}");
        }
        assert_eq!(
            CartridgeError::InvalidJson { line: 3 }.to_string(),
            "CART.JSON IS NOT VALID JSON (LINE 3)"
        );
    }

    #[test]
    fn required_fields_must_be_present_strings() {
        assert_eq!(
            manifest(r#"{"version": "0.1.0", "entry": "main.luau"}"#),
            Err(CartridgeError::MissingField("name"))
        );
        assert_eq!(
            manifest(r#"{"name": "Pong", "entry": "main.luau"}"#),
            Err(CartridgeError::MissingField("version"))
        );
        assert_eq!(
            manifest(r#"{"name": "Pong", "version": "0.1.0"}"#),
            Err(CartridgeError::MissingField("entry"))
        );
        assert_eq!(
            manifest(r#"{"name": 3, "version": "0.1.0", "entry": "main.luau"}"#),
            Err(CartridgeError::NotAString("name"))
        );
        assert_eq!(
            manifest(r#"{"name": "Pong", "version": null, "entry": "main.luau"}"#),
            Err(CartridgeError::NotAString("version"))
        );
        assert_eq!(
            manifest(r#"{"name": "Pong", "version": "0.1.0", "entry": ["main.luau"]}"#),
            Err(CartridgeError::NotAString("entry"))
        );
        assert_eq!(
            CartridgeError::MissingField("name").to_string(),
            "CART.JSON: MISSING name"
        );
        assert_eq!(
            CartridgeError::NotAString("entry").to_string(),
            "CART.JSON: entry MUST BE A STRING"
        );
    }

    #[test]
    fn name_must_not_be_blank() {
        for name in ["", "   ", "\t\n"] {
            let json =
                serde_json::json!({ "name": name, "version": "0.1.0", "entry": "main.luau" });
            assert_eq!(
                manifest(&json.to_string()),
                Err(CartridgeError::EmptyName),
                "{name:?}"
            );
        }
        let json = r#"{"name": " Pong ", "version": "0.1.0", "entry": "main.luau"}"#;
        assert_eq!(manifest(json).map(|m| m.name), Ok(" Pong ".into()));
    }

    #[test]
    fn version_must_be_semver() {
        for version in ["", "1", "1.0", "v1.0.0", "1.0.0.0", "01.0.0"] {
            let json =
                serde_json::json!({ "name": "Pong", "version": version, "entry": "main.luau" });
            assert_eq!(
                manifest(&json.to_string()),
                Err(CartridgeError::InvalidVersion),
                "{version:?}"
            );
        }
    }

    #[test]
    fn entry_must_be_a_portable_relative_path() {
        for entry in [
            "",
            "/main.luau",
            "//main.luau",
            "../main.luau",
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
                with_entry(entry),
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
            "src/.luau",
            "main.luau.txt",
        ] {
            assert_eq!(
                with_entry(entry),
                Err(CartridgeError::EntryNotLuau),
                "{entry:?}"
            );
        }
        for entry in ["main.luau", "src/game/main.luau", "a.b.luau", "..luau"] {
            assert_eq!(with_entry(entry).map(|m| m.entry), Ok(entry.into()));
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
        assert_eq!(pong.name, "Pong");
        assert_eq!(pong.version, Version::new(0, 1, 0));
        storage.change_dir("games/owl").unwrap();
        assert_eq!(load(&storage, "../pong"), Ok(pong.clone()));
        assert_eq!(load(&storage, "/games/./owl/../pong/"), Ok(pong));
        assert_eq!(storage.cwd(), Ok("/games/owl".into()), "load does not move");
    }

    #[test]
    fn entry_may_live_in_a_subdirectory() {
        let fixture = Fixture::new("load-subdir-entry");
        let dir = fixture.root().join("games/pong");
        write_manifest(
            &dir,
            r#"{"name": "Pong", "version": "0.1.0", "entry": "src/main.luau"}"#,
        );
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/main.luau"), "").unwrap();
        assert!(load(&fixture.storage(), "games/pong").is_ok());
    }

    #[test]
    fn entry_source_reads_the_current_entry() {
        let fixture = Fixture::new("load-entry-source");
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        fs::write(dir.join("main.luau"), "print(1)").unwrap();
        let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
        assert_eq!(cartridge.entry_source(), Ok(b"print(1)".to_vec()));

        fs::write(dir.join("main.luau"), "print(2)").unwrap();
        assert_eq!(cartridge.entry_source(), Ok(b"print(2)".to_vec()));

        fs::remove_file(dir.join("main.luau")).unwrap();
        assert_eq!(
            cartridge.entry_source(),
            Err(CartridgeError::EntryNotFound("main.luau".into()))
        );
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
            "NO CART.JSON IN /games/owl"
        );
        fs::create_dir(fixture.root().join("games/owl/cart.json")).unwrap();
        assert_eq!(
            load(&storage, "games/owl"),
            Err(CartridgeError::ManifestNotAFile)
        );
    }

    #[test]
    fn manifest_errors_surface() {
        let fixture = Fixture::new("load-bad-manifest");
        let dir = fixture.root().join("games/owl");
        write_manifest(&dir, "{");
        assert_eq!(
            load(&fixture.storage(), "games/owl"),
            Err(CartridgeError::InvalidJson { line: 1 })
        );
        write_manifest(
            &dir,
            r#"{"name": "Owl", "version": "1", "entry": "main.luau"}"#,
        );
        assert_eq!(
            load(&fixture.storage(), "games/owl"),
            Err(CartridgeError::InvalidVersion)
        );
    }

    #[test]
    fn entry_must_exist_as_a_regular_file() {
        let fixture = Fixture::new("load-entry");
        let dir = fixture.root().join("games/owl");
        write_manifest(
            &dir,
            r#"{"name": "Owl", "version": "0.1.0", "entry": "src/main.luau"}"#,
        );
        let storage = fixture.storage();
        let missing = Err(CartridgeError::EntryNotFound("src/main.luau".into()));
        assert_eq!(load(&storage, "games/owl"), missing);

        // A file where a directory is expected.
        fs::write(dir.join("src"), "").unwrap();
        assert_eq!(load(&storage, "games/owl"), missing);

        fs::remove_file(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("src/main.luau")).unwrap();
        assert_eq!(
            load(&storage, "games/owl"),
            Err(CartridgeError::EntryNotAFile("src/main.luau".into()))
        );
        assert_eq!(
            CartridgeError::EntryNotAFile("src/main.luau".into()).to_string(),
            "ENTRY IS NOT A FILE: src/main.luau"
        );
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
            write_manifest(
                &pong,
                r#"{"name": "Pong", "version": "0.1.0", "entry": "main.luau"}"#,
            );
            let storage = fixture.storage();

            let targets = [
                // Outside storage, absolute and relative.
                fixture.outside().join("main.luau"),
                "../../../../outside/main.luau".into(),
                // Inside storage, but in a sibling cartridge.
                root.join("games/other/main.luau"),
                "../other/main.luau".into(),
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
            write_manifest(
                &pong,
                r#"{"name": "Pong", "version": "0.1.0", "entry": "src/main.luau"}"#,
            );
            symlink("../other", pong.join("src")).unwrap();
            assert_eq!(
                load(&fixture.storage(), "games/pong"),
                Err(CartridgeError::EntryNotFound("src/main.luau".into()))
            );
        }

        #[test]
        fn manifest_links_leaving_the_cartridge_are_missing() {
            let fixture = Fixture::new("load-link-manifest");
            let root = fixture.root();
            write_cartridge(&root.join("games/other"), "Other");
            write_manifest(
                &fixture.outside(),
                r#"{"name": "X", "version": "0.1.0", "entry": "main.luau"}"#,
            );
            let pong = root.join("games/pong");
            fs::create_dir_all(&pong).unwrap();
            fs::write(pong.join("main.luau"), "").unwrap();
            let storage = fixture.storage();

            for target in [
                fixture.outside().join("cart.json"),
                "../other/cart.json".into(),
            ] {
                let _ = fs::remove_file(pong.join("cart.json"));
                symlink(&target, pong.join("cart.json")).unwrap();
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
            write_manifest(
                &pong,
                r#"{"name": "Pong", "version": "0.1.0", "entry": "main.luau"}"#,
            );
            fs::create_dir_all(pong.join("src")).unwrap();
            fs::write(pong.join("src/game.luau"), "").unwrap();
            symlink("src/game.luau", pong.join("main.luau")).unwrap();
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
                load(&storage, "pong-link").map(|m| m.name),
                Ok("Pong".into())
            );

            // From the target's point of view `games/` is outside, even though
            // the link that reached it sits at the storage root.
            fs::remove_file(root.join("games/pong/main.luau")).unwrap();
            fs::write(root.join("main.luau"), "").unwrap();
            symlink("../../main.luau", root.join("games/pong/main.luau")).unwrap();
            assert_eq!(load(&storage, "pong-link"), entry_not_found());
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
