//! Script source files of a cartridge, named by their path from its root.

use std::fmt;

use super::{Language, LoadedCartridge, is_portable_path, is_script_of};
use crate::cartridge_fs::{FsError, ReplaceError, WalkGap, WalkLimits};

/// How far [`LoadedCartridge::script_files`] searches.
const SCRIPT_WALK: WalkLimits = WalkLimits {
    max_depth: 16,
    max_files: 1024,
};

/// The portable path of a script file from the cartridge root, as in
/// `code/player.luau`. Its extension names its language, whatever language
/// the manifest declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePath {
    path: String,
    language: Language,
}

/// A path rejected by [`SourcePath::parse`]. Holds the path as given.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourcePathError {
    /// Not a portable path relative to the cartridge root.
    #[error("Invalid path: {0}")]
    Invalid(String),
    /// Not a `.luau` or `.rhai` file.
    #[error("Not a Luau or Rhai file: {0}")]
    UnsupportedExtension(String),
}

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

    /// The last component, as in `player.luau`.
    pub fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// The order paths are listed in: ignoring case, then exactly, so it
    /// never depends on the order the host reports files in.
    pub fn listing_order(&self, other: &Self) -> std::cmp::Ordering {
        self.path
            .to_lowercase()
            .cmp(&other.path.to_lowercase())
            .then_with(|| self.path.cmp(&other.path))
    }
}

/// The script files of a cartridge, and the parts of it not searched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptListing {
    /// In [`SourcePath::listing_order`].
    pub paths: Vec<SourcePath>,
    /// Empty only when the whole cartridge was searched.
    pub gaps: Vec<WalkGap>,
}

/// A failed [`LoadedCartridge::replace_source`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceWriteError {
    /// The file could not be checked or written.
    Failed(FsError),
    /// The file no longer holds what the caller expected. Holds what it
    /// holds now, `None` when it is absent.
    Changed(Option<Vec<u8>>),
}

/// A failed [`LoadedCartridge::delete_source`] or
/// [`LoadedCartridge::rename_source`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceEditError {
    /// The manifest names the file as the cartridge's entry, which Glaux
    /// neither deletes nor moves.
    #[error("{0} is the entry script of this cartridge")]
    Entry(SourcePath),
    /// A script keeps the language its extension names.
    #[error("{from} cannot become {to}: a script keeps its language")]
    Language { from: SourcePath, to: SourcePath },
    /// The file no longer holds what the caller saw. Nothing was deleted.
    #[error("{0} changed on disk, so it was not deleted")]
    Changed(SourcePath),
    #[error(transparent)]
    Fs(#[from] FsError),
}

impl fmt::Display for SourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.path)
    }
}

impl LoadedCartridge {
    /// Current bytes of the script file at `path`, read on every call.
    ///
    /// Lookups are confined to the cartridge directory: symlinks leading
    /// out of it, including into other cartridges, are
    /// [`FsError::NotFound`].
    pub fn read_source(&self, path: &SourcePath) -> Result<Vec<u8>, FsError> {
        self.files.read_file(&format!("/{path}"))
    }

