//! The cartridge directory, presented to the shell as a filesystem rooted
//! at `/`.
//!
//! Paths inside Glaux are virtual: `/`-separated components below the
//! cartridge root. Host paths never leave this module except in logs.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use anyhow::Context as _;

/// Name of the cartridge root inside the application data directory.
const CARTRIDGE_DIR: &str = "cartridges";

/// Symlinks followed while locating one path before giving up, as with
/// POSIX `MAXSYMLINKS`. Bounds link cycles.
const MAX_SYMLINK_HOPS: usize = 40;

/// Name prefix of the directories new trees are built in before they are
/// published. Listings hide them.
const STAGING_PREFIX: &str = ".glaux-staging-";

/// A staging directory untouched for this long was left behind by an
/// interrupted creation and is removed by the next creation beside it.
const STALE_STAGING: Duration = Duration::from_secs(60 * 60);

/// Kind of a directory entry. Directories order first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryKind {
    Directory,
    File,
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
}

/// A failed filesystem request. Paths are virtual, so the message is safe to
/// show in the shell.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FsError {
    #[error("No such file or directory: {0}")]
    NotFound(String),
    #[error("Not a directory: {0}")]
    NotADirectory(String),
    #[error("Not a file: {0}")]
    NotAFile(String),
    #[error("Cannot read {0}")]
    Unreadable(String),
    /// Something already exists where a new entry was to be published.
    #[error("Already exists: {0}")]
    AlreadyExists(String),
    #[error("Cannot write {0}")]
    Unwritable(String),
    /// The file is marked read-only, so it is left as it is.
    #[error("Read-only: {0}")]
    ReadOnly(String),
    #[error("Cartridge storage unavailable")]
    Unavailable,
}

/// A failed [`CartridgeFs::create_dir_with`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateDirError<E> {
    Fs(FsError),
    /// The validation callback rejected the staged tree.
    Invalid(E),
}

impl<E> From<FsError> for CreateDirError<E> {
    fn from(err: FsError) -> Self {
        Self::Fs(err)
    }
}

/// A failed [`CartridgeFs::replace_file`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplaceError {
    Fs(FsError),
    /// The file no longer holds what the caller expected. Holds what it
    /// holds now, `None` when it is absent.
    Changed(Option<Vec<u8>>),
}

impl From<FsError> for ReplaceError {
    fn from(err: FsError) -> Self {
        Self::Fs(err)
    }
}

/// Cartridge storage as the shell sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CartridgeStorage {
    Available(CartridgeFs),
    /// The cartridge directory could not be opened; the cause was logged.
    Unavailable,
}

impl CartridgeStorage {
    /// Opens the cartridge directory in `data_dir`.
    ///
    /// Without a data directory, or when opening fails, logs the cause and
    /// returns [`CartridgeStorage::Unavailable`]: the system runs without
    /// cartridge storage rather than failing to start.
    pub fn open(data_dir: Option<&Path>) -> Self {
        let opened = data_dir
            .context("no data directory is available on this platform")
            .and_then(CartridgeFs::open);
        match opened {
            Ok(fs) => Self::Available(fs),
            Err(err) => {
                tracing::warn!(
                    error = %format!("{err:#}"),
                    "cartridge storage unavailable, filesystem commands are disabled"
                );
                Self::Unavailable
            }
        }
    }

    /// The current directory as a virtual path.
    pub fn cwd(&self) -> Result<String, FsError> {
        Ok(self.fs()?.cwd())
    }

    /// See [`CartridgeFs::list`].
    pub fn list(&self, path: Option<&str>) -> Result<Vec<DirEntry>, FsError> {
        self.fs()?.list(path)
    }

    /// See [`CartridgeFs::change_dir`].
    pub fn change_dir(&mut self, path: &str) -> Result<(), FsError> {
        match self {
            Self::Available(fs) => fs.change_dir(path),
            Self::Unavailable => Err(FsError::Unavailable),
        }
    }

    /// See [`CartridgeFs::subdir`].
    pub fn subdir(&self, path: &str) -> Result<(String, CartridgeFs), FsError> {
        self.fs()?.subdir(path)
    }

    /// See [`CartridgeFs::create_dir_with`].
    pub fn create_dir_with<E>(
        &self,
        name: &str,
        files: &[(&str, &[u8])],
        validate: impl FnOnce(&CartridgeFs) -> Result<(), E>,
    ) -> Result<String, CreateDirError<E>> {
        self.fs()?.create_dir_with(name, files, validate)
    }

    fn fs(&self) -> Result<&CartridgeFs, FsError> {
        match self {
            Self::Available(fs) => Ok(fs),
            Self::Unavailable => Err(FsError::Unavailable),
        }
    }
}

/// Navigation state over the cartridge directory, which Glaux presents
/// as `/`.
///
/// Nothing outside the root is reachable: `..` stops at `/`, and symlinks
/// are followed only when their target lies inside the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeFs {
    /// Canonical host path of `/`.
    root: PathBuf,
    /// Components of the current directory below `/`.
    cwd: Vec<String>,
}

/// Why a virtual path has no accessible host entry.
#[derive(Debug)]
enum Lookup {
    /// Missing, or hidden because reaching it would leave the root.
    Missing,
    Io(io::Error),
}

impl CartridgeFs {
    /// Creates the cartridge directory inside `data_dir` if needed and
    /// starts at `/`.
    pub fn open(data_dir: &Path) -> anyhow::Result<Self> {
        let dir = data_dir.join(CARTRIDGE_DIR);
        fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
        let root = fs::canonicalize(&dir)
            .with_context(|| format!("failed to resolve {}", dir.display()))?;
        tracing::info!(root = %root.display(), "cartridge directory ready");
        Ok(Self::new(root))
    }

