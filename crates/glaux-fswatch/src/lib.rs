//! Filesystem change notifications, debounced and reduced to what a
//! consumer acts on.
//!
//! [`FsWatcher`] wraps `notify` and knows nothing about the machine, the
//! editor or any server. Raw events are noisy: a save can arrive as a write,
//! as a temporary file renamed over the original, or as several of both, and
//! platforms differ in what they report. The watcher waits until a path has
//! been quiet for the debounce interval, looks at what is on disk, and
//! reports one [`FsEvent`] for it.
//!
//! `Created` and `Modified` are hints that mean the same thing to a
//! consumer: read the file now. A backend cannot always tell a new file from
//! one that was replaced, so a consumer should decide by what it already
//! knows. `Removed` is reported for any path that is gone, including
//! directories, whose files are not listed one by one.
//!
//! When events may have been lost, because the backend overflowed or
//! reported an error, the watcher says [`FsEvent::Rescan`] for the watched
//! root. The consumer then compares its own state with the disk once, which
//! makes any other polling unnecessary.
//!
//! Paths are reported as the backend sees them. The watched roots are
//! canonicalized first, so events can be matched to a root with
//! [`Path::starts_with`] on the path [`FsWatcher::watch`] returned.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};

/// What happened to a path, once it settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
    Renamed {
        from: PathBuf,
        to: PathBuf,
    },
    /// Events under this watched root may have been lost.
    Rescan(PathBuf),
}

