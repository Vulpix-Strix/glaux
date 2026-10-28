//! Scripts that change on disk while the analyzer is running.
//!
//! The cartridge directory is watched for as long as its session lives. What
//! the watcher reports is turned into a [`Plan`], which the actor carries
//! out, and nothing here touches the server or the cartridge.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use glaux_fswatch::{FsEvent, FsWatcher, WatchError, WatchId};
use glaux_modules::MODULE_ROOT;
use tokio::sync::Notify;

use crate::cartridge::source::SourcePath;

/// How long a path must be quiet before its change is reported. Editors
/// write a file in several steps, and this keeps the analyzer from reading
/// it halfway.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// The watches on one cartridge.
pub(super) struct Watching {
    watcher: FsWatcher,
    /// The cartridge directory, canonical, watched without descending.
    root: PathBuf,
    /// The `code` directory below it, when it exists.
    code: Option<WatchId>,
}

impl Watching {
    /// Watches the cartridge at `dir`. `woken` is notified whenever there
    /// may be something to [`poll`](Self::poll).
    pub(super) fn start(dir: &Path, woken: Arc<Notify>) -> Result<Self, WatchError> {
        let began = Instant::now();
        let mut watcher = FsWatcher::new(DEBOUNCE, move || woken.notify_one())?;
        // The root is watched on its own to notice `code` appearing or going.
        let (_, root) = watcher.watch(dir, false)?;
        let mut watching = Self {
            watcher,
            root,
            code: None,
        };
        watching.follow_code();
        tracing::debug!(elapsed = ?began.elapsed(), "the cartridge is watched");
        Ok(watching)
    }

    /// Watches `code` if it exists, and stops watching it if it does not.
    /// Called again whenever the directory may have been replaced.
    pub(super) fn follow_code(&mut self) {
        if let Some(id) = self.code.take() {
            self.watcher.unwatch(id);
        }
        let code = self.root.join(MODULE_ROOT);
        if code.is_dir() {
            match self.watcher.watch(&code, true) {
                Ok((id, _)) => self.code = Some(id),
                Err(error) => tracing::warn!(%error, "the scripts are no longer watched"),
            }
        }
    }

    pub(super) fn poll(&mut self) -> Vec<FsEvent> {
        self.watcher.poll()
    }

    pub(super) fn due(&self) -> Option<Instant> {
        self.watcher.next_due()
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }
}

/// What a batch of events asks of the actor.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Plan {
    /// Scripts to read again. One that no longer exists is gone.
    pub touched: Vec<SourcePath>,
    /// Scripts that were removed.
    pub gone: Vec<SourcePath>,
    /// `code` was created, removed or replaced.
    pub layout: bool,
    /// Compare the whole cartridge with the disk, because events may have
    /// been lost or a directory arrived with unknown contents.
    pub reconcile: bool,
}

/// Reads `events` as the changes to the cartridge at `root`.
///
/// `holds_scripts_below` says whether any known script lives under a path,
/// which is how a removed directory is told from a removed file nobody
/// cares about.
pub(super) fn plan(
    root: &Path,
    events: &[FsEvent],
    holds_scripts_below: impl Fn(&str) -> bool,
) -> Plan {
    let mut plan = Plan::default();
    let note = |plan: &mut Plan, path: &Path, appeared: bool| {
        let Some(relative) = relative(root, path) else {
            return;
        };
        if relative == MODULE_ROOT {
            plan.layout = true;
            plan.reconcile = true;
        } else if let Ok(script) = SourcePath::parse(&relative) {
            if appeared {
                plan.touched.push(script);
            } else {
                plan.gone.push(script);
            }
        } else if appeared {
            // A directory can arrive with scripts in it that no event names.
            if path.is_dir() {
                plan.reconcile = true;
            }
        } else if holds_scripts_below(&relative) {
            plan.reconcile = true;
        }
    };
    for event in events {
        match event {
            FsEvent::Created(path) => note(&mut plan, path, true),
            FsEvent::Modified(path) => {
                // A directory is modified whenever something in it is.
                if !path.is_dir() {
                    note(&mut plan, path, true);
                }
            }
            FsEvent::Removed(path) => note(&mut plan, path, false),
            FsEvent::Renamed { from, to } => {
                note(&mut plan, from, false);
                note(&mut plan, to, true);
            }
            FsEvent::Rescan(_) => plan.reconcile = true,
        }
    }
    plan.touched.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    plan.touched.dedup();
    plan.gone.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    plan.gone.dedup();
    plan
}

/// `path` below `root`, `/`-separated, or `None` if it is not below it or
/// cannot be written that way.
fn relative(root: &Path, path: &Path) -> Option<String> {
    let inside = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for part in inside.components() {
        parts.push(part.as_os_str().to_str()?);
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(relative: &str) -> PathBuf {
        Path::new("/cart").join(relative)
    }

    fn script(path: &str) -> SourcePath {
        SourcePath::parse(path).unwrap()
    }

    fn plan_of(events: &[FsEvent]) -> Plan {
        plan(Path::new("/cart"), events, |below| {
            below == "code/game" || below == "code"
        })
    }

    #[test]
    fn scripts_are_touched_or_gone_by_what_happened_to_them() {
        let plan = plan_of(&[
            FsEvent::Created(p("code/b.luau")),
            FsEvent::Modified(p("code/a.luau")),
            FsEvent::Modified(p("code/a.luau")),
            FsEvent::Removed(p("code/c.luau")),
            FsEvent::Renamed {
                from: p("code/d.luau"),
                to: p("code/e.luau"),
            },
        ]);
        assert_eq!(
            plan,
            Plan {
                touched: vec![
                    script("code/a.luau"),
                    script("code/b.luau"),
                    script("code/e.luau")
                ],
                gone: vec![script("code/c.luau"), script("code/d.luau")],
                ..Plan::default()
            }
        );
    }

    #[test]
    fn what_is_not_a_script_or_not_in_the_cartridge_is_ignored() {
        let plan = plan_of(&[
            FsEvent::Modified(p("sprites.png")),
            FsEvent::Created(p("code/notes.txt")),
            FsEvent::Removed(p("code/.main.luau.tmp")),
            FsEvent::Modified(PathBuf::from("/elsewhere/code/a.luau")),
        ]);
        assert_eq!(plan, Plan::default());
    }

    #[test]
    fn a_removed_directory_with_scripts_in_it_asks_for_a_reconciliation() {
        assert!(plan_of(&[FsEvent::Removed(p("code/game"))]).reconcile);
        assert!(!plan_of(&[FsEvent::Removed(p("code/other"))]).reconcile);
    }

    #[test]
    fn lost_events_and_a_replaced_code_directory_ask_for_a_reconciliation() {
        assert!(plan_of(&[FsEvent::Rescan(p(""))]).reconcile);
        let plan = plan_of(&[FsEvent::Created(p("code"))]);
        assert!(plan.layout && plan.reconcile);
    }

    #[test]
    fn a_directory_that_appears_is_looked_into() {
        let dir = std::env::temp_dir().join(format!("glaux-external-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("code/new")).unwrap();
        let plan = plan(&dir, &[FsEvent::Created(dir.join("code/new"))], |_| false);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(plan.reconcile);
    }
}