    /// Navigation at `/` over `root`, without touching the filesystem.
    ///
    /// `root` must be canonical (absolute, free of symlinks): containment
    /// checks compare symlink targets against it lexically.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            cwd: Vec::new(),
        }
    }

    /// The current directory as a virtual path.
    pub fn cwd(&self) -> String {
        virtual_path(&self.cwd)
    }

    /// Makes the directory at `path`, absolute or relative to the current
    /// directory, the current directory. The current directory is unchanged
    /// on error.
    pub fn change_dir(&mut self, path: &str) -> Result<(), FsError> {
        let target = self.resolve(path)?;
        self.directory(&target)?;
        self.cwd = target;
        Ok(())
    }

    /// Entries of the directory at `path`, or of the current directory.
    ///
    /// Directories come first, then files, each sorted by name ignoring
    /// case. Entries that cannot be reached inside the root (such as
    /// symlinks leading out of it) and names the shell cannot address are
    /// left out.
    pub fn list(&self, path: Option<&str>) -> Result<Vec<DirEntry>, FsError> {
        let target = match path {
            Some(path) => self.resolve(path)?,
            None => self.cwd.clone(),
        };
        let dir = self.directory(&target)?;
        let entries = fs::read_dir(&dir).map_err(|err| io_failure(err, &target))?;

        let mut listing = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|err| io_failure(err, &target))?;
            let Some(name) = entry
                .file_name()
                .into_string()
                .ok()
                .filter(|n| is_name(n) && !n.starts_with(STAGING_PREFIX))
            else {
                continue;
            };
            let mut components = target.clone();
            components.push(name.clone());
            if let Ok((_, kind)) = self.locate(&components) {
                listing.push(DirEntry { name, kind });
            }
        }
        listing.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(listing)
    }

    /// The directory at `path`, absolute or relative to the current
    /// directory, as a filesystem of its own: returns the directory's virtual
    /// path here and a navigation state at `/` rooted at it.
    ///
    /// The new root is the directory's canonical host path, so every lookup
    /// through the returned filesystem is confined to that directory: `..`
    /// stops at it and symlinks leading out of it are not followed, even when
    /// their target lies elsewhere inside this filesystem.
    pub fn subdir(&self, path: &str) -> Result<(String, CartridgeFs), FsError> {
        let target = self.resolve(path)?;
        let dir = self.directory(&target)?;
        let canonical = fs::canonicalize(&dir).map_err(|err| io_failure(err, &target))?;
        // `locate` only yields paths inside the root, but the tree may change
        // between that walk and `canonicalize`.
        if !canonical.starts_with(&self.root) {
            tracing::debug!(
                path = %virtual_path(&target),
                resolved = %canonical.display(),
                "directory resolved outside the cartridge root"
            );
            return Err(FsError::NotFound(virtual_path(&target)));
        }
        Ok((virtual_path(&target), Self::new(canonical)))
    }

    /// The real directory `name` directly inside the root, as a filesystem
    /// of its own rooted at it.
    ///
    /// Unlike [`subdir`](Self::subdir), `name` is never followed through a
    /// symlink: a linked entry is [`FsError::NotFound`] even when its target
    /// lies inside this filesystem. Every lookup through the result is
    /// confined to that directory.
    pub fn real_subdir(&self, name: &str) -> Result<CartridgeFs, FsError> {
        let missing = || FsError::NotFound(virtual_path(&[name.to_owned()]));
        if !is_name(name) {
            return Err(missing());
        }
        let dir = self.root.join(name);
        let meta = fs::symlink_metadata(&dir).map_err(|err| match lookup_error(err) {
            Lookup::Missing => missing(),
            Lookup::Io(err) => io_failure(err, &[name.to_owned()]),
        })?;
        if meta.file_type().is_symlink() {
            return Err(missing());
        }
        if !meta.is_dir() {
            return Err(FsError::NotADirectory(virtual_path(&[name.to_owned()])));
        }
        // A canonical root joined with a plain name that is a real directory
        // is itself canonical.
        Ok(Self::new(dir))
    }

    /// Fails unless `path`, absolute or relative to the current directory,
    /// is a regular file.
    pub fn ensure_file(&self, path: &str) -> Result<(), FsError> {
        let target = self.resolve(path)?;
        self.regular_file(&target).map(drop)
    }

    /// Contents of the regular file at `path`, absolute or relative to the
    /// current directory.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, FsError> {
        let target = self.resolve(path)?;
        let file = self.regular_file(&target)?;
        fs::read(&file).map_err(|err| io_failure(err, &target))
    }

    /// Replaces the regular file at `path`, absolute or relative to the
    /// current directory, with `contents`, provided it still holds
    /// `expected` (`None`: provided it does not exist).
    ///
    /// The file is written in full to a hidden staging file beside it,
    /// synced, and renamed over it, so `path` holds either its old contents
    /// or `contents`, never a mix. A file reached through a symlink inside
    /// the root is replaced at its target. Unreachable names (links leading
    /// out of the root or nowhere) are never replaced.
    ///
    /// A file marked read-only is [`FsError::ReadOnly`] and is left alone,
    /// even where the platform would let a rename replace it.
    ///
    /// The file is compared with `expected` once more just before the
    /// rename. No portable primitive makes that comparison and the rename
    /// one step, so a write landing in between is still replaced.
    pub fn replace_file(
        &self,
        path: &str,
        contents: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<(), ReplaceError> {
        let target = self.resolve(path)?;
        let Some((name, parent)) = target.split_last() else {
            return Err(FsError::NotAFile(virtual_path(&target)).into());
        };
        if name.starts_with(STAGING_PREFIX) {
            return Err(FsError::Unwritable(virtual_path(&target)).into());
        }
        let (dest, permissions) = match self.locate(&target) {
            Ok((host, EntryKind::File)) => {
                let meta = writable_file(&host, &target)?;
                (host, Some(meta.permissions()))
            }
            Ok((_, EntryKind::Directory)) => {
                return Err(FsError::NotAFile(virtual_path(&target)).into());
            }
            Err(Lookup::Missing) => {
                let dest = self.directory(parent)?.join(name);
                // Something unreachable holds the name: a dangling link or
                // one leading out of the root.
                match fs::symlink_metadata(&dest) {
                    Ok(_) => return Err(FsError::Unwritable(virtual_path(&target)).into()),
                    Err(err) if err.kind() == ErrorKind::NotFound => {}
                    Err(err) => return Err(io_failure(err, &target).into()),
                }
                (dest, None)
            }
            Err(Lookup::Io(err)) => return Err(io_failure(err, &target).into()),
        };
        let Some(dir) = dest.parent() else {
            return Err(FsError::Unwritable(virtual_path(&target)).into());
        };
        sweep_stale_staging(dir);
        let (staging, mut file) =
            create_staging_file(dir, name).map_err(|err| write_failure(err, &target))?;
        let result = (|| {
            file.write_all(contents)
                .and_then(|()| match &permissions {
                    Some(permissions) => file.set_permissions(permissions.clone()),
                    None => Ok(()),
                })
                .and_then(|()| file.sync_all())
                .map_err(|err| write_failure(err, &target))?;
            drop(file);
            let current = current_contents(&dest, &target)?;
            if current.as_deref() != expected {
                return Err(ReplaceError::Changed(current));
            }
            fs::rename(&staging, &dest).map_err(|err| write_failure(err, &target))?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                if let Err(err) = sync_dir(dir) {
                    tracing::warn!(path = %virtual_path(&target), error = %err, "failed to sync a replaced file's directory");
                }
                Ok(())
            }
            Err(err) => {
                if let Err(remove) = fs::remove_file(&staging)
                    && remove.kind() != ErrorKind::NotFound
                {
                    tracing::warn!(
                        staging = %staging.display(),
                        error = %remove,
                        "failed to remove a staging file"
                    );
                }
                Err(err)
            }
        }
    }

    /// Creates the directory `name` in the current directory holding
    /// `files`, given as `/`-separated paths relative to it, and returns its
    /// virtual path.
    ///
    /// The tree is built in a uniquely named staging directory beside the
    /// destination and handed to `validate` as a filesystem of its own. Only
    /// a tree that passes is published under `name`, and publishing never
    /// replaces anything: if an entry of that name exists, or appears while
    /// the tree is built, the result is [`FsError::AlreadyExists`]. On any
    /// failure the staging directory is removed, so `name` either holds the
    /// complete tree or was never created.
    ///
    /// Staging directories left behind by an interrupted creation are
    /// removed once they are an hour old.
    pub fn create_dir_with<E>(
        &self,
        name: &str,
        files: &[(&str, &[u8])],
        validate: impl FnOnce(&CartridgeFs) -> Result<(), E>,
    ) -> Result<String, CreateDirError<E>> {
        let mut components = self.cwd.clone();
        components.push(name.to_owned());
        if !is_name(name) || name.starts_with(STAGING_PREFIX) {
            return Err(FsError::Unwritable(virtual_path(&components)).into());
        }
        if files.iter().any(|(path, _)| !path.split('/').all(is_name)) {
            return Err(FsError::Unwritable(virtual_path(&components)).into());
        }

        let parent = self.directory(&self.cwd)?;
        sweep_stale_staging(&parent);
        let target = parent.join(name);
        match fs::symlink_metadata(&target) {
            Ok(_) => return Err(FsError::AlreadyExists(virtual_path(&components)).into()),
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => return Err(io_failure(err, &components).into()),
        }

        let staging =
            create_staging(&parent, name).map_err(|err| write_failure(err, &components))?;
        let result = stage_and_publish(&staging, &target, &components, files, validate);
        if result.is_err()
            && let Err(err) = fs::remove_dir_all(&staging)
            && err.kind() != ErrorKind::NotFound
        {
            tracing::warn!(
                staging = %staging.display(),
                error = %err,
                "failed to remove a staging directory"
            );
        }
        result.map(|()| virtual_path(&components))
    }

    /// Resolves `path` lexically into components below `/`. `..` at `/`
    /// stays at `/`.
    fn resolve(&self, path: &str) -> Result<Vec<String>, FsError> {
        let mut components = if path.starts_with('/') {
            Vec::new()
        } else {
            self.cwd.clone()
        };
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                name if is_name(name) => components.push(name.to_owned()),
                name => {
                    components.push(name.to_owned());
                    return Err(FsError::NotFound(virtual_path(&components)));
                }
            }
        }
        Ok(components)
    }

    /// Host path of the directory at `components`.
    fn directory(&self, components: &[String]) -> Result<PathBuf, FsError> {
        match self.locate(components) {
            Ok((path, EntryKind::Directory)) => Ok(path),
            Ok((_, EntryKind::File)) => Err(FsError::NotADirectory(virtual_path(components))),
            Err(Lookup::Missing) => Err(FsError::NotFound(virtual_path(components))),
            Err(Lookup::Io(err)) => Err(io_failure(err, components)),
        }
    }

    /// Host path of the regular file at `components`.
    ///
    /// Anything else that is not a directory (a FIFO, socket or device) is
    /// [`FsError::NotAFile`] too, so reading never blocks on a special file.
    fn regular_file(&self, components: &[String]) -> Result<PathBuf, FsError> {
        match self.locate(components) {
            Ok((path, EntryKind::File)) => {
                // `locate` returns a path free of symlinks, so this inspects
                // the file itself.
                let meta =
                    fs::symlink_metadata(&path).map_err(|err| io_failure(err, components))?;
                if meta.is_file() {
                    Ok(path)
                } else {
                    Err(FsError::NotAFile(virtual_path(components)))
                }
            }
            Ok((_, EntryKind::Directory)) => Err(FsError::NotAFile(virtual_path(components))),
            Err(Lookup::Missing) => Err(FsError::NotFound(virtual_path(components))),
            Err(Lookup::Io(err)) => Err(io_failure(err, components)),
        }
    }

    /// Host path and kind of the entry at `components`.
    ///
    /// Walks down from the root with `symlink_metadata`, so every directory
    /// reached is a real directory inside the root. A symlink's target is
    /// normalized lexically and must lie inside the root before anything at
    /// it is inspected; the walk then restarts from the root along the
    /// target. Targets outside the root, dangling links and link cycles are
    /// [`Lookup::Missing`].
    fn locate(&self, components: &[String]) -> Result<(PathBuf, EntryKind), Lookup> {
        let mut pending: VecDeque<OsString> = components.iter().map(OsString::from).collect();
        let mut current = self.root.clone();
        let mut kind = EntryKind::Directory;
        let mut hops = 0;
        while let Some(name) = pending.pop_front() {
            if kind != EntryKind::Directory {
                return Err(Lookup::Missing);
            }
            let next = current.join(&name);
            let meta = fs::symlink_metadata(&next).map_err(lookup_error)?;
            if meta.file_type().is_symlink() {
                hops += 1;
                if hops > MAX_SYMLINK_HOPS {
                    return Err(Lookup::Missing);
                }
                let target = normalize(&current.join(fs::read_link(&next).map_err(lookup_error)?));
                let Ok(inside) = target.strip_prefix(&self.root) else {
                    return Err(Lookup::Missing);
                };
                for component in inside.components().rev() {
                    pending.push_front(component.as_os_str().to_owned());
                }
                current = self.root.clone();
                kind = EntryKind::Directory;
                continue;
            }
            kind = if meta.is_dir() {
                EntryKind::Directory
            } else {
                EntryKind::File
            };
            current = next;
        }
        Ok((current, kind))
    }
}

