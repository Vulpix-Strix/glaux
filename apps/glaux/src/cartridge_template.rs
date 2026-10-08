//! New cartridges: the rules a new cartridge's metadata must follow, the
//! manifest written for it, and the starter scene it begins with.

use semver::Version;

use crate::cartridge::{
    CartridgeError, CartridgeFile, CompatibilityFile, FORMAT, GraphicsProfile, HardwareModel,
    HardwareSpec, Language, LoadedCartridge, ManifestFile, RuntimeSpec, glaux_version,
};
use crate::cartridge_fs::{CartridgeStorage, CreateDirError, FsError};

/// Entry script of every new cartridge, from the cartridge root.
const ENTRY: &str = "code/main.luau";

/// Code a new cartridge starts with.
pub const STARTER: &str = include_str!("../templates/starter/main.luau");

/// Longest accepted ID, in characters.
const MAX_ID_LEN: usize = 32;

/// Longest accepted display name or author, in characters. Keeps both on
/// one shell row next to their labels.
const MAX_TEXT_LEN: usize = 32;

/// Names Windows reserves for devices in every directory. An ID is a
/// directory name, so these would make a cartridge impossible to copy there.
const RESERVED_IDS: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Why a cartridge could not be created. Messages name virtual paths only,
/// so they are safe to show in the shell.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CreateError {
    #[error("Invalid cartridge id '{0}': use lowercase letters, digits and dashes, like my-game")]
    InvalidId(String),
    #[error("The name must not be empty")]
    EmptyName,
    #[error("The name must not contain control characters")]
    InvalidName,
    #[error("The name must be at most {MAX_TEXT_LEN} characters")]
    NameTooLong,
    #[error("Invalid version '{0}': use semver, like 1.2.0")]
    InvalidVersion(String),
    #[error("The author must be 1-{MAX_TEXT_LEN} characters, without control characters")]
    InvalidAuthor,
    /// Holds the virtual path that is already taken.
    #[error("{0} already exists")]
    Exists(String),
    #[error(transparent)]
    Fs(FsError),
    /// The written cartridge failed validation. Only a broken template
    /// produces this.
    #[error(transparent)]
    Invalid(CartridgeError),
}

/// The validated metadata of a cartridge about to be created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCartridge {
    /// Directory name: portable lowercase kebab-case.
    id: String,
    name: String,
    version: Version,
    author: Option<String>,
}

impl NewCartridge {
    /// Validates the metadata for a cartridge with directory name `id`.
    ///
    /// Without a `name`, the display name is derived from `id`. The version
    /// defaults to `0.1.0`. The name and author are trimmed but otherwise
    /// kept exactly as given.
    pub fn new(
        id: &str,
        name: Option<&str>,
        version: Option<&str>,
        author: Option<&str>,
    ) -> Result<Self, CreateError> {
        if !is_valid_id(id) {
            return Err(CreateError::InvalidId(id.to_owned()));
        }
        let name = match name {
            None => display_name(id),
            Some(name) => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(CreateError::EmptyName);
                }
                if name.chars().any(char::is_control) {
                    return Err(CreateError::InvalidName);
                }
                if name.chars().count() > MAX_TEXT_LEN {
                    return Err(CreateError::NameTooLong);
                }
                name.to_owned()
            }
        };
        let version = match version {
            None => Version::new(0, 1, 0),
            Some(text) => {
                Version::parse(text).map_err(|_| CreateError::InvalidVersion(text.to_owned()))?
            }
        };
        let author = match author {
            None => None,
            Some(author) => {
                let author = author.trim();
                let valid = !author.is_empty()
                    && author.chars().count() <= MAX_TEXT_LEN
                    && !author.chars().any(char::is_control);
                if !valid {
                    return Err(CreateError::InvalidAuthor);
                }
                Some(author.to_owned())
            }
        };
        Ok(Self {
            id: id.to_owned(),
            name,
            version,
            author,
        })
    }

    /// The `cartridge.ron` written for this cartridge: a Luau cartridge for
    /// the G1 with 4 bpp graphics, requiring this Glaux or newer.
    pub fn manifest_ron(&self) -> String {
        let file = ManifestFile {
            format: FORMAT,
            cartridge: CartridgeFile {
                name: self.name.clone(),
                version: self.version.to_string(),
                author: self.author.clone(),
            },
            runtime: RuntimeSpec {
                language: Language::Luau,
                entry: ENTRY.to_owned(),
            },
            hardware: HardwareSpec {
                model: HardwareModel::G1,
                graphics: GraphicsProfile::G1Bpp4,
            },
            compatibility: CompatibilityFile {
                min_glaux: glaux_version().to_string(),
            },
        };
        let config = ron::ser::PrettyConfig::new().indentor("    ".to_owned());
        let mut text = ron::ser::to_string_pretty(&file, config)
            .expect("the manifest has only strings, integers and unit variants");
        text.push('\n');
        text
    }
}