    /// Replaces the script file at `path` with `bytes`, provided it still
    /// holds `expected`, where `None` means the file must be absent.
    ///
    /// See [`CartridgeFs::replace_file`]. The file is replaced whole or not
    /// at all, a read-only file is never replaced, and names reached
    /// through symlinks leading out of the cartridge are never written.
    ///
    /// [`CartridgeFs::replace_file`]: crate::cartridge_fs::CartridgeFs::replace_file
    pub fn replace_source(
        &self,
        path: &SourcePath,
        bytes: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<(), SourceWriteError> {
        self.files
            .replace_file(&format!("/{path}"), bytes, expected)
            .map_err(|err| match err {
                ReplaceError::Changed(current) => SourceWriteError::Changed(current),
                ReplaceError::Fs(err) => SourceWriteError::Failed(err),
            })
    }

    /// Creates an empty script file at `path`, and the directories above it
    /// that are missing. An existing file is [`FsError::AlreadyExists`] and
    /// is left as it is.
    ///
    /// The file is created as [`replace_source`](Self::replace_source)
    /// writes, so it is never written through a link leading out of the
    /// cartridge. Directories created before a failure are kept.
    pub fn create_source(&self, path: &SourcePath) -> Result<(), FsError> {
        if let Some((dir, _)) = path.as_str().rsplit_once('/') {
            self.files.create_dirs(&format!("/{dir}"))?;
        }
        self.files
            .replace_file(&format!("/{path}"), b"", None)
            .map_err(|err| match err {
                ReplaceError::Changed(_) => FsError::AlreadyExists(format!("/{path}")),
                ReplaceError::Fs(err) => err,
            })
    }

    /// Whether `path` is the entry script the manifest declares.
    pub fn is_entry(&self, path: &SourcePath) -> bool {
        self.manifest.runtime.entry == path.as_str()
    }

    /// Deletes the script file at `path`, provided it still holds exactly
    /// `expected`, where `None` means the file must be absent and there is
    /// nothing to delete. A file that vanished meanwhile is as good as
    /// deleted. The entry script is never deleted, and directories above
    /// the file are kept.
    ///
    /// See [`CartridgeFs::remove_file`]. A link is never followed. No
    /// portable primitive compares and unlinks in one step, so a write
    /// landing between the comparison and the removal is deleted with the
    /// file.
    ///
    /// [`CartridgeFs::remove_file`]: crate::cartridge_fs::CartridgeFs::remove_file
    pub fn delete_source(
        &self,
        path: &SourcePath,
        expected: Option<&[u8]>,
    ) -> Result<(), SourceEditError> {
        if self.is_entry(path) {
            return Err(SourceEditError::Entry(path.clone()));
        }
        let current = match self.read_source(path) {
            Ok(bytes) => bytes,
            Err(FsError::NotFound(_) | FsError::NotADirectory(_)) => return Ok(()),
            Err(err) => return Err(err.into()),
        };
        if Some(current.as_slice()) != expected {
            return Err(SourceEditError::Changed(path.clone()));
        }
        self.files.remove_file(&format!("/{path}"))?;
        Ok(())
    }

    /// Moves the script file at `from` to `to`, creating the directories
    /// above `to` that are missing. The entry script never moves, and
    /// neither does a file to a path of another language.
    ///
    /// See [`CartridgeFs::rename_file`]. Nothing at `to` is ever replaced,
    /// and a link is never followed.
    ///
    /// [`CartridgeFs::rename_file`]: crate::cartridge_fs::CartridgeFs::rename_file
    pub fn rename_source(&self, from: &SourcePath, to: &SourcePath) -> Result<(), SourceEditError> {
        if self.is_entry(from) {
            return Err(SourceEditError::Entry(from.clone()));
        }
        if self.is_entry(to) {
            return Err(FsError::AlreadyExists(format!("/{to}")).into());
        }
        if from.language() != to.language() {
            return Err(SourceEditError::Language {
                from: from.clone(),
                to: to.clone(),
            });
        }
        self.files
            .rename_file(&format!("/{from}"), &format!("/{to}"))?;
        Ok(())
    }

    /// Every `.luau` and `.rhai` file in the cartridge, in any directory.
    ///
    /// Searched as [`CartridgeFs::walk_files`] searches, within depth and
    /// file limits. Whatever was not searched is reported in
    /// [`ScriptListing::gaps`]. Fails only when the cartridge directory
    /// itself cannot be listed.
    ///
    /// [`CartridgeFs::walk_files`]: crate::cartridge_fs::CartridgeFs::walk_files
    pub fn script_files(&self) -> Result<ScriptListing, FsError> {
        self.script_files_within(SCRIPT_WALK)
    }

    pub(crate) fn script_files_within(&self, limits: WalkLimits) -> Result<ScriptListing, FsError> {
        let walk = self
            .files
            .walk_files(limits, |path| SourcePath::parse(path).is_ok())?;
        let mut paths: Vec<SourcePath> = walk
            .files
            .iter()
            .filter_map(|path| SourcePath::parse(path).ok())
            .collect();
        paths.sort_by(SourcePath::listing_order);
        Ok(ScriptListing {
            paths,
            gaps: walk.gaps,
        })
    }
}

#[cfg(test)]
mod tests;