/// Metadata of the regular file at `host`, failing if it is anything else
/// or marked read-only. `components` names it in errors.
fn writable_file(host: &Path, components: &[String]) -> Result<fs::Metadata, FsError> {
    let meta = fs::symlink_metadata(host).map_err(|err| io_failure(err, components))?;
    if !meta.is_file() {
        return Err(FsError::NotAFile(virtual_path(components)));
    }
    if meta.permissions().readonly() {
        return Err(FsError::ReadOnly(virtual_path(components)));
    }
    Ok(meta)
}

/// What the file at `host` holds now, `None` when it is absent.
fn current_contents(host: &Path, components: &[String]) -> Result<Option<Vec<u8>>, FsError> {
    match fs::symlink_metadata(host) {
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(io_failure(err, components)),
        Ok(_) => {}
    }
    writable_file(host, components)?;
    fs::read(host)
        .map(Some)
        .map_err(|err| io_failure(err, components))
}

/// Makes renames and new entries in `dir` durable where the platform allows
/// syncing a directory.
fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// Writes `files` into `staging`, validates the result and publishes it as
/// `target`. `components` is the virtual path of `target`, for errors.
fn stage_and_publish<E>(
    staging: &Path,
    target: &Path,
    components: &[String],
    files: &[(&str, &[u8])],
    validate: impl FnOnce(&CartridgeFs) -> Result<(), E>,
) -> Result<(), CreateDirError<E>> {
    for (path, contents) in files {
        let host = path
            .split('/')
            .fold(staging.to_path_buf(), |dir, part| dir.join(part));
        write_new_file(&host, contents).map_err(|err| write_failure(err, components))?;
    }
    // `staging` is a plain name inside a canonical directory, so it is
    // canonical itself.
    validate(&CartridgeFs::new(staging.to_path_buf())).map_err(CreateDirError::Invalid)?;
    publish_dir(staging, target).map_err(|err| match err.kind() {
        ErrorKind::AlreadyExists | ErrorKind::DirectoryNotEmpty => {
            FsError::AlreadyExists(virtual_path(components))
        }
        _ => write_failure(err, components),
    })?;
    Ok(())
}