/// Creates the cartridge described by `spec` in the current directory of
/// `storage` and returns it loaded.
///
/// The cartridge is written and validated in a staging directory before it
/// appears under its ID, so a failure leaves nothing behind, and an existing
/// entry with that name is never replaced. Its code does not run.
pub fn create(
    storage: &CartridgeStorage,
    spec: &NewCartridge,
) -> Result<LoadedCartridge, CreateError> {
    let manifest = spec.manifest_ron();
    let files: [(&str, &[u8]); 2] = [
        ("cartridge.ron", manifest.as_bytes()),
        (ENTRY, STARTER.as_bytes()),
    ];
    let location = storage
        .create_dir_with(&spec.id, &files, |staged| {
            LoadedCartridge::open("/".into(), staged.clone()).map(drop)
        })
        .map_err(|err| match err {
            CreateDirError::Fs(FsError::AlreadyExists(path)) => CreateError::Exists(path),
            CreateDirError::Fs(err) => CreateError::Fs(err),
            CreateDirError::Invalid(err) => CreateError::Invalid(err),
        })?;
    tracing::info!(cartridge = %location, "cartridge created");
    LoadedCartridge::load(storage, &spec.id).map_err(CreateError::Invalid)
}

/// Whether `id` is lowercase kebab-case that every host accepts as a
/// directory name: words of ASCII lowercase letters and digits joined by
/// single dashes, starting with a letter.
fn is_valid_id(id: &str) -> bool {
    id.len() <= MAX_ID_LEN
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
        && !RESERVED_IDS.contains(&id)
}

