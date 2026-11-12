//! Cart ROM accounting.
//!
//! A development cartridge may hold files that are not part of the game,
//! such as source images, notes or tools. Cart ROM counts only the
//! distributable selection built by [`LoadedCartridge::rom_selection`], at
//! the original size of each file. The budget is [`CART_ROM_BUDGET`].
//!
//! The selection is the one place that decides what a cartridge ships, so
//! the accounting, the exporter and the resource tools all read it instead of
//! walking the directory themselves.

use std::collections::BTreeMap;

use super::map::{MapFile, MapId};
use super::{
    CartridgeError, Language, LoadedCartridge, VideoAsset, is_portable_path, is_script_of,
};
use crate::cartridge_fs::{CartridgeFs, FsError, RealEntry, RealKind};

/// Size of Cart ROM in bytes: 1 MiB.
pub const CART_ROM_BUDGET: u64 = 1_048_576;

/// How far the scan goes. These guard the scanner, they are not a limit of
/// the hardware.
#[derive(Debug, Clone, Copy)]
struct ScanLimits {
    /// Deepest directory scanned, counting the root's subdirectories as 1.
    max_depth: usize,
    /// Most files selected.
    max_files: usize,
    /// Most directory entries looked at in all, selected or not. Unlike
    /// `max_files` it also bounds the work spent on what is left out.
    max_entries: usize,
}

const ROM_LIMITS: ScanLimits = ScanLimits {
    max_depth: 16,
    max_files: 4096,
    max_entries: 16_384,
};

/// The resource directory every cartridge has without declaring it.
const ASSETS_DIR: &str = "assets";

/// Directories the manifest cannot declare as resources, because the
/// selection already treats them in their own way.
const RESERVED_DIRS: [&str; 3] = ["code", "maps", ASSETS_DIR];

/// A directory of game resources declared in `cartridge.ron`, as a single
/// name below the cartridge root.
///
/// Rejects anything that is not a plain, visible directory name: empty
/// names, paths, `.` and `..`, `\`, `:`, names starting with `.` and the
/// directories the cartridge already uses (`code`, `maps` and `assets`,
/// ignoring case so the rule holds on every host).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceDir(String);

impl ResourceDir {
    pub fn parse(name: &str) -> Result<Self, CartridgeError> {
        let reserved = RESERVED_DIRS
            .iter()
            .any(|dir| dir.eq_ignore_ascii_case(name));
        if is_portable_path(name) && !name.contains('/') && !name.starts_with('.') && !reserved {
            Ok(Self(name.to_owned()))
        } else {
            Err(CartridgeError::InvalidResourceDir(name.to_owned()))
        }
    }

    /// Parses every name, rejecting repeats (ignoring case).
    pub(super) fn parse_all(names: &[String]) -> Result<Vec<Self>, CartridgeError> {
        let mut dirs: Vec<Self> = Vec::with_capacity(names.len());
        for name in names {
            let dir = Self::parse(name)?;
            if dirs.iter().any(|d| d.0.eq_ignore_ascii_case(&dir.0)) {
                return Err(CartridgeError::InvalidResourceDir(name.clone()));
            }
            dirs.push(dir);
        }
        Ok(dirs)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a distributable file is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RomCategory {
    Manifest,
    Code,
    Graphics,
    Maps,
    Resources,
}

impl RomCategory {
    pub fn index(self) -> usize {
        self as usize
    }
}

/// One file of the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomFile {
    /// Path from the cartridge root without a leading `/`, which is the
    /// path the file keeps in the exported cartridge.
    pub path: String,
    /// Size in bytes before any compression.
    pub size: u64,
    pub category: RomCategory,
}

/// Something that stopped the scan from being certain about a path. The
/// paths are relative to the cartridge root.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RomIssue {
    /// Symbolic links are not distributed, even when they point inside the
    /// cartridge.
    #[error("Symbolic links are not distributed: {0}")]
    Symlink(String),
    #[error("Cannot read {0}")]
    Unreadable(String),
    #[error("Too deep to scan: {0}")]
    TooDeep(String),
    #[error("More than {0} files to scan")]
    FileLimit(usize),
    #[error("More than {0} directory entries to scan")]
    EntryLimit(usize),
    /// Neither a regular file nor a directory, or a name that cannot be
    /// addressed.
    #[error("Not a regular file: {0}")]
    Unsupported(String),
}

/// The files a cartridge distributes.
///
/// A selection with [`issues`](Self::issues) is incomplete: its files are
/// real, but others may be missing from it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RomSelection {
    /// Each file once, ordered by path.
    pub files: Vec<RomFile>,
    pub issues: Vec<RomIssue>,
}

/// The size of a [`RomSelection`] against the budget.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RomUsage {
    /// Bytes of the files found. Only a lower bound when the selection is
    /// incomplete.
    pub used: u64,
    /// Bytes per [`RomCategory`], indexed by [`RomCategory::index`].
    pub by_category: [u64; 5],
    pub issues: Vec<RomIssue>,
}