/// Creates `path` and the directories above it, failing if the file exists.
fn write_new_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

/// Creates a fresh staging directory for `name` inside `parent`.
fn create_staging(parent: &Path, name: &str) -> io::Result<PathBuf> {
    loop {
        let path = staging_path(parent, name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
}

/// Creates a fresh staging file for `name` inside `dir`.
fn create_staging_file(dir: &Path, name: &str) -> io::Result<(PathBuf, fs::File)> {
    loop {
        let path = staging_path(dir, name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
}

/// A unique staging name for `name` inside `parent`.
fn staging_path(parent: &Path, name: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        "{STAGING_PREFIX}{name}-{}-{nanos}-{count}",
        std::process::id()
    ))
}

/// Removes staging directories and files in `parent` that an interrupted
/// creation or replacement left behind. Failures are only logged.
fn sweep_stale_staging(parent: &Path) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let staged = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(STAGING_PREFIX));
        // `DirEntry::file_type` does not follow symlinks, so a link is
        // never mistaken for a staging entry.
        let kind = entry.file_type().ok();
        let is_dir = kind.is_some_and(|kind| kind.is_dir());
        let is_file = kind.is_some_and(|kind| kind.is_file());
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= STALE_STAGING);
        if !(staged && (is_dir || is_file) && stale) {
            continue;
        }
        let removed = if is_dir {
            fs::remove_dir_all(entry.path())
        } else {
            fs::remove_file(entry.path())
        };
        match removed {
            Ok(()) => {
                tracing::info!(path = %entry.path().display(), "removed a stale staging entry")
            }
            Err(err) => tracing::warn!(
                path = %entry.path().display(),
                error = %err,
                "failed to remove a stale staging entry"
            ),
        }
    }
}

/// Renames the directory `staging` to `target`, failing instead of
/// replacing anything already at `target`.
#[cfg(unix)]
fn publish_dir(staging: &Path, target: &Path) -> io::Result<()> {
    // Claiming the name first makes the rename replace only this call's own
    // empty placeholder: POSIX rename atomically replaces an empty directory
    // and refuses a non-empty one, so content that appears at `target` in
    // between is never overwritten.
    fs::create_dir(target)?;
    fs::rename(staging, target).inspect_err(|_| {
        // Fails, leaving it in place, once anything else is inside.
        let _ = fs::remove_dir(target);
    })
}

/// Renames the directory `staging` to `target`, failing instead of
/// replacing anything already at `target`.
#[cfg(windows)]
fn publish_dir(staging: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

    let wide = |path: &Path| -> Vec<u16> { path.as_os_str().encode_wide().chain([0]).collect() };
    let (from, to) = (wide(staging), wide(target));
    // SAFETY: `from` and `to` are NUL-terminated UTF-16 strings that outlive
    // the call. Without MOVEFILE_REPLACE_EXISTING the move fails if `target`
    // exists.
    let moved = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Logs `err` and reports a failed write against the virtual path only.
fn write_failure(err: io::Error, components: &[String]) -> FsError {
    let path = virtual_path(components);
    tracing::warn!(path = %path, error = %err, "cartridge directory write failed");
    FsError::Unwritable(path)
}

/// Whether `name` is a single plain path component on the host, so joining
/// it onto a directory adds exactly one level. Rejects `.`, `..`, host
/// separators and prefixes such as Windows drive letters.
fn is_name(name: &str) -> bool {
    !name.contains(['/', '\\', '\0'])
        && matches!(
            Path::new(name).components().collect::<Vec<_>>()[..],
            [Component::Normal(_)]
        )
}

/// `components` as a virtual path: `/` or `/a/b`.
fn virtual_path(components: &[String]) -> String {
    if components.is_empty() {
        return "/".into();
    }
    components.iter().map(|c| format!("/{c}")).collect()
}

/// Removes `.` and resolves `..` without consulting the filesystem.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn lookup_error(err: io::Error) -> Lookup {
    match err.kind() {
        ErrorKind::NotFound | ErrorKind::NotADirectory => Lookup::Missing,
        _ => Lookup::Io(err),
    }
}

