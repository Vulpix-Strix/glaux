//! Scratch directories for tests that touch the host filesystem.

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};

/// An empty directory under the system temp directory, named after the
/// process and `name`. It is removed on drop, so a failing test cleans up
/// too.
pub(crate) struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    /// Keep `name` short where sockets are created inside: Unix socket paths
    /// have a small length limit.
    pub(crate) fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("glaux-test-{}-{name}", std::process::id()));
        // Left over from an earlier process that reused this pid.
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Deref for ScratchDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for ScratchDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