/// Why a cartridge cannot be exported or loaded as distributable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RomError {
    #[error("The distributable files could not all be determined")]
    IncompleteSelection { issues: Vec<RomIssue> },
    #[error("Cart ROM is {used} bytes, over the {limit} byte budget")]
    OverBudget { used: u64, limit: u64 },
}

impl RomSelection {
    pub fn usage(&self) -> RomUsage {
        let mut usage = RomUsage {
            issues: self.issues.clone(),
            ..RomUsage::default()
        };
        for file in &self.files {
            let slot = &mut usage.by_category[file.category.index()];
            *slot = slot.saturating_add(file.size);
            usage.used = usage.used.saturating_add(file.size);
        }
        usage
    }
}

impl RomUsage {
    /// Whether the scan could not settle every path, which makes
    /// [`used`](Self::used) a lower bound.
    pub fn incomplete(&self) -> bool {
        !self.issues.is_empty()
    }

    /// Bytes left in the budget, 0 once it is spent.
    pub fn available(&self) -> u64 {
        CART_ROM_BUDGET.saturating_sub(self.used)
    }

    /// Bytes over the budget, 0 while within it.
    pub fn excess(&self) -> u64 {
        self.used.saturating_sub(CART_ROM_BUDGET)
    }

    /// Whether the files found fit the budget. Says nothing about files the
    /// scan missed: use [`check`](Self::check) to decide on a cartridge.
    pub fn fits_known(&self) -> bool {
        self.used <= CART_ROM_BUDGET
    }

    /// The restriction exporting and loading a distributable cartridge
    /// apply. An incomplete selection is rejected before its size is
    /// considered, since it cannot be known to fit.
    pub fn check(&self) -> Result<(), RomError> {
        if self.incomplete() {
            return Err(RomError::IncompleteSelection {
                issues: self.issues.clone(),
            });
        }
        if !self.fits_known() {
            return Err(RomError::OverBudget {
                used: self.used,
                limit: CART_ROM_BUDGET,
            });
        }
        Ok(())
    }
}

impl LoadedCartridge {
    /// The files this cartridge distributes, with their current sizes.
    ///
    /// Selected, in this order of rules:
    /// - `cartridge.ron` and the entry script.
    /// - The scripts of the manifest's language under `code/`.
    /// - The video asset files in the root.
    /// - `map.ron` and the layers of each valid map under `maps/`.
    /// - Every file, whatever its extension, under `assets/` and the
    ///   manifest's `resources` directories.
    ///
    /// Nothing else is looked at, so development directories are never
    /// walked. Names starting with `.` are left out. Symbolic links are
    /// never selected: one inside a selected path becomes a
    /// [`RomIssue::Symlink`] instead of disappearing. A file is counted
    /// once, by its path.
    ///
    /// Only metadata is read, never file contents.
    pub fn rom_selection(&self) -> RomSelection {
        self.rom_selection_within(ROM_LIMITS)
    }

    fn rom_selection_within(&self, limits: ScanLimits) -> RomSelection {
        let mut scan = Scan {
            limits,
            examined: 0,
            fs: &self.files,
            language: self.manifest.runtime.language,
            files: BTreeMap::new(),
            issues: Vec::new(),
            full: false,
        };
        scan.file("cartridge.ron", RomCategory::Manifest);
        scan.file(&self.manifest.runtime.entry, RomCategory::Code);
        scan.code();
        for asset in VideoAsset::ALL {
            scan.file(asset.file_name(), RomCategory::Graphics);
        }
        scan.maps();
        let resources = std::iter::once(ASSETS_DIR)
            .chain(self.manifest.resources.iter().map(ResourceDir::as_str));
        for dir in resources {
            scan.resources(dir);
        }
        RomSelection {
            files: scan.files.into_values().collect(),
            issues: scan.issues,
        }
    }
}

/// State of one [`LoadedCartridge::rom_selection`].
struct Scan<'a> {
    limits: ScanLimits,
    /// Directory entries looked at so far.
    examined: usize,
    fs: &'a CartridgeFs,
    language: Language,
    files: BTreeMap<String, RomFile>,
    issues: Vec<RomIssue>,
    /// The file limit was reached, so nothing more is added.
    full: bool,
}