/// `my-first-game` as `My First Game`.
fn display_name(id: &str) -> String {
    id.split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::cartridge::CartridgeManifest;
    use crate::cartridge::fixture::write_cartridge;
    use crate::cartridge_fs::fixture::Fixture;
    use pretty_assertions::assert_eq;

    fn spec(id: &str) -> NewCartridge {
        NewCartridge::new(id, None, None, None).unwrap()
    }

    #[test]
    fn defaults_derive_the_name_from_the_id() {
        let spec = spec("my-first-game");
        let manifest = CartridgeManifest::parse(spec.manifest_ron().as_bytes()).unwrap();
        assert_eq!(manifest.cartridge.name, "My First Game");
        assert_eq!(manifest.cartridge.version, Version::new(0, 1, 0));
        assert_eq!(manifest.cartridge.author, None);
        assert_eq!(manifest.runtime.language, Language::Luau);
        assert_eq!(manifest.runtime.entry, "code/main.luau");
        assert_eq!(manifest.hardware.model, HardwareModel::G1);
        assert_eq!(manifest.hardware.graphics, GraphicsProfile::G1Bpp4);
        assert_eq!(manifest.format, FORMAT);
        assert_eq!(&manifest.compatibility.min_glaux, glaux_version());
        assert!(!spec.manifest_ron().contains("author"));
        assert_eq!(display_name("a1-b-c2"), "A1 B C2");
    }

    #[test]
    fn explicit_metadata_is_kept_as_given() {
        let spec = NewCartridge::new(
            "space-adventure",
            Some(" Space Adventure "),
            Some("1.2.0-beta.1"),
            Some("Cuervolu \"El\" Ñandú"),
        )
        .unwrap();
        let manifest = CartridgeManifest::parse(spec.manifest_ron().as_bytes()).unwrap();
        assert_eq!(manifest.cartridge.name, "Space Adventure");
        assert_eq!(
            manifest.cartridge.version,
            Version::parse("1.2.0-beta.1").unwrap()
        );
        assert_eq!(
            manifest.cartridge.author.as_deref(),
            Some("Cuervolu \"El\" Ñandú")
        );
    }

    #[test]
    fn ids_must_be_portable_kebab_case() {
        let too_long = "a".repeat(MAX_ID_LEN + 1);
        for id in [
            "", "My-Game", "my game", "-a", "a-", "a--b", "1abc", "a_b", "a/b", "..", ".a", "é",
            "con", "lpt1", &too_long,
        ] {
            assert_eq!(
                NewCartridge::new(id, None, None, None),
                Err(CreateError::InvalidId(id.to_owned())),
                "{id:?}"
            );
        }
        for id in ["a", "pong", "my-game-2", "x1-y2", &"a".repeat(MAX_ID_LEN)] {
            assert!(NewCartridge::new(id, None, None, None).is_ok(), "{id:?}");
        }
    }

    #[test]
    fn versions_must_be_semver() {
        for version in ["1", "1.0", "v1.0.0", "", "1.0.0.0"] {
            assert_eq!(
                NewCartridge::new("a", None, Some(version), None),
                Err(CreateError::InvalidVersion(version.to_owned())),
                "{version:?}"
            );
        }
        assert_eq!(
            CreateError::InvalidVersion("1.0".into()).to_string(),
            "Invalid version '1.0': use semver, like 1.2.0"
        );
    }

    #[test]
    fn names_and_authors_are_checked() {
        let long = "x".repeat(MAX_TEXT_LEN + 1);
        let new = |name, author| NewCartridge::new("a", name, None, author).map(drop);
        assert_eq!(new(Some("  "), None), Err(CreateError::EmptyName));
        assert_eq!(new(Some("a\nb"), None), Err(CreateError::InvalidName));
        assert_eq!(new(Some(&long), None), Err(CreateError::NameTooLong));
        for author in ["", " ", "a\tb", &long] {
            assert_eq!(
                new(None, Some(author)),
                Err(CreateError::InvalidAuthor),
                "{author:?}"
            );
        }
    }

    #[test]
    fn created_cartridges_load_with_the_starter() {
        let fixture = Fixture::new("template-create");
        let mut storage = fixture.storage();
        storage.change_dir("games").unwrap();
        let cartridge = create(&storage, &spec("my-game")).unwrap();
        assert_eq!(cartridge.manifest().cartridge.name, "My Game");
        assert_eq!(cartridge.entry_source(), Ok(STARTER.as_bytes().to_vec()));
        let dir = fixture.root().join("games/my-game");
        assert_eq!(
            fs::read_to_string(dir.join("cartridge.ron")).unwrap(),
            spec("my-game").manifest_ron()
        );
        let mut names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, ["cartridge.ron", "code"]);
    }

    #[test]
    fn existing_entries_are_never_replaced() {
        let fixture = Fixture::new("template-exists");
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        let before = fs::read(dir.join("cartridge.ron")).unwrap();
        let storage = fixture.storage();
        assert_eq!(
            create(&storage, &spec("pong")).map(drop),
            Err(CreateError::Exists("/pong".into()))
        );
        assert_eq!(fs::read(dir.join("cartridge.ron")).unwrap(), before);
        assert_eq!(fs::read(dir.join("code/main.luau")).unwrap(), b"");

        fs::write(fixture.root().join("note"), "kept").unwrap();
        assert_eq!(
            create(&storage, &spec("note")).map(drop),
            Err(CreateError::Exists("/note".into()))
        );
        assert_eq!(fs::read(fixture.root().join("note")).unwrap(), b"kept");
    }

    #[test]
    fn unavailable_storage_creates_nothing() {
        assert_eq!(
            create(&CartridgeStorage::Unavailable, &spec("a")).map(drop),
            Err(CreateError::Fs(FsError::Unavailable))
        );
    }
}
