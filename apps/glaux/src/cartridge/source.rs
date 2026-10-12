//! Script source files of a cartridge, named by their path from its root.

use std::fmt;

use super::{Language, LoadedCartridge, is_portable_path, is_script_of};
use crate::cartridge_fs::FsError;

/// The portable path of a script file from the cartridge root, as in
/// `code/player.luau`. Its extension names its language, whatever language
/// the manifest declares.
#[cfg_attr(not(test), expect(dead_code, reason = "no caller outside tests"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePath {
    path: String,
    language: Language,
}

/// A path rejected by [`SourcePath::parse`]. Holds the path as given.
#[cfg_attr(not(test), expect(dead_code, reason = "no caller outside tests"))]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourcePathError {
    /// Not a portable path relative to the cartridge root.
    #[error("Invalid path: {0}")]
    Invalid(String),
    /// Not a `.luau` or `.rhai` file.
    #[error("Not a Luau or Rhai file: {0}")]
    UnsupportedExtension(String),
}

#[cfg_attr(not(test), expect(dead_code, reason = "no caller outside tests"))]
impl SourcePath {
    /// Validates `path` as relative to the cartridge root under the same
    /// rules as the manifest's entry: `/`-separated plain names, no
    /// absolute paths, `.`, `..`, `\` or `:`. The file name must be a
    /// non-empty stem followed by `.luau` or `.rhai`, in lowercase.
    pub fn parse(path: &str) -> Result<Self, SourcePathError> {
        if !is_portable_path(path) {
            return Err(SourcePathError::Invalid(path.to_owned()));
        }
        let language = Language::ALL
            .into_iter()
            .find(|&language| is_script_of(path, language))
            .ok_or_else(|| SourcePathError::UnsupportedExtension(path.to_owned()))?;
        Ok(Self {
            path: path.to_owned(),
            language,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.path
    }

    pub fn language(&self) -> Language {
        self.language
    }
}

impl fmt::Display for SourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.path)
    }
}

#[cfg_attr(not(test), expect(dead_code, reason = "no caller outside tests"))]
impl LoadedCartridge {
    /// Current bytes of the script file at `path`, read on every call.
    ///
    /// Lookups are confined to the cartridge directory: symlinks leading
    /// out of it, including into other cartridges, are
    /// [`FsError::NotFound`].
    pub fn read_source(&self, path: &SourcePath) -> Result<Vec<u8>, FsError> {
        self.files.read_file(&format!("/{path}"))
    }
}

#[cfg(test)]
mod tests;
