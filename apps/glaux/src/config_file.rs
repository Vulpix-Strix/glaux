//! Writing the small files Glaux keeps in its config directory.

use std::fs;
use std::path::Path;

use anyhow::Context;

/// Writes `text` to `path`, creating its directory if needed.
///
/// The text goes to a temporary file renamed into place, so an interrupted
/// write never leaves a truncated file behind.
pub fn write_atomically(path: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    }
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = Path::new(&temp);
    fs::write(temp, text).with_context(|| format!("failed to write {}", temp.display()))?;
    fs::rename(temp, path).with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}
