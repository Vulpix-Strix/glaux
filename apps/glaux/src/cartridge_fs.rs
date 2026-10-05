//! The cartridge directory, presented to the shell as a filesystem rooted
//! at `/`.
//!
//! Paths inside Glaux are virtual: `/`-separated components below the
//! cartridge root. Host paths never leave this module except in logs.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Component, Path, PathBuf};

use anyhow::Context as _;

/// Name of the cartridge root inside the application data directory.
const CARTRIDGE_DIR: &str = "cartridges";

/// Symlinks followed while locating one path before giving up, as with
/// POSIX `MAXSYMLINKS`. Bounds link cycles.
const MAX_SYMLINK_HOPS: usize = 40;

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
    #[error("NO SUCH FILE OR DIRECTORY: {0}")]
    NotFound(String),
    #[error("NOT A DIRECTORY: {0}")]
    NotADirectory(String),
    #[error("NOT A FILE: {0}")]
    NotAFile(String),
    #[error("CANNOT READ {0}")]
    Unreadable(String),
    #[error("CARTRIDGE STORAGE UNAVAILABLE")]
    Unavailable,
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
            let Some(name) = entry.file_name().into_string().ok().filter(|n| is_name(n)) else {
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
            "CARTRIDGE STORAGE UNAVAILABLE"
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
        assert_eq!(err.to_string(), "NO SUCH FILE OR DIRECTORY: /games/nope");
        let err = FsError::NotADirectory("/b.txt".into());
        assert_eq!(err.to_string(), "NOT A DIRECTORY: /b.txt");
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
        assert_eq!(not_found("/x").to_string(), "NO SUCH FILE OR DIRECTORY: /x");
        assert_eq!(FsError::NotAFile("/x".into()).to_string(), "NOT A FILE: /x");
    }

    #[cfg(unix)]
    mod symlinks {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::os::unix::fs::symlink;

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
