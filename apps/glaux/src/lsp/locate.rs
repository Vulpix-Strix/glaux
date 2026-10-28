//! Where to find the `luau-lsp` executable.
//!
//! Glaux never downloads it. Release builds ship it next to the game
//! executable. Development builds can point to any copy with an environment
//! variable, drop one in the config folder, or rely on `PATH`.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Environment variables that name the executable, strongest first. The
/// first one that is set decides, even when it is wrong: a typo should be
/// reported, not quietly replaced by another copy.
const OVERRIDES: [&str; 2] = ["GLAUX_LUAU_LSP", "LUAU_LSP"];

/// Why no executable was found, worded for the log.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub(super) struct NotFound(String);

fn binary_name() -> String {
    format!("luau-lsp{}", std::env::consts::EXE_SUFFIX)
}

/// Picks the executable from what the process can see.
///
/// The order is the overrides, the folder of the Glaux executable, the
/// `Resources` folder of a macOS bundle, `<config>/bin`, and then `PATH`.
pub(super) fn search(
    overrides: &[(&'static str, Option<OsString>)],
    exe_dir: Option<&Path>,
    config_dir: Option<&Path>,
    path_var: Option<&OsStr>,
) -> Result<PathBuf, NotFound> {
    for (name, value) in overrides {
        let Some(value) = value.as_deref().filter(|value| !value.is_empty()) else {
            continue;
        };
        let path = PathBuf::from(value);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(NotFound(format!(
                "{name} is set to {}, which is not a file",
                path.display()
            )))
        };
    }
    let file = binary_name();
    let mut places = Vec::new();
    if let Some(dir) = exe_dir {
        places.push(dir.join(&file));
        places.push(dir.join("../Resources").join(&file));
    }
    if let Some(dir) = config_dir {
        places.push(dir.join("bin").join(&file));
    }
    if let Some(found) = places.into_iter().find(|candidate| candidate.is_file()) {
        return Ok(found);
    }
    let on_path = path_var
        .into_iter()
        .flat_map(std::env::split_paths)
        .map(|dir| dir.join(&file))
        .find(|candidate| candidate.is_file());
    on_path.ok_or_else(|| {
        let bin = config_dir.map_or_else(
            || "the config folder's bin".to_owned(),
            |dir| dir.join("bin").display().to_string(),
        );
        NotFound(format!(
            "luau-lsp was not found next to Glaux, in {bin} or in PATH. \
             Set {} to its path",
            OVERRIDES[0]
        ))
    })
}

/// The executable for this process.
pub(super) fn find(config_dir: Option<&Path>) -> Result<PathBuf, NotFound> {
    let overrides: Vec<_> = OVERRIDES
        .iter()
        .map(|name| (*name, std::env::var_os(name)))
        .collect();
    let exe = std::env::current_exe().ok();
    search(
        &overrides,
        exe.as_deref().and_then(Path::parent),
        config_dir,
        std::env::var_os("PATH").as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::scratch_dir::ScratchDir;

    /// An executable named like luau-lsp inside `dir`.
    fn install(dir: &Path) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let file = dir.join(binary_name());
        fs::write(&file, "").unwrap();
        file
    }

    fn set(name: &'static str, value: &Path) -> (&'static str, Option<OsString>) {
        (name, Some(value.as_os_str().to_owned()))
    }

    #[test]
    fn an_override_beats_every_other_place() {
        let scratch = ScratchDir::new("lsp-locate-override");
        let chosen = install(&scratch.join("chosen"));
        let beside = scratch.join("exe");
        install(&beside);
        let found = search(
            &[set("GLAUX_LUAU_LSP", &chosen), ("LUAU_LSP", None)],
            Some(&beside),
            None,
            None,
        );
        assert_eq!(found, Ok(chosen));
    }

    #[test]
    fn the_first_override_that_is_set_decides_even_when_it_is_wrong() {
        let scratch = ScratchDir::new("lsp-locate-wrong");
        let good = install(&scratch.join("good"));
        let missing = scratch.join("nowhere");
        let found = search(
            &[set("GLAUX_LUAU_LSP", &missing), set("LUAU_LSP", &good)],
            None,
            None,
            None,
        );
        let reason = found.unwrap_err().to_string();
        assert!(reason.starts_with("GLAUX_LUAU_LSP is set to"), "{reason}");
        assert!(reason.contains("not a file"), "{reason}");
    }

    #[test]
    fn an_empty_override_is_ignored() {
        let scratch = ScratchDir::new("lsp-locate-empty");
        let beside = scratch.join("exe");
        let installed = install(&beside);
        let found = search(
            &[("GLAUX_LUAU_LSP", Some("".into()))],
            Some(&beside),
            None,
            None,
        );
        assert_eq!(found, Ok(installed));
    }

    #[test]
    fn places_are_tried_in_order() {
        let scratch = ScratchDir::new("lsp-locate-order");
        let exe = scratch.join("exe");
        let config = scratch.join("config");
        let path_dir = scratch.join("onpath");
        let on_path = install(&path_dir);
        let path_var = std::env::join_paths([&path_dir]).unwrap();
        let find = |exe_dir: Option<&Path>, config_dir: Option<&Path>| {
            search(&[], exe_dir, config_dir, Some(path_var.as_os_str()))
        };

        assert_eq!(find(Some(&exe), Some(&config)), Ok(on_path));
        let in_config = install(&config.join("bin"));
        assert_eq!(find(Some(&exe), Some(&config)), Ok(in_config));
        let resources = install(&exe.join("../Resources"));
        assert_eq!(find(Some(&exe), Some(&config)), Ok(resources));
        let beside = install(&exe);
        assert_eq!(find(Some(&exe), Some(&config)), Ok(beside));
    }

    #[test]
    fn path_entries_are_searched_in_order() {
        let scratch = ScratchDir::new("lsp-locate-path");
        let empty = scratch.join("empty");
        fs::create_dir_all(&empty).unwrap();
        let first = install(&scratch.join("first"));
        let second = install(&scratch.join("second"));
        let path_var =
            std::env::join_paths([&empty, &scratch.join("first"), &scratch.join("second")])
                .unwrap();
        assert_eq!(search(&[], None, None, Some(&path_var)), Ok(first));
        assert_ne!(search(&[], None, None, Some(&path_var)), Ok(second));
    }

    #[test]
    fn a_directory_named_like_the_executable_does_not_count() {
        let scratch = ScratchDir::new("lsp-locate-dir");
        let beside = scratch.join("exe");
        fs::create_dir_all(beside.join(binary_name())).unwrap();
        assert!(search(&[], Some(&beside), None, None).is_err());
    }

    #[test]
    fn the_failure_says_where_to_put_it_and_how_to_point_to_it() {
        let reason = search(&[], None, Some(Path::new("/cfg")), Some(OsStr::new("")))
            .unwrap_err()
            .to_string();
        assert!(reason.contains("/cfg/bin"), "{reason}");
        assert!(reason.contains("PATH"), "{reason}");
        assert!(reason.contains("GLAUX_LUAU_LSP"), "{reason}");
    }

    #[test]
    fn the_executable_suffix_follows_the_platform() {
        assert!(binary_name().ends_with(std::env::consts::EXE_SUFFIX));
    }
}
