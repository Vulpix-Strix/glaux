//! Creates new cartridges. This covers the rules their metadata must follow,
//! the manifest written for them, and the starter scene they begin with.

use semver::Version;

use crate::cartridge::{
    CartridgeError, CartridgeFile, CompatibilityFile, FORMAT, GraphicsProfile, HardwareModel,
    HardwareSpec, Language, LoadedCartridge, ManifestFile, RuntimeSpec, glaux_version,
};
use crate::cartridge_fs::{CartridgeStorage, CreateDirError, FsError};

/// Entry script of every new Luau cartridge, from the cartridge root.
const ENTRY: &str = "code/main.luau";

/// Code a new Luau cartridge starts with.
pub const STARTER: &str = include_str!("../templates/starter/main.luau");

/// Entry script of every new Rhai cartridge, from the cartridge root.
const RHAI_ENTRY: &str = "code/main.rhai";

/// Code a new Rhai cartridge starts with: the Luau starter's scene.
pub const RHAI_STARTER: &str = include_str!("../templates/starter/main.rhai");

/// The owl sprite every new cartridge starts with: native tiles, tile
/// attributes and object palettes, imported from the Glaux logo. Its colors
/// all come from the system palette, so no `palette.bin` is needed.
const SPRITES: [(&str, &[u8]); 3] = [
    (
        "sprites.bin",
        include_bytes!("../templates/starter/sprites.bin"),
    ),
    (
        "objattr.bin",
        include_bytes!("../templates/starter/objattr.bin"),
    ),
    ("obp.bin", include_bytes!("../templates/starter/obp.bin")),
];

/// Longest accepted ID, in characters.
const MAX_ID_LEN: usize = 32;

/// Longest accepted display name or author, in characters. Keeps both on
/// one shell row next to their labels.
pub(crate) const MAX_TEXT_LEN: usize = 32;

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
    #[error("Unsupported language '{0}': use luau or rhai")]
    UnsupportedLanguage(String),
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
    language: Language,
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
            language: Language::Luau,
        })
    }

    /// The same cartridge written in the language named `name`, ignoring
    /// case. New cartridges are Luau unless this chooses otherwise.
    pub fn with_language(self, name: &str) -> Result<Self, CreateError> {
        let language = Language::from_name(name)
            .ok_or_else(|| CreateError::UnsupportedLanguage(name.to_owned()))?;
        Ok(Self { language, ..self })
    }

    /// The entry script's path and starting code.
    fn entry(&self) -> (&'static str, &'static str) {
        match self.language {
            Language::Luau => (ENTRY, STARTER),
            Language::Rhai => (RHAI_ENTRY, RHAI_STARTER),
        }
    }

    /// The `cartridge.ron` written for this cartridge: a cartridge in its
    /// language for the G1 with 4 bpp graphics, requiring this Glaux or
    /// newer.
    pub fn manifest_ron(&self) -> String {
        let file = ManifestFile {
            format: FORMAT,
            cartridge: CartridgeFile {
                name: self.name.clone(),
                version: self.version.to_string(),
                author: self.author.clone(),
            },
            runtime: RuntimeSpec {
                language: self.language,
                entry: self.entry().0.to_owned(),
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
    let (entry, starter) = spec.entry();
    let mut files: Vec<(&str, &[u8])> = vec![
        ("cartridge.ron", manifest.as_bytes()),
        (entry, starter.as_bytes()),
    ];
    files.extend(SPRITES);
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

/// An ID for a cartridge described by free text such as a display name,
/// as in `My New Game` → `my-new-game`, when one that follows the ID rules
/// can be made from its ASCII letters and digits.
pub fn suggest_id(text: &str) -> Option<String> {
    // Dropping other letters would turn `Ñandú` into `and`.
    if text
        .chars()
        .any(|c| c.is_alphanumeric() && !c.is_ascii_alphanumeric())
    {
        return None;
    }
    let id = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-");
    is_valid_id(&id).then_some(id)
}

/// The display name a cartridge gets by default: `my-first-game` as
/// `My First Game`.
pub fn display_name(id: &str) -> String {
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
mod tests;