/// Identifies a call to [`FsWatcher::watch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WatchId(u64);

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("cannot watch {path}: {source}")]
    Path {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the system watcher failed: {0}")]
    Backend(#[from] notify::Error),
}

/// What the raw events of one path add up to.
struct Touched {
    last: Instant,
    created: bool,
    removed: bool,
    renamed: bool,
    /// Where a rename that named both ends moved this path to.
    moved_to: Option<PathBuf>,
}

struct Root {
    id: WatchId,
    path: PathBuf,
}

pub struct FsWatcher {
    watcher: RecommendedWatcher,
    raw: Receiver<notify::Result<Event>>,
    roots: Vec<Root>,
    pending: HashMap<PathBuf, Touched>,
    rescans: HashMap<PathBuf, Instant>,
    debounce: Duration,
    next_id: u64,
}

impl FsWatcher {
    /// Starts a watcher that reports a path after it was quiet for
    /// `debounce`.
    ///
    /// `wake` runs on the backend's thread whenever raw events arrive, so a
    /// consumer can sleep until there is something to [`poll`](Self::poll)
    /// instead of asking on a timer. It must be quick and must not block.
    pub fn new(debounce: Duration, wake: impl Fn() + Send + 'static) -> Result<Self, WatchError> {
        let (sender, raw) = mpsc::channel();
        let watcher = RecommendedWatcher::new(
            move |event| {
                if sender.send(event).is_ok() {
                    wake();
                }
            },
            Config::default(),
        )?;
        Ok(Self {
            watcher,
            raw,
            roots: Vec::new(),
            pending: HashMap::new(),
            rescans: HashMap::new(),
            debounce,
            next_id: 0,
        })
    }

    /// Starts watching `path`, which must exist, and everything below it if
    /// `recursive`. Returns the id and the canonical path events will be
    /// reported under.
    pub fn watch(
        &mut self,
        path: &Path,
        recursive: bool,
    ) -> Result<(WatchId, PathBuf), WatchError> {
        let canonical = fs::canonicalize(path).map_err(|source| WatchError::Path {
            path: path.to_owned(),
            source,
        })?;
        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        self.watcher.watch(&canonical, mode)?;
        let id = WatchId(self.next_id);
        self.next_id += 1;
        self.roots.push(Root {
            id,
            path: canonical.clone(),
        });
        Ok((id, canonical))
    }

    /// Stops watching what `id` named. Unknown ids are ignored.
    pub fn unwatch(&mut self, id: WatchId) {
        let Some(at) = self.roots.iter().position(|root| root.id == id) else {
            return;
        };
        let root = self.roots.remove(at);
        // A root that is already gone from the disk cannot be unwatched,
        // and has nothing left to report either.
        let _ = self.watcher.unwatch(&root.path);
        self.pending.retain(|path, _| !path.starts_with(&root.path));
        self.rescans.remove(&root.path);
    }

    /// Takes the raw events that arrived and returns what settled. Never
    /// waits.
    pub fn poll(&mut self) -> Vec<FsEvent> {
        self.poll_at(Instant::now())
    }

    /// When [`poll`](Self::poll) will have something to report, if anything
    /// is waiting out its debounce interval. Raw events that were not
    /// collected yet are not counted: call `poll` first.
    pub fn next_due(&self) -> Option<Instant> {
        let paths = self.pending.values().map(|touched| touched.last);
        paths
            .chain(self.rescans.values().copied())
            .min()
            .map(|oldest| oldest + self.debounce)
    }

    fn poll_at(&mut self, now: Instant) -> Vec<FsEvent> {
        while let Ok(raw) = self.raw.try_recv() {
            self.ingest(raw, now);
        }
        self.settle(now)
    }

    fn ingest(&mut self, raw: notify::Result<Event>, now: Instant) {
        let event = match raw {
            Ok(event) => event,
            Err(error) => {
                tracing::warn!(%error, "the file watcher reported an error");
                let under = error.paths.clone();
                self.lose(&under, now);
                return;
            }
        };
        if event.need_rescan() {
            self.lose(&event.paths, now);
            return;
        }
        match event.kind {
            EventKind::Access(_) | EventKind::Other => {}
            EventKind::Create(_) => {
                for path in event.paths {
                    self.touch(path, now).created = true;
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    self.touch(path, now).removed = true;
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() == 2 => {
                let mut paths = event.paths.into_iter();
                let (from, to) = (paths.next().unwrap(), paths.next().unwrap());
                let touched = self.touch(from, now);
                touched.renamed = true;
                touched.moved_to = Some(to.clone());
                self.touch(to, now).renamed = true;
            }
            EventKind::Modify(ModifyKind::Name(_)) => {
                for path in event.paths {
                    self.touch(path, now).renamed = true;
                }
            }
            EventKind::Modify(_) | EventKind::Any => {
                for path in event.paths {
                    self.touch(path, now);
                }
            }
        }
    }

    fn touch(&mut self, path: PathBuf, now: Instant) -> &mut Touched {
        let touched = self.pending.entry(path).or_insert(Touched {
            last: now,
            created: false,
            removed: false,
            renamed: false,
            moved_to: None,
        });
        touched.last = now;
        touched
    }

    /// Events under the roots that contain any of `under` (all roots if
    /// none do) may have been lost.
    fn lose(&mut self, under: &[PathBuf], now: Instant) {
        let named: Vec<PathBuf> = self
            .roots
            .iter()
            .filter(|root| under.iter().any(|path| path.starts_with(&root.path)))
            .map(|root| root.path.clone())
            .collect();
        let lost = if named.is_empty() {
            self.roots.iter().map(|root| root.path.clone()).collect()
        } else {
            named
        };
        for root in lost {
            self.rescans.insert(root, now);
        }
    }

    fn settle(&mut self, now: Instant) -> Vec<FsEvent> {
        let debounce = self.debounce;
        let due = |last: Instant| now.saturating_duration_since(last) >= debounce;
        let mut events = Vec::new();

        let rescans: Vec<PathBuf> = self
            .rescans
            .iter()
            .filter(|&(_, &last)| due(last))
            .map(|(root, _)| root.clone())
            .collect();
        for root in rescans {
            self.rescans.remove(&root);
            // A rescan covers whatever else settled under the same root.
            self.pending
                .retain(|path, touched| !(path.starts_with(&root) && due(touched.last)));
            events.push(FsEvent::Rescan(root));
        }

        let mut settled: Vec<(PathBuf, Touched)> = Vec::new();
        let paths: Vec<PathBuf> = self
            .pending
            .iter()
            .filter(|(_, touched)| due(touched.last))
            .map(|(path, _)| path.clone())
            .collect();
        for path in paths {
            if let Some(touched) = self.pending.remove(&path) {
                settled.push((path, touched));
            }
        }
        settled.sort_by(|a, b| a.0.cmp(&b.0));

        // A rename that named both ends is one event when both have settled.
        let mut absorbed: Vec<PathBuf> = Vec::new();
        for (from, touched) in &settled {
            let Some(to) = &touched.moved_to else {
                continue;
            };
            let both_settled = settled.iter().any(|(path, _)| path == to);
            if both_settled && !exists(from) && exists(to) {
                absorbed.push(from.clone());
                absorbed.push(to.clone());
                events.push(FsEvent::Renamed {
                    from: from.clone(),
                    to: to.clone(),
                });
            }
        }
        for (path, touched) in settled {
            if absorbed.contains(&path) {
                continue;
            }
            events.push(if exists(&path) {
                let new = touched.created && !touched.removed && !touched.renamed;
                if new {
                    FsEvent::Created(path)
                } else {
                    FsEvent::Modified(path)
                }
            } else {
                FsEvent::Removed(path)
            });
        }
        events
    }
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

#[cfg(test)]
mod tests;
