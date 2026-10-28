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

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

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

/// The file whose lock says a workspace belongs to a running Glaux.
const LOCK_FILE: &str = ".lock";

/// A temporary directory of placeholders for one cartridge.
///
/// The directory holds a lock file, locked by the process that made it for
/// as long as the [`Shadow`] lives. The operating system releases the lock
/// when the process ends, however it ends, which is how [`clean_abandoned`]
/// tells a workspace left behind from one another Glaux is using.
pub(super) struct Shadow {
    root: PathBuf,
    /// Held to keep the lock. Dropped before the directory is removed.
    lock: Option<File>,
}

impl Shadow {
    /// An empty workspace at `root`, replacing whatever was there.
    pub fn create(root: PathBuf) -> io::Result<Self> {
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(LOCK_FILE))
            .ok();
        // Where locking is not supported the workspace is left unlocked, and
        // `clean_abandoned` then never removes it.
        let lock = lock.filter(|file| file.try_lock().is_ok());
        Ok(Self { root, lock })
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
        // Windows will not remove a directory with an open file in it.
        drop(self.lock.take());
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// How long a workspace must have gone untouched to be taken for left behind.
/// A workspace is locked a moment after it is made, and this keeps one that
/// is still being made safe.
pub(super) const ABANDONED_AFTER: Duration = Duration::from_secs(60);

/// Removes the workspaces under `workspaces` that no running Glaux owns,
/// and returns how many.
///
/// A workspace goes only if its lock is free, which means whoever made it is
/// gone, and nothing in it changed for `grace`. One whose lock is held, or
/// that cannot be checked, stays.
pub(super) fn clean_abandoned(workspaces: &Path, grace: Duration) -> usize {
    let Ok(entries) = fs::read_dir(workspaces) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|dir| dir.is_dir() && is_abandoned(dir, grace))
        .filter(|dir| fs::remove_dir_all(dir).is_ok())
        .count()
}

fn is_abandoned(dir: &Path, grace: Duration) -> bool {
    let idle = fs::metadata(dir)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= grace);
    if !idle {
        return false;
    }
    match OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join(LOCK_FILE))
    {
        // Free means nobody holds it. The handle is closed on return.
        Ok(lock) => lock.try_lock().is_ok(),
        // Made before workspaces had locks, or never got one.
        Err(error) => error.kind() == io::ErrorKind::NotFound,
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

    fn workspace_with_file(parent: &Path, name: &str) -> Shadow {
        let shadow = Shadow::create(parent.join(name)).unwrap();
        shadow.add(&ModuleName::parse("util").unwrap()).unwrap();
        shadow
    }

    #[test]
    fn a_workspace_in_use_is_never_cleaned() {
        let scratch = ScratchDir::new("lsp-clean-live");
        let live = workspace_with_file(&scratch, "100-1");
        assert_eq!(clean_abandoned(&scratch, Duration::ZERO), 0);
        assert!(live.root().join("util.luau").exists());
    }

    #[test]
    fn a_workspace_whose_owner_is_gone_is_cleaned() {
        let scratch = ScratchDir::new("lsp-clean-dead");
        let gone = scratch.join("200-1");
        drop(workspace_with_file(&scratch, "200-1"));
        // Dropping removes it, so leave one behind as a crash would.
        fs::create_dir_all(&gone).unwrap();
        fs::write(gone.join(LOCK_FILE), "").unwrap();
        fs::write(gone.join("util.luau"), "").unwrap();
        let kept = workspace_with_file(&scratch, "300-1");

        assert_eq!(clean_abandoned(&scratch, Duration::ZERO), 1);
        assert!(!gone.exists());
        assert!(kept.root().exists());
    }

    #[test]
    fn a_workspace_touched_recently_is_given_time() {
        let scratch = ScratchDir::new("lsp-clean-fresh");
        let fresh = scratch.join("400-1");
        fs::create_dir_all(&fresh).unwrap();
        fs::write(fresh.join(LOCK_FILE), "").unwrap();
        assert_eq!(clean_abandoned(&scratch, ABANDONED_AFTER), 0);
        assert!(fresh.exists());
        assert_eq!(clean_abandoned(&scratch, Duration::ZERO), 1);
    }

    #[test]
    fn a_directory_without_a_lock_goes_only_when_idle_and_files_are_left_alone() {
        let scratch = ScratchDir::new("lsp-clean-nolock");
        let old = scratch.join("500-1");
        fs::create_dir_all(&old).unwrap();
        fs::write(scratch.join("stray.txt"), "x").unwrap();
        assert_eq!(clean_abandoned(&scratch, ABANDONED_AFTER), 0);
        assert_eq!(clean_abandoned(&scratch, Duration::ZERO), 1);
        assert!(!old.exists());
        assert!(scratch.join("stray.txt").exists());
    }

    #[test]
    fn a_missing_folder_has_nothing_to_clean() {
        let scratch = ScratchDir::new("lsp-clean-missing");
        assert_eq!(clean_abandoned(&scratch.join("nowhere"), Duration::ZERO), 0);
    }

    /// Another process holds the lock, as a second Glaux would.
    #[cfg(unix)]
    #[test]
    fn a_workspace_another_process_holds_is_left_alone_until_it_ends() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};

        let Ok(python) = Command::new("python3").arg("--version").output() else {
            return;
        };
        if !python.status.success() {
            return;
        }
        let scratch = ScratchDir::new("lsp-clean-other");
        let other = scratch.join("600-1");
        fs::create_dir_all(&other).unwrap();
        let lock = other.join(LOCK_FILE);
        fs::write(&lock, "").unwrap();
        /// Kills the process when the test ends, even by failing.
        struct Holder(std::process::Child);
        impl Drop for Holder {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut child = Holder(Command::new("python3")
            .args([
                "-c",
                "import fcntl,sys,time\nf=open(sys.argv[1],'r+')\nfcntl.flock(f,fcntl.LOCK_EX)\nprint('locked',flush=True)\ntime.sleep(60)",
            ])
            .arg(&lock)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap());
        let mut line = String::new();
        BufReader::new(child.0.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(line.trim(), "locked");

        assert_eq!(clean_abandoned(&scratch, Duration::ZERO), 0);
        assert!(other.exists(), "the other process's workspace was removed");

        child.0.kill().unwrap();
        child.0.wait().unwrap();
        assert_eq!(clean_abandoned(&scratch, Duration::ZERO), 1);
        assert!(!other.exists());
    }

    #[test]
    fn creating_over_an_old_workspace_starts_empty_and_dropping_removes_it() {
        let scratch = ScratchDir::new("lsp-shadow-drop");
        let root = scratch.join("ws");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("old.luau"), "stale").unwrap();
        let shadow = Shadow::create(root.clone()).unwrap();
        // Only the lock file.
        let names: Vec<_> = fs::read_dir(shadow.root())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [LOCK_FILE]);
        drop(shadow);
        assert!(!root.exists());
    }
}
