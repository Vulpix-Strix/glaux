//! The workspace `luau-lsp` sees in place of the cartridge.
//!
//! Glaux resolves `require("game.enemies")` from the cartridge's `code`
//! directory, so the name is the same wherever the requiring script lives.
//! `luau-lsp` treats a string without `./` as a file name beside the
//! requiring script, and a dot as part of that name. A flat directory with
//! one file per module, `game.enemies.luau`, therefore makes both agree.
//!
//! The server only resolves a module whose file exists, so each module gets
//! an empty placeholder here. The real text, saved or not, is sent as an
//! open document, which the server prefers to the file. The cartridge is
//! never touched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use glaux_modules::{MODULE_ROOT, ModuleName};

use crate::cartridge::Language;
use crate::cartridge::source::SourcePath;

/// The name a script is required by, or `None` if no `require` can reach
/// it: it is not Luau, it is outside `code`, or its name is not a valid
/// module name.
pub(super) fn module_name(path: &SourcePath) -> Option<ModuleName> {
    if path.language() != Language::Luau {
        return None;
    }
    let inside = path
        .as_str()
        .strip_prefix(MODULE_ROOT)?
        .strip_prefix('/')?
        .strip_suffix(".luau")?;
    ModuleName::parse(&inside.replace('/', ".")).ok()
}

/// A temporary directory of placeholders for one cartridge.
pub(super) struct Shadow {
    root: PathBuf,
}

impl Shadow {
    /// An empty workspace at `root`, replacing whatever was there.
    pub fn create(root: PathBuf) -> io::Result<Self> {
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Makes the placeholder for `module`.
    pub fn add(&self, module: &ModuleName) -> io::Result<PathBuf> {
        placeholder(&self.root, module)
    }
}

/// The placeholder file of `module` in the workspace at `root`.
fn placeholder_file(root: &Path, module: &ModuleName) -> PathBuf {
    root.join(format!("{}.luau", module.as_str()))
}

/// Makes the placeholder for `module` in the workspace at `root`, if it is
/// not there yet. Blocking, like every file operation here.
pub(super) fn placeholder(root: &Path, module: &ModuleName) -> io::Result<PathBuf> {
    let file = placeholder_file(root, module);
    if !file.exists() {
        fs::write(&file, "")?;
    }
    Ok(file)
}

/// Removes the placeholder of `module` from the workspace at `root`.
pub(super) fn remove_placeholder(root: &Path, module: &ModuleName) {
    let _ = fs::remove_file(placeholder_file(root, module));
}

impl Drop for Shadow {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::scratch_dir::ScratchDir;

    fn name(path: &str) -> Option<String> {
        module_name(&SourcePath::parse(path).unwrap()).map(|m| m.as_str().to_owned())
    }

    #[test]
    fn directories_become_dots() {
        assert_eq!(name("code/util.luau").as_deref(), Some("util"));
        assert_eq!(
            name("code/game/enemies.luau").as_deref(),
            Some("game.enemies")
        );
        assert_eq!(name("code/a/b/c.luau").as_deref(), Some("a.b.c"));
    }

    #[test]
    fn scripts_no_require_can_reach_have_no_name() {
        assert_eq!(name("code/main.rhai"), None);
        assert_eq!(name("main.luau"), None);
        assert_eq!(name("lib/util.luau"), None);
        assert_eq!(name("code/Upper.luau"), None);
        assert_eq!(name("code/9lives.luau"), None);
    }

    #[test]
    fn placeholders_are_empty_files_named_after_the_module() {
        let scratch = ScratchDir::new("lsp-shadow");
        let shadow = Shadow::create(scratch.join("ws")).unwrap();
        let module = ModuleName::parse("game.enemies").unwrap();
        let file = shadow.add(&module).unwrap();
        assert_eq!(file.file_name().unwrap(), "game.enemies.luau");
        assert_eq!(fs::read_to_string(&file).unwrap(), "");
        remove_placeholder(shadow.root(), &module);
        assert!(!file.exists());
    }

    #[test]
    fn creating_over_an_old_workspace_starts_empty_and_dropping_removes_it() {
        let scratch = ScratchDir::new("lsp-shadow-drop");
        let root = scratch.join("ws");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("old.luau"), "stale").unwrap();
        let shadow = Shadow::create(root.clone()).unwrap();
        assert_eq!(fs::read_dir(shadow.root()).unwrap().count(), 0);
        drop(shadow);
        assert!(!root.exists());
    }
}
