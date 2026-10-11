//! The cartridge directory, presented to the shell as a filesystem rooted
//! at `/`.
//!
//! Paths inside Glaux are virtual: `/`-separated components below the
//! cartridge root. Host paths never leave this module except in logs and
//! as the folder host file dialogs open in.

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

    /// The canonical host path of `/`, for host file dialogs to open in.
    /// It is never shown in the shell or given to cartridges.
    pub fn host_root(&self) -> &Path {
        &self.root
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
pub(crate) mod fixture;

#[cfg(test)]
mod tests;