impl Scan<'_> {
    /// Selects the regular file at `path` if it exists.
    fn file(&mut self, path: &str, category: RomCategory) {
        match self.fs.real_entry(path) {
            Ok(None) => {}
            Ok(Some(entry)) => match entry.kind {
                RealKind::File => self.add(path, entry.len, category),
                RealKind::Symlink => self.report(RomIssue::Symlink(path.to_owned())),
                RealKind::Directory | RealKind::Other => {
                    self.report(RomIssue::Unsupported(path.to_owned()));
                }
            },
            // A link on the way to the file.
            Err(FsError::NotFound(_)) => self.report(RomIssue::Symlink(path.to_owned())),
            Err(_) => self.report(RomIssue::Unreadable(path.to_owned())),
        }
    }

    /// Records `issue` once, however many rules reach the same path.
    fn report(&mut self, issue: RomIssue) {
        if !self.issues.contains(&issue) {
            self.issues.push(issue);
        }
    }

    fn add(&mut self, path: &str, size: u64, category: RomCategory) {
        if self.files.contains_key(path) {
            return;
        }
        if self.files.len() >= self.limits.max_files {
            if !self.full {
                self.full = true;
                self.report(RomIssue::FileLimit(self.limits.max_files));
            }
            return;
        }
        self.files.insert(
            path.to_owned(),
            RomFile {
                path: path.to_owned(),
                size,
                category,
            },
        );
    }

    /// The entries of the real directory `path`, as many as the entry limit
    /// still allows. Running into the limit, or failing to read the
    /// directory, is reported and stops what is left of the scan.
    fn entries(&mut self, path: &str) -> Option<Vec<(String, RealEntry)>> {
        let room = self.limits.max_entries - self.examined;
        match self.fs.real_entries(path, room) {
            Ok(listing) => {
                self.examined += listing.entries.len();
                if listing.truncated {
                    self.full = true;
                    self.report(RomIssue::EntryLimit(self.limits.max_entries));
                }
                Some(listing.entries)
            }
            Err(_) => {
                self.report(RomIssue::Unreadable(path.to_owned()));
                None
            }
        }
    }

    /// The scripts of the manifest's language below `code/`.
    fn code(&mut self) {
        let language = self.language;
        self.directory("code", RomCategory::Code, &|name| {
            is_script_of(name, language)
        });
    }

    /// Every file below the resource directory `dir`.
    fn resources(&mut self, dir: &str) {
        self.directory(dir, RomCategory::Resources, &|_| true);
    }

    /// Selects the files below the directory `path` that `accept` takes by
    /// name, if it exists.
    fn directory(&mut self, path: &str, category: RomCategory, accept: &dyn Fn(&str) -> bool) {
        match self.fs.real_entry(path) {
            Ok(None) => {}
            Ok(Some(entry)) => match entry.kind {
                RealKind::Directory => self.descend(path, category, accept),
                RealKind::Symlink => self.report(RomIssue::Symlink(path.to_owned())),
                RealKind::File | RealKind::Other => {
                    self.report(RomIssue::Unsupported(path.to_owned()));
                }
            },
            Err(_) => self.report(RomIssue::Unreadable(path.to_owned())),
        }
    }

    /// Selects the files below the real directory `path`.
    fn descend(&mut self, path: &str, category: RomCategory, accept: &dyn Fn(&str) -> bool) {
        let Some(entries) = self.entries(path) else {
            return;
        };
        for (name, entry) in entries {
            if name.starts_with('.') || self.full {
                continue;
            }
            let child = format!("{path}/{name}");
            match entry.kind {
                RealKind::Symlink => self.report(RomIssue::Symlink(child)),
                RealKind::Directory => {
                    if child.split('/').count() > self.limits.max_depth {
                        self.report(RomIssue::TooDeep(child));
                    } else {
                        self.descend(&child, category, accept);
                    }
                }
                RealKind::File => {
                    if accept(&name) {
                        self.add(&child, entry.len, category);
                    }
                }
                RealKind::Other => {
                    if accept(&name) {
                        self.report(RomIssue::Unsupported(child));
                    }
                }
            }
        }
    }

    /// `map.ron` and the layers of every valid map below `maps/`.
    fn maps(&mut self) {
        let entries = match self.fs.real_entry("maps") {
            Ok(None) => return,
            Ok(Some(entry)) if entry.kind == RealKind::Directory => {
                let Some(entries) = self.entries("maps") else {
                    return;
                };
                entries
            }
            Ok(Some(entry)) => {
                let path = "maps".to_owned();
                self.report(match entry.kind {
                    RealKind::Symlink => RomIssue::Symlink(path),
                    _ => RomIssue::Unsupported(path),
                });
                return;
            }
            Err(_) => {
                self.report(RomIssue::Unreadable("maps".to_owned()));
                return;
            }
        };
        for (name, entry) in entries {
            if name.starts_with('.') || self.full {
                continue;
            }
            if entry.kind == RealKind::Symlink {
                self.report(RomIssue::Symlink(format!("maps/{name}")));
            } else if entry.kind == RealKind::Directory && MapId::parse(&name).is_ok() {
                for file in MapFile::ALL {
                    self.file(
                        &format!("maps/{name}/{}", file.file_name()),
                        RomCategory::Maps,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
