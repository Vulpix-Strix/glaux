use std::fs;
use std::path::{Path, PathBuf};

use super::{CartridgeFs, CartridgeStorage};
use crate::scratch_dir::ScratchDir;

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
    base: ScratchDir,
}

impl Fixture {
    pub(crate) fn new(name: &str) -> Self {
        let base = ScratchDir::new(name);
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

    /// Whether the host file system tells names apart that differ only in
    /// case. Tests that create such names cannot run where it does not,
    /// as on the default macOS and Windows volumes.
    pub(crate) fn is_case_sensitive(&self) -> bool {
        let probe = self.base.join("case-probe");
        fs::write(&probe, "").unwrap();
        let sensitive = !self.base.join("CASE-PROBE").exists();
        fs::remove_file(probe).unwrap();
        sensitive
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
