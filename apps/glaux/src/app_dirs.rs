//! Platform directories Glaux keeps its data in.

#![warn(unreachable_pub)]

use std::path::PathBuf;

use directories::{BaseDirs, ProjectDirs};

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("dev", "cuervolu", "Glaux")
}

/// Glaux's platform data directory, if the platform provides one.
pub(crate) fn data_dir() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.data_dir().to_path_buf())
}

/// Glaux's platform configuration directory, if the platform provides one.
pub(crate) fn config_dir() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.config_dir().to_path_buf())
}

/// Glaux's platform cache directory, if the platform provides one. Anything
/// kept there can be rebuilt.
pub(crate) fn cache_dir() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.cache_dir().to_path_buf())
}

/// The user's runtime directory (`$XDG_RUNTIME_DIR`), if set. Only Linux
/// provides one.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn runtime_dir() -> Option<PathBuf> {
    BaseDirs::new().and_then(|dirs| dirs.runtime_dir().map(PathBuf::from))
}
