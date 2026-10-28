//! Where to find the `luau-lsp` executable.
//!
//! Glaux never downloads it. Release builds ship it next to the game
//! executable, and development builds can point to any copy.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Names the executable for development, ahead of the bundled copy.
const OVERRIDES: [&str; 2] = ["GLAUX_LUAU_LSP", "LUAU_LSP"];

fn binary_name() -> String {
    format!("luau-lsp{}", std::env::consts::EXE_SUFFIX)
}

/// Every place worth looking, best first.
///
/// A macOS application bundle keeps resources beside `MacOS`, so that
/// folder is a candidate too.
pub(super) fn candidates(overrides: &[Option<OsString>], exe_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = overrides
        .iter()
        .flatten()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect();
    if let Some(dir) = exe_dir {
        found.push(dir.join(binary_name()));
        found.push(dir.join("../Resources").join(binary_name()));
    }
    found
}

/// The first candidate that is a file.
pub(super) fn find() -> Option<PathBuf> {
    let overrides: Vec<_> = OVERRIDES.iter().map(std::env::var_os).collect();
    let exe = std::env::current_exe().ok();
    candidates(&overrides, exe.as_deref().and_then(Path::parent))
        .into_iter()
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn overrides_come_first_then_the_bundled_copies() {
        let found = candidates(
            &[Some("/dev/lsp".into()), None, Some("".into())],
            Some(Path::new("/app/bin")),
        );
        let name = binary_name();
        assert_eq!(
            found,
            [
                PathBuf::from("/dev/lsp"),
                Path::new("/app/bin").join(&name),
                Path::new("/app/bin/../Resources").join(&name),
            ]
        );
    }

    #[test]
    fn without_overrides_or_a_known_directory_there_is_nothing() {
        assert_eq!(candidates(&[None, None], None), Vec::<PathBuf>::new());
    }

    #[test]
    fn the_executable_suffix_follows_the_platform() {
        assert!(binary_name().ends_with(std::env::consts::EXE_SUFFIX));
    }
}
