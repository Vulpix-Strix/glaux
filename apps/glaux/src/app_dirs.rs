//! Platform directories Glaux keeps its data in.

use std::path::PathBuf;

use directories::ProjectDirs;

/// Glaux's platform data directory, if the platform provides one.
pub fn data_dir() -> Option<PathBuf> {
    ProjectDirs::from("dev", "cuervolu", "Glaux").map(|dirs| dirs.data_dir().to_path_buf())
}