/// Logs `err` and reports it against the virtual path only.
fn io_failure(err: io::Error, components: &[String]) -> FsError {
    let path = virtual_path(components);
    tracing::debug!(path = %path, error = %err, "cartridge directory access failed");
    match err.kind() {
        ErrorKind::NotFound => FsError::NotFound(path),
        ErrorKind::NotADirectory => FsError::NotADirectory(path),
        _ => FsError::Unreadable(path),
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{CartridgeFs, CartridgeStorage};

    /// A scratch data directory whose cartridge root holds:
    ///
    /// ```text
    /// /games/owl/
    /// /tools/
    /// /Zeta/
    /// /A.txt  /b.txt  /readme
    /// ```
    ///
    /// plus an `outside/` directory next to the data directory. Removed on
    /// drop.
    pub(crate) struct Fixture {
        base: PathBuf,
    }

    impl Fixture {
        pub(crate) fn new(name: &str) -> Self {
            let base =
                std::env::temp_dir().join(format!("glaux-test-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&base);
            let root = base.join("data").join(super::CARTRIDGE_DIR);
            for dir in ["games/owl", "tools", "Zeta"] {
                fs::create_dir_all(root.join(dir)).unwrap();
            }
            for file in ["A.txt", "b.txt", "readme"] {
                fs::write(root.join(file), "").unwrap();
            }
            fs::create_dir_all(base.join("outside")).unwrap();
            fs::write(base.join("outside/secret"), "").unwrap();
            Self { base }
        }

        pub(crate) fn data_dir(&self) -> PathBuf {
            self.base.join("data")
        }

        pub(crate) fn open(&self) -> CartridgeFs {
            CartridgeFs::open(&self.data_dir()).unwrap()
        }

        pub(crate) fn storage(&self) -> CartridgeStorage {
            CartridgeStorage::Available(self.open())
        }

        /// Host path of the cartridge root.
        pub(crate) fn root(&self) -> PathBuf {
            fs::canonicalize(self.base.join("data").join(super::CARTRIDGE_DIR)).unwrap()
        }

        pub(crate) fn outside(&self) -> PathBuf {
            fs::canonicalize(self.base.join("outside")).unwrap()
        }

        pub(crate) fn base(&self) -> &Path {
            &self.base
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::Fixture;
    use super::*;
    use pretty_assertions::assert_eq;

    fn names(listing: &[DirEntry]) -> Vec<String> {
        listing
            .iter()
            .map(|entry| match entry.kind {
                EntryKind::Directory => format!("{}/", entry.name),
                EntryKind::File => entry.name.clone(),
            })
            .collect()
    }

    fn ls(fs: &CartridgeFs, path: Option<&str>) -> Vec<String> {
        names(&fs.list(path).unwrap())
    }

    fn not_found(path: &str) -> FsError {
        FsError::NotFound(path.into())
    }

    #[test]
    fn open_creates_the_cartridge_directory() {
        let fixture = Fixture::new("open-creates");
        let data = fixture.base().join("fresh");
        let fs = CartridgeFs::open(&data).unwrap();
        assert!(data.join(CARTRIDGE_DIR).is_dir());
        assert_eq!(fs.list(None).unwrap(), []);
    }

    #[test]
    fn storage_without_a_data_directory_is_unavailable() {
        assert_eq!(CartridgeStorage::open(None), CartridgeStorage::Unavailable);
    }

    #[test]
    fn storage_that_cannot_be_created_is_unavailable() {
        let fixture = Fixture::new("storage-blocked");
        let blocker = fixture.base().join("not-a-dir");
        fs::write(&blocker, "").unwrap();
        assert_eq!(
            CartridgeStorage::open(Some(&blocker)),
            CartridgeStorage::Unavailable
        );
    }

    #[test]
    fn storage_opens_at_root() {
        let fixture = Fixture::new("storage-open");
        let storage = CartridgeStorage::open(Some(&fixture.data_dir()));
        assert_eq!(storage, fixture.storage());
        assert_eq!(storage.cwd(), Ok("/".into()));
    }

    #[test]
    fn unavailable_storage_refuses_every_request() {
        let mut storage = CartridgeStorage::Unavailable;
        assert_eq!(storage.cwd(), Err(FsError::Unavailable));
        assert_eq!(storage.list(None), Err(FsError::Unavailable));
        assert_eq!(storage.change_dir("/"), Err(FsError::Unavailable));
        assert_eq!(
            FsError::Unavailable.to_string(),
            "Cartridge storage unavailable"
        );
    }

    #[test]
    fn starts_at_root() {
        let fs = CartridgeFs::new(PathBuf::from("/nowhere"));
        assert_eq!(fs.cwd(), "/");
    }

    #[test]
    fn cd_relative_and_absolute() {
        let fixture = Fixture::new("cd-paths");
        let mut fs = fixture.open();
        fs.change_dir("games").unwrap();
        fs.change_dir("owl").unwrap();
        assert_eq!(fs.cwd(), "/games/owl");
        fs.change_dir("/tools").unwrap();
        assert_eq!(fs.cwd(), "/tools");
        fs.change_dir("/games/owl/").unwrap();
        assert_eq!(fs.cwd(), "/games/owl");
    }

    #[test]
    fn dot_and_dot_dot() {
        let fixture = Fixture::new("dots");
        let mut fs = fixture.open();
        fs.change_dir("games/./owl/..").unwrap();
        assert_eq!(fs.cwd(), "/games");
        fs.change_dir(".").unwrap();
        assert_eq!(fs.cwd(), "/games");
        fs.change_dir("..").unwrap();
        assert_eq!(fs.cwd(), "/");
        fs.change_dir("games//owl").unwrap();
        assert_eq!(fs.cwd(), "/games/owl");
    }

    #[test]
    fn dot_dot_never_leaves_root() {
        let fixture = Fixture::new("above-root");
        let mut fs = fixture.open();
        fs.change_dir("..").unwrap();
        assert_eq!(fs.cwd(), "/");
        fs.change_dir("../../..").unwrap();
        assert_eq!(fs.cwd(), "/");
        fs.change_dir("/../games").unwrap();
        assert_eq!(fs.cwd(), "/games");
        fs.change_dir("../../../tools").unwrap();
        assert_eq!(fs.cwd(), "/tools");
        assert_eq!(ls(&fs, Some("../../..")), ls(&fs, Some("/")));
        // The host directory above the root is not reachable by name either.
        assert_eq!(fs.change_dir("/../outside"), Err(not_found("/outside")));
    }

    #[test]
    fn listing_puts_directories_first_and_ignores_case() {
        let fixture = Fixture::new("listing");
        let mut fs = fixture.open();
        let expected = ["games/", "tools/", "Zeta/", "A.txt", "b.txt", "readme"];
        assert_eq!(ls(&fs, None), expected);
        assert_eq!(ls(&fs, Some("/")), expected);
        assert_eq!(ls(&fs, Some("games")), ["owl/"]);
        assert_eq!(ls(&fs, Some("games/owl")), Vec::<String>::new());

        fs.change_dir("games").unwrap();
        assert_eq!(ls(&fs, Some("..")), expected);
        assert_eq!(fs.cwd(), "/games", "ls does not move");
    }

    #[test]
    fn listing_order_is_total() {
        let fixture = Fixture::new("listing-ties");
        let fs = fixture.open();
        for name in ["readme", "README", "Readme"] {
            fs::write(fixture.root().join("tools").join(name), "").unwrap();
        }
        // Case-insensitive ties fall back to the exact name.
        assert_eq!(ls(&fs, Some("tools")), ["README", "Readme", "readme"]);
    }

    #[test]
    fn nonexistent_paths_are_not_found() {
        let fixture = Fixture::new("missing");
        let mut fs = fixture.open();
        fs.change_dir("games").unwrap();
        assert_eq!(fs.change_dir("nope"), Err(not_found("/games/nope")));
        assert_eq!(
            fs.change_dir("/nope/deeper"),
            Err(not_found("/nope/deeper"))
        );
        assert_eq!(fs.list(Some("nope")), Err(not_found("/games/nope")));
        assert_eq!(fs.cwd(), "/games");
    }

    #[test]
    fn files_are_not_directories() {
        let fixture = Fixture::new("files");
        let mut fs = fixture.open();
        let err = FsError::NotADirectory("/b.txt".into());
        assert_eq!(fs.change_dir("b.txt"), Err(err.clone()));
        assert_eq!(fs.list(Some("/b.txt")), Err(err));
        assert_eq!(fs.change_dir("b.txt/x"), Err(not_found("/b.txt/x")));
        assert_eq!(fs.cwd(), "/");
    }

    #[test]
    fn names_with_host_separators_are_not_found() {
        let fixture = Fixture::new("separators");
        let mut fs = fixture.open();
        assert_eq!(fs.change_dir(r"games\owl"), Err(not_found(r"/games\owl")));
        assert_eq!(fs.change_dir(r"..\.."), Err(not_found(r"/..\..")));
        assert_eq!(fs.cwd(), "/");
    }

    #[test]
    fn errors_show_virtual_paths() {
        let err = FsError::NotFound("/games/nope".into());
        assert_eq!(err.to_string(), "No such file or directory: /games/nope");
        let err = FsError::NotADirectory("/b.txt".into());
        assert_eq!(err.to_string(), "Not a directory: /b.txt");
    }

    #[test]
    fn subdir_is_rooted_at_the_directory() {
        let fixture = Fixture::new("subdir");
        fs::write(fixture.root().join("games/owl/cart.json"), "{}").unwrap();
        let mut fs = fixture.open();
        fs.change_dir("games").unwrap();

        let (path, owl) = fs.subdir("owl").unwrap();
        assert_eq!(path, "/games/owl");
        assert_eq!(owl.cwd(), "/");
        assert_eq!(ls(&owl, None), ["cart.json"]);
        // `..` stops at the new root.
        assert_eq!(ls(&owl, Some("../..")), ["cart.json"]);
        assert_eq!(owl.read_file("/../../readme"), Err(not_found("/readme")));

        assert_eq!(fs.subdir("/").map(|(path, _)| path), Ok("/".into()));
        assert_eq!(fs.subdir("nope").map(drop), Err(not_found("/games/nope")));
        assert_eq!(
            fs.subdir("/b.txt").map(drop),
            Err(FsError::NotADirectory("/b.txt".into()))
        );
        assert_eq!(
            CartridgeStorage::Unavailable.subdir("/").map(drop),
            Err(FsError::Unavailable)
        );
    }

    #[test]
    fn real_subdir_is_rooted_at_the_directory() {
        let fixture = Fixture::new("real-subdir");
        fs::write(fixture.root().join("games/owl/cart.json"), "{}").unwrap();
        let fs = fixture.open();

        let games = fs.real_subdir("games").unwrap();
        assert_eq!(ls(&games, None), ["owl/"]);
        assert_eq!(games.read_file("/../readme"), Err(not_found("/readme")));
        assert_eq!(fs.real_subdir("nope").map(drop), Err(not_found("/nope")));
        assert_eq!(
            fs.real_subdir("b.txt").map(drop),
            Err(FsError::NotADirectory("/b.txt".into()))
        );
        for name in ["", ".", "..", "games/owl", r"games\owl"] {
            assert!(
                matches!(fs.real_subdir(name), Err(FsError::NotFound(_))),
                "{name:?}"
            );
        }
    }

    #[test]
    fn files_are_read_only_when_regular() {
        let fixture = Fixture::new("read-file");
        fs::write(fixture.root().join("tools/note"), "hello").unwrap();
        let fs = fixture.open();

        assert_eq!(fs.read_file("tools/note"), Ok(b"hello".to_vec()));
        assert_eq!(fs.ensure_file("/tools/note"), Ok(()));
        let not_a_file = FsError::NotAFile("/tools".into());
        assert_eq!(fs.read_file("tools"), Err(not_a_file.clone()));
        assert_eq!(fs.ensure_file("tools"), Err(not_a_file));
        assert_eq!(fs.read_file("/"), Err(FsError::NotAFile("/".into())));
        assert_eq!(fs.read_file("tools/nope"), Err(not_found("/tools/nope")));
        assert_eq!(fs.ensure_file("b.txt/x"), Err(not_found("/b.txt/x")));
        assert_eq!(not_found("/x").to_string(), "No such file or directory: /x");
        assert_eq!(FsError::NotAFile("/x".into()).to_string(), "Not a file: /x");
    }

    const TREE: &[(&str, &[u8])] = &[("top.txt", b"top"), ("code/main.luau", b"main")];

    /// Creates `name` holding [`TREE`] in `fs`, accepting it unchecked.
    fn create(fs: &CartridgeFs, name: &str) -> Result<String, CreateDirError<()>> {
        fs.create_dir_with(name, TREE, |_| Ok(()))
    }

    /// Every entry name directly in the host directory `dir`, sorted.
    fn host_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn create_dir_with_publishes_the_tree_in_the_current_directory() {
        let fixture = Fixture::new("create-dir");
        let mut fs = fixture.open();
        fs.change_dir("games").unwrap();
        let mut seen = None;
        let created = fs.create_dir_with("new", TREE, |staged| {
            seen = Some(staged.read_file("/code/main.luau"));
            Ok::<(), ()>(())
        });
        assert_eq!(created, Ok("/games/new".into()));
        assert_eq!(seen, Some(Ok(b"main".to_vec())));
        assert_eq!(fs.read_file("new/top.txt"), Ok(b"top".to_vec()));
        assert_eq!(fs.read_file("new/code/main.luau"), Ok(b"main".to_vec()));
        assert_eq!(host_names(&fixture.root().join("games")), ["new", "owl"]);
    }

    #[test]
    fn create_dir_with_never_replaces_an_entry() {
        let fixture = Fixture::new("create-dir-exists");
        let fs = fixture.open();
        fs::write(fixture.root().join("games/owl/keep"), "kept").unwrap();
        for name in ["games", "b.txt"] {
            assert_eq!(
                create(&fs, name),
                Err(CreateDirError::Fs(FsError::AlreadyExists(format!(
                    "/{name}"
                ))))
            );
        }
        assert_eq!(
            fs::read(fixture.root().join("games/owl/keep")).unwrap(),
            b"kept"
        );
        assert_eq!(fs::read(fixture.root().join("b.txt")).unwrap(), b"");
        assert_eq!(host_names(&fixture.root().join("games")), ["owl"]);
    }

    #[test]
    fn an_entry_appearing_during_creation_is_kept() {
        let fixture = Fixture::new("create-dir-race");
        let fs = fixture.open();
        let target = fixture.root().join("new");
        // Another creator takes the name while the tree is being staged.
        let created = fs.create_dir_with("new", TREE, |_| {
            fs::create_dir(&target).unwrap();
            fs::write(target.join("theirs"), "theirs").unwrap();
            Ok::<(), ()>(())
        });
        assert_eq!(
            created,
            Err(CreateDirError::Fs(FsError::AlreadyExists("/new".into())))
        );
        assert_eq!(host_names(&target), ["theirs"]);
        assert_eq!(fs::read(target.join("theirs")).unwrap(), b"theirs");
        assert!(
            !host_names(&fixture.root())
                .iter()
                .any(|n| n.starts_with(STAGING_PREFIX))
        );
    }

    #[test]
    fn failed_creations_leave_nothing_behind() {
        let fixture = Fixture::new("create-dir-fail");
        let fs = fixture.open();
        let before = host_names(&fixture.root());

        let rejected = fs.create_dir_with("new", TREE, |_| Err("bad"));
        assert_eq!(rejected, Err(CreateDirError::Invalid("bad")));

        // `code` is written as a file, so `code/main.luau` cannot be.
        let clash: &[(&str, &[u8])] = &[("code", b""), ("code/main.luau", b"")];
        assert_eq!(
            fs.create_dir_with("new", clash, |_| Ok::<(), ()>(())),
            Err(CreateDirError::Fs(FsError::Unwritable("/new".into())))
        );
        assert_eq!(host_names(&fixture.root()), before);
    }

    #[test]
    fn create_dir_with_rejects_names_that_are_not_one_component() {
        let fixture = Fixture::new("create-dir-names");
        let fs = fixture.open();
        for name in ["", ".", "..", "a/b", r"a\b", ".glaux-staging-x"] {
            assert!(
                matches!(
                    create(&fs, name),
                    Err(CreateDirError::Fs(FsError::Unwritable(_)))
                ),
                "{name:?}"
            );
        }
        let escaping: &[(&str, &[u8])] = &[("../out", b"")];
        assert!(matches!(
            fs.create_dir_with("new", escaping, |_| Ok::<(), ()>(())),
            Err(CreateDirError::Fs(FsError::Unwritable(_)))
        ));
        assert!(!fixture.root().join("new").exists());
        assert!(!fixture.root().join("../out").exists());
        assert_eq!(
            CartridgeStorage::Unavailable.create_dir_with("new", TREE, |_| Ok::<(), ()>(())),
            Err(CreateDirError::Fs(FsError::Unavailable))
        );
    }

    #[test]
    fn staging_directories_are_hidden_and_stale_ones_are_swept() {
        let fixture = Fixture::new("create-dir-stale");
        let fs = fixture.open();
        let stale = fixture.root().join(format!("{STAGING_PREFIX}old-1-2-3"));
        let fresh = fixture.root().join(format!("{STAGING_PREFIX}busy-1-2-3"));
        fs::create_dir_all(stale.join("code")).unwrap();
        fs::create_dir(&fresh).unwrap();
        let old = SystemTime::now() - STALE_STAGING - Duration::from_secs(60);
        fs::File::open(&stale).unwrap().set_modified(old).unwrap();
        assert_eq!(
            ls(&fs, None),
            ["games/", "tools/", "Zeta/", "A.txt", "b.txt", "readme"]
        );

        create(&fs, "new").unwrap();
        assert!(!stale.exists());
        assert!(
            fresh.exists(),
            "a recent staging directory may still be in use"
        );
    }

    /// Names in the host directory `dir` that are staging entries.
    fn staged(dir: &Path) -> Vec<String> {
        host_names(dir)
            .into_iter()
            .filter(|name| name.starts_with(STAGING_PREFIX))
            .collect()
    }

    #[test]
    fn replace_file_swaps_contents_that_match_the_expectation() {
        let fixture = Fixture::new("replace");
        let fs = fixture.open();
        fs::write(fixture.root().join("tools/data"), b"old").unwrap();
        assert_eq!(
            fs.replace_file("tools/data", b"new contents", Some(b"old")),
            Ok(())
        );
        assert_eq!(fs.read_file("tools/data"), Ok(b"new contents".to_vec()));
        assert_eq!(staged(&fixture.root().join("tools")), Vec::<String>::new());
    }

    #[test]
    fn replace_file_creates_a_file_expected_to_be_absent() {
        let fixture = Fixture::new("replace-create");
        let fs = fixture.open();
        assert_eq!(fs.replace_file("/tools/fresh", b"hi", None), Ok(()));
        assert_eq!(fs.read_file("/tools/fresh"), Ok(b"hi".to_vec()));
        assert_eq!(
            fs.replace_file("/nope/fresh", b"hi", None),
            Err(ReplaceError::Fs(not_found("/nope")))
        );
    }

    #[test]
    fn replace_file_refuses_contents_that_changed() {
        let fixture = Fixture::new("replace-changed");
        let fs = fixture.open();
        let path = fixture.root().join("tools/data");
        fs::write(&path, b"theirs").unwrap();
        assert_eq!(
            fs.replace_file("tools/data", b"mine", Some(b"old")),
            Err(ReplaceError::Changed(Some(b"theirs".to_vec())))
        );
        // Absent and empty are different states.
        assert_eq!(
            fs.replace_file("tools/data", b"mine", None),
            Err(ReplaceError::Changed(Some(b"theirs".to_vec())))
        );
        fs::write(&path, b"").unwrap();
        assert_eq!(
            fs.replace_file("tools/data", b"mine", None),
            Err(ReplaceError::Changed(Some(Vec::new())))
        );
        fs::remove_file(&path).unwrap();
        assert_eq!(
            fs.replace_file("tools/data", b"mine", Some(b"")),
            Err(ReplaceError::Changed(None))
        );
        assert!(!path.exists());
        assert_eq!(staged(&fixture.root().join("tools")), Vec::<String>::new());
    }

    #[test]
    fn replace_file_refuses_directories_and_staging_names() {
        let fixture = Fixture::new("replace-kinds");
        let fs = fixture.open();
        assert_eq!(
            fs.replace_file("tools", b"x", None),
            Err(ReplaceError::Fs(FsError::NotAFile("/tools".into())))
        );
        assert_eq!(
            fs.replace_file("/", b"x", None),
            Err(ReplaceError::Fs(FsError::NotAFile("/".into())))
        );
        let staging = format!("{STAGING_PREFIX}x");
        assert!(matches!(
            fs.replace_file(&staging, b"x", None),
            Err(ReplaceError::Fs(FsError::Unwritable(_)))
        ));
        assert!(!fixture.root().join(staging).exists());
    }

    #[test]
    fn stale_staging_files_are_swept_by_the_next_replace() {
        let fixture = Fixture::new("replace-stale");
        let fs = fixture.open();
        let stale = fixture.root().join(format!("{STAGING_PREFIX}data-1-2-3"));
        let fresh = fixture.root().join(format!("{STAGING_PREFIX}data-4-5-6"));
        fs::write(&stale, b"").unwrap();
        fs::write(&fresh, b"").unwrap();
        let old = SystemTime::now() - STALE_STAGING - Duration::from_secs(60);
        fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert!(!ls(&fs, None).iter().any(|n| n.starts_with(STAGING_PREFIX)));

        fs.replace_file("data", b"x", None).unwrap();
        assert!(!stale.exists());
        assert!(fresh.exists());
    }

    #[cfg(unix)]
    mod symlinks {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::os::unix::fs::symlink;

        #[test]
        fn creation_never_replaces_a_link() {
            let fixture = Fixture::new("create-dir-link");
            let root = fixture.root();
            symlink(fixture.outside(), root.join("out")).unwrap();
            symlink("missing", root.join("dangling")).unwrap();
            let fs = fixture.open();
            for name in ["out", "dangling"] {
                assert_eq!(
                    create(&fs, name),
                    Err(CreateDirError::Fs(FsError::AlreadyExists(format!(
                        "/{name}"
                    ))))
                );
            }
            assert_eq!(host_names(&fixture.outside()), ["secret"]);
            assert!(
                fs::symlink_metadata(root.join("dangling"))
                    .unwrap()
                    .is_symlink()
            );
        }

        #[test]
        fn links_leaving_the_root_are_hidden_and_unreachable() {
            let fixture = Fixture::new("links-escape");
            let root = fixture.root();
            symlink(fixture.outside(), root.join("abs-out")).unwrap();
            symlink("../../outside", root.join("rel-out")).unwrap();
            symlink("..", root.join("parent")).unwrap();
            symlink(fixture.outside().join("secret"), root.join("secret")).unwrap();
            let mut fs = fixture.open();

            for name in ["abs-out", "rel-out", "parent", "secret"] {
                let path = format!("/{name}");
                assert_eq!(fs.change_dir(name), Err(not_found(&path)), "{name}");
                assert_eq!(fs.list(Some(name)), Err(not_found(&path)), "{name}");
            }
            assert_eq!(
                fs.change_dir("abs-out/.."),
                Ok(()),
                "lexical `..` cancels the link before it is used"
            );
            assert_eq!(fs.cwd(), "/");
            assert_eq!(
                ls(&fs, None),
                ["games/", "tools/", "Zeta/", "A.txt", "b.txt", "readme"]
            );
        }

        #[test]
        fn dangling_and_looping_links_are_hidden_and_unreachable() {
            let fixture = Fixture::new("links-broken");
            let root = fixture.root();
            symlink("missing", root.join("dangling")).unwrap();
            symlink("loop-b", root.join("loop-a")).unwrap();
            symlink("loop-a", root.join("loop-b")).unwrap();
            let mut fs = fixture.open();

            for name in ["dangling", "loop-a", "loop-b"] {
                assert_eq!(fs.change_dir(name), Err(not_found(&format!("/{name}"))));
            }
            assert_eq!(
                ls(&fs, None),
                ["games/", "tools/", "Zeta/", "A.txt", "b.txt", "readme"]
            );
        }

        #[test]
        fn subdir_confines_links_to_the_directory() {
            let fixture = Fixture::new("links-subdir");
            let root = fixture.root();
            symlink(root.join("b.txt"), root.join("games/owl/b-abs")).unwrap();
            symlink("../../b.txt", root.join("games/owl/b-rel")).unwrap();
            symlink("../../tools", root.join("games/owl/tools")).unwrap();
            fs::write(root.join("games/owl/own"), "").unwrap();
            symlink("own", root.join("games/owl/own-link")).unwrap();
            symlink("games/owl", root.join("owl-link")).unwrap();
            let fs = fixture.open();

            // Reached through a link, the subdirectory is rooted at its target.
            let (path, owl) = fs.subdir("owl-link").unwrap();
            assert_eq!(path, "/owl-link");
            assert_eq!(owl, fs.subdir("games/owl").unwrap().1);

            assert_eq!(ls(&owl, None), ["own", "own-link"]);
            assert_eq!(owl.ensure_file("own-link"), Ok(()));
            for name in ["b-abs", "b-rel", "tools"] {
                assert_eq!(owl.ensure_file(name), Err(not_found(&format!("/{name}"))));
            }
            assert_eq!(owl.read_file("tools/b.txt"), Err(not_found("/tools/b.txt")));
        }

        #[test]
        fn real_subdir_does_not_follow_links() {
            let fixture = Fixture::new("links-real-subdir");
            let root = fixture.root();
            symlink("games", root.join("games-link")).unwrap();
            symlink(fixture.outside(), root.join("out")).unwrap();
            let fs = fixture.open();

            for name in ["games-link", "out"] {
                assert_eq!(
                    fs.real_subdir(name).map(drop),
                    Err(not_found(&format!("/{name}")))
                );
            }
        }

        #[test]
        fn special_files_are_not_files() {
            let fixture = Fixture::new("socket");
            let _listener =
                std::os::unix::net::UnixListener::bind(fixture.root().join("sock")).unwrap();
            let fs = fixture.open();
            assert_eq!(fs.read_file("sock"), Err(FsError::NotAFile("/sock".into())));
            assert_eq!(
                fs.ensure_file("sock"),
                Err(FsError::NotAFile("/sock".into()))
            );
        }

        #[test]
        fn replace_file_writes_through_links_inside_the_root() {
            let fixture = Fixture::new("replace-link");
            let root = fixture.root();
            fs::write(root.join("tools/real"), b"old").unwrap();
            symlink("tools/real", root.join("alias")).unwrap();
            let fs = fixture.open();
            fs.replace_file("alias", b"new", Some(b"old")).unwrap();
            assert!(
                fs::symlink_metadata(root.join("alias"))
                    .unwrap()
                    .is_symlink()
            );
            assert_eq!(fs::read(root.join("tools/real")).unwrap(), b"new");
        }

        #[test]
        fn replace_file_never_replaces_unreachable_links() {
            let fixture = Fixture::new("replace-escape");
            let root = fixture.root();
            symlink(fixture.outside().join("secret"), root.join("out")).unwrap();
            symlink("missing", root.join("dangling")).unwrap();
            let fs = fixture.open();
            for name in ["out", "dangling"] {
                assert_eq!(
                    fs.replace_file(name, b"x", None),
                    Err(ReplaceError::Fs(FsError::Unwritable(format!("/{name}")))),
                    "{name}"
                );
                assert!(fs::symlink_metadata(root.join(name)).unwrap().is_symlink());
            }
            assert_eq!(fs::read(fixture.outside().join("secret")).unwrap(), b"");
            assert_eq!(staged(&root), Vec::<String>::new());
        }

        /// Sets `path`'s Unix mode, restoring `restore` on drop so the
        /// fixture can be removed.
        struct Mode {
            path: PathBuf,
            restore: u32,
        }

        impl Mode {
            fn set(path: &Path, mode: u32, restore: u32) -> Self {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
                Self {
                    path: path.to_owned(),
                    restore,
                }
            }
        }

        impl Drop for Mode {
            fn drop(&mut self) {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.restore));
            }
        }

        /// Whether this process can create files in `dir` despite its mode,
        /// as root can. Permission tests are meaningless then.
        fn bypasses_permissions(dir: &Path) -> bool {
            let probe = dir.join("probe");
            let created = fs::write(&probe, b"").is_ok();
            let _ = fs::remove_file(&probe);
            created
        }

        #[test]
        fn a_read_only_file_is_never_replaced() {
            use std::os::unix::fs::MetadataExt;
            let fixture = Fixture::new("replace-read-only");
            let path = fixture.root().join("tools/data");
            fs::write(&path, b"old").unwrap();
            let inode = fs::metadata(&path).unwrap().ino();
            let _mode = Mode::set(&path, 0o444, 0o644);
            let fs = fixture.open();
            // The directory stays writable, so a rename could replace the
            // file; the read-only flag alone keeps it.
            assert_eq!(
                fs.replace_file("tools/data", b"new", Some(b"old")),
                Err(ReplaceError::Fs(FsError::ReadOnly("/tools/data".into())))
            );
            assert_eq!(fs::read(&path).unwrap(), b"old");
            assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
            assert_eq!(staged(&fixture.root().join("tools")), Vec::<String>::new());
        }

        #[test]
        fn a_read_only_directory_is_unwritable() {
            let fixture = Fixture::new("replace-read-only-dir");
            let dir = fixture.root().join("tools");
            fs::write(dir.join("data"), b"old").unwrap();
            let _mode = Mode::set(&dir, 0o555, 0o755);
            if bypasses_permissions(&dir) {
                return;
            }
            let fs = fixture.open();
            assert_eq!(
                fs.replace_file("tools/data", b"new", Some(b"old")),
                Err(ReplaceError::Fs(FsError::Unwritable("/tools/data".into())))
            );
            assert_eq!(fs::read(dir.join("data")).unwrap(), b"old");
            assert_eq!(staged(&dir), Vec::<String>::new());
        }

        #[test]
        fn links_inside_the_root_are_followed() {
            let fixture = Fixture::new("links-inside");
            let root = fixture.root();
            symlink("games/owl", root.join("owl-link")).unwrap();
            symlink(root.join("b.txt"), root.join("tools/b-link")).unwrap();
            symlink("../games", root.join("tools/games-link")).unwrap();
            let mut fs = fixture.open();

            assert_eq!(ls(&fs, Some("tools")), ["games-link/", "b-link"]);
            assert_eq!(ls(&fs, Some("tools/games-link")), ["owl/"]);
            fs.change_dir("owl-link").unwrap();
            // The virtual path is the one navigated, not the link target.
            assert_eq!(fs.cwd(), "/owl-link");
            assert_eq!(
                fs.change_dir("/tools/b-link"),
                Err(FsError::NotADirectory("/tools/b-link".into()))
            );
        }
    }
}
