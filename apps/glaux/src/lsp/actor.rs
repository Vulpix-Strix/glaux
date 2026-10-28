//! The task that keeps a `luau-lsp` session in step with the loaded
//! cartridge, off the thread that draws frames.
//!
//! It runs on the runtime of `glaux-lsp`. The main thread sends it
//! [`Command`]s and reads what it found from an [`Inbox`], and neither side
//! ever waits for the other. Anything that touches the disk runs through
//! `spawn_blocking`, so the servers are never held up by it.
//!
//! Every command carries the generation of the cartridge it is about. When
//! the cartridge is replaced or unloaded the generation moves on, work in
//! flight is cancelled, and whatever it still produces is thrown away.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use glaux_lsp::{DocumentInit, Event, LspHandle, ServerSpec, SessionKey};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::prepare::{self, PrepareError, Prepared};
use super::translate::to_editor;
use super::workspace::{self, Shadow, module_name};
use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::SourcePath;
use crate::system::{AnalyzerStatus, EditorDiagnostic};

/// The text of an open tab as of one of its revisions.
pub(super) struct TabText {
    pub path: SourcePath,
    pub revision: u64,
    pub text: String,
}

pub(super) enum Command {
    /// A cartridge was loaded, with the tabs open on it.
    Load {
        generation: u64,
        cartridge: LoadedCartridge,
        tabs: Vec<TabText>,
    },
    /// The tabs changed: `open` lists every tab, `changed` the texts that are
    /// new since the last command.
    Tabs {
        generation: u64,
        open: Vec<SourcePath>,
        changed: Vec<TabText>,
    },
    /// The cartridge was unloaded.
    Unload { generation: u64 },
}

/// What the actor found for an open tab.
pub(super) struct Found {
    pub generation: u64,
    pub path: SourcePath,
    pub revision: u64,
    pub items: Vec<EditorDiagnostic>,
}

/// What the actor hands back, read by the main thread once per frame.
#[derive(Default)]
pub(super) struct Inbox {
    state: Mutex<InboxState>,
}

#[derive(Default)]
struct InboxState {
    status: Option<(u64, AnalyzerStatus)>,
    found: Vec<Found>,
    session: Option<SessionKey>,
}

/// What was waiting in the [`Inbox`] for the generation asked about.
#[derive(Default)]
pub(super) struct Delivery {
    pub status: Option<AnalyzerStatus>,
    pub found: Vec<Found>,
}

impl Inbox {
    fn state(&self) -> std::sync::MutexGuard<'_, InboxState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn status(&self, generation: u64, status: AnalyzerStatus) {
        self.state().status = Some((generation, status));
    }

    /// Keeps only the newest answer for each tab, which is the only one worth
    /// showing, so the box holds at most one entry per open tab.
    fn found(&self, found: Found) {
        let mut state = self.state();
        state
            .found
            .retain(|kept| !(kept.generation == found.generation && kept.path == found.path));
        state.found.push(found);
    }

    fn session(&self, key: Option<SessionKey>) {
        self.state().session = key;
    }

    /// The key of the running session.
    #[cfg(test)]
    pub(super) fn session_key(&self) -> Option<SessionKey> {
        self.state().session.clone()
    }

    /// Takes what is waiting. Anything from another generation belongs to a
    /// cartridge that is gone and is dropped.
    pub(super) fn take(&self, generation: u64) -> Delivery {
        let mut state = self.state();
        let status = state
            .status
            .take()
            .filter(|(from, _)| *from == generation)
            .map(|(_, status)| status);
        let found = std::mem::take(&mut state.found)
            .into_iter()
            .filter(|found| found.generation == generation)
            .collect();
        Delivery { status, found }
    }
}

/// What does not change while the program runs.
pub(super) struct Settings {
    pub cache_dir: PathBuf,
    pub config_dir: Option<PathBuf>,
    /// Restarts and gives up within milliseconds, and waits this long for
    /// the handshake, for tests of servers that fail.
    #[cfg(test)]
    pub quick_failures: Option<std::time::Duration>,
}

enum Source {
    Disk,
    /// An open tab at this revision, and its text.
    Tab {
        revision: u64,
        text: String,
    },
}

struct Module {
    path: SourcePath,
    uri: String,
    /// The version the server last received. Only grows.
    version: i64,
    source: Source,
}

struct Loaded {
    key: SessionKey,
    cartridge: LoadedCartridge,
    shadow: Shadow,
    modules: HashMap<String, Module>,
    by_uri: HashMap<String, String>,
    callbacks: Vec<String>,
    /// What the listing of the cartridge left out, see
    /// [`Prepared::skipped`].
    skipped: Vec<String>,
    started_at: Instant,
}

/// A preparation running on a blocking thread.
struct Preparing {
    generation: u64,
    cancel: Arc<AtomicBool>,
    job: JoinHandle<Result<Prepared, PrepareError>>,
}

/// The tabs as the last command described them, for when the workspace is
/// ready to receive them.
#[derive(Default)]
struct Tabs {
    open: Vec<SourcePath>,
    texts: HashMap<SourcePath, (u64, String)>,
}

pub(super) struct Actor {
    handle: LspHandle,
    inbox: Arc<Inbox>,
    settings: Settings,
    generation: u64,
    preparing: Option<Preparing>,
    loaded: Option<Loaded>,
    tabs: Tabs,
    sessions_started: u32,
}

/// Runs the actor until the main thread drops its end of `commands`, then
/// closes everything and says so through `finished`.
pub(super) async fn run(
    mut commands: mpsc::Receiver<Command>,
    handle: LspHandle,
    inbox: Arc<Inbox>,
    settings: Settings,
    finished: std::sync::mpsc::Sender<()>,
) {
    let mut actor = Actor {
        handle: handle.clone(),
        inbox,
        settings,
        generation: 0,
        preparing: None,
        loaded: None,
        tabs: Tabs::default(),
        sessions_started: 0,
    };
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => actor.command(command).await,
                None => break,
            },
            finished = Actor::finished(&mut actor.preparing), if actor.preparing.is_some() => {
                actor.prepared(finished).await;
            }
            batch = handle.events() => actor.events(batch).await,
        }
    }
    actor.teardown().await;
    let _ = finished.send(());
}

type Finished = (
    u64,
    Result<Result<Prepared, PrepareError>, tokio::task::JoinError>,
);

impl Actor {
    /// Waits for the preparation in `slot`, which is then empty.
    async fn finished(slot: &mut Option<Preparing>) -> Finished {
        let Some(preparing) = slot.as_mut() else {
            return std::future::pending().await;
        };
        let result = (&mut preparing.job).await;
        let generation = preparing.generation;
        *slot = None;
        (generation, result)
    }

    async fn command(&mut self, command: Command) {
        match command {
            Command::Load {
                generation,
                cartridge,
                tabs,
            } => {
                self.teardown().await;
                self.generation = generation;
                self.tabs = Tabs::default();
                self.tabs
                    .update(tabs.iter().map(|tab| tab.path.clone()).collect(), tabs);
                self.begin(cartridge);
            }
            Command::Tabs {
                generation,
                open,
                changed,
            } => {
                if generation != self.generation {
                    return;
                }
                self.tabs.update(open, changed);
                if self.loaded.is_some() {
                    self.apply_tabs().await;
                }
            }
            Command::Unload { generation } => {
                self.teardown().await;
                self.generation = generation;
            }
        }
    }

    /// Starts preparing the workspace of `cartridge` on a blocking thread.
    fn begin(&mut self, cartridge: LoadedCartridge) {
        self.sessions_started += 1;
        let workspace = self
            .settings
            .cache_dir
            .join("lsp")
            .join("workspaces")
            .join(format!("{}-{}", std::process::id(), self.sessions_started));
        let input = prepare::Input {
            cartridge,
            cache_dir: self.settings.cache_dir.clone(),
            config_dir: self.settings.config_dir.clone(),
            workspace,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let job = self
            .handle
            .spawn_blocking(move || prepare::prepare(input, &flag));
        self.preparing = Some(Preparing {
            generation: self.generation,
            cancel,
            job,
        });
    }

    async fn prepared(&mut self, (generation, result): Finished) {
        if generation != self.generation {
            // Replaced while it worked. It already cleaned up after itself if
            // it noticed; otherwise this removes its workspace.
            if let Ok(Ok(prepared)) = result {
                self.discard(prepared.shadow);
            }
            return;
        }
        let prepared = match result {
            Ok(Ok(prepared)) => prepared,
            Ok(Err(PrepareError::Cancelled)) => return,
            Ok(Err(PrepareError::Unavailable(reason))) => {
                tracing::debug!(%reason, "the Luau analyzer is unavailable");
                self.inbox
                    .status(generation, AnalyzerStatus::Unavailable(reason));
                return;
            }
            Ok(Err(PrepareError::Failed(reason))) => {
                self.give_up(reason);
                return;
            }
            Err(error) => {
                self.give_up(format!("the preparation failed: {error}"));
                return;
            }
        };
        self.start_session(prepared).await;
    }

    /// Starts the server on a workspace that is ready.
    async fn start_session(&mut self, prepared: Prepared) {
        let Prepared {
            cartridge,
            binary,
            definitions,
            shadow,
            modules,
            callbacks,
            skipped,
        } = prepared;
        let key = SessionKey::new(format!("luau-{}", self.sessions_started));
        let mut spec = ServerSpec::new(&binary, shadow.root());
        spec.args = vec![
            "lsp".into(),
            format!("--definitions={}", definitions.display()).into(),
        ];
        #[cfg(test)]
        if let Some(handshake) = self.settings.quick_failures {
            spec.max_failures = 1;
            spec.backoff_base = std::time::Duration::from_millis(20);
            spec.backoff_cap = std::time::Duration::from_millis(20);
            spec.handshake_timeout = handshake;
        }
        let initial = modules
            .iter()
            .map(|module| DocumentInit {
                uri: module.uri.clone(),
                language: "luau".into(),
                version: 1,
                text: module.text.clone(),
            })
            .collect();
        if let Err(error) = self.handle.open_session_with(key.clone(), spec, initial) {
            self.discard(shadow);
            self.give_up(format!("could not start the Luau analyzer: {error}"));
            return;
        }
        let mut by_uri = HashMap::new();
        let modules = modules
            .into_iter()
            .map(|module| {
                by_uri.insert(module.uri.clone(), module.name.clone());
                let value = Module {
                    path: module.path,
                    uri: module.uri,
                    version: 1,
                    source: Source::Disk,
                };
                (module.name, value)
            })
            .collect();
        if !skipped.is_empty() {
            tracing::warn!(
                target: crate::logging::DIAGNOSTIC_TARGET,
                skipped = skipped.len(),
                first = %skipped[0],
                "some scripts of the cartridge are not analyzed"
            );
        }
        self.inbox.session(Some(key.clone()));
        self.loaded = Some(Loaded {
            key,
            cartridge,
            shadow,
            modules,
            by_uri,
            callbacks,
            skipped,
            started_at: Instant::now(),
        });
        self.apply_tabs().await;
    }

    /// Brings the modules that have tabs in line with the tabs, and the ones
    /// whose tab closed back to what is on disk.
    async fn apply_tabs(&mut self) {
        let Some(loaded) = &mut self.loaded else {
            return;
        };
        let open: HashSet<&SourcePath> = self.tabs.open.iter().collect();
        let mut added = Vec::new();
        for path in &self.tabs.open {
            let Some(name) = module_name(path) else {
                continue;
            };
            let Some((revision, text)) = self.tabs.texts.get(path) else {
                continue;
            };
            let key = name.as_str().to_owned();
            match loaded.modules.get_mut(&key) {
                Some(module) if matches!(&module.source, Source::Tab { revision: kept, .. } if kept == revision) =>
                    {}
                Some(module) => {
                    module.version += 1;
                    module.source = Source::Tab {
                        revision: *revision,
                        text: text.clone(),
                    };
                    let _ =
                        self.handle
                            .change_document(&loaded.key, &module.uri, module.version, text);
                }
                None => added.push((name, path.clone(), *revision, text.clone())),
            }
        }
        let closed: Vec<String> = loaded
            .modules
            .iter()
            .filter(|(_, module)| {
                matches!(module.source, Source::Tab { .. }) && !open.contains(&module.path)
            })
            .map(|(name, _)| name.clone())
            .collect();

        for (name, path, revision, text) in added {
            let root = loaded.shadow.root().to_path_buf();
            let module_name = name.clone();
            let file = self
                .handle
                .spawn_blocking(move || workspace::placeholder(&root, &module_name))
                .await;
            let Ok(Ok(file)) = file else {
                tracing::debug!(module = name.as_str(), "no placeholder for a module");
                continue;
            };
            let uri = glaux_lsp::uri::path_to_uri(&file);
            if self
                .handle
                .open_document(&loaded.key, &uri, "luau", 1, &text)
                .is_err()
            {
                continue;
            }
            loaded.by_uri.insert(uri.clone(), name.as_str().to_owned());
            loaded.modules.insert(
                name.as_str().to_owned(),
                Module {
                    path,
                    uri,
                    version: 1,
                    source: Source::Tab { revision, text },
                },
            );
        }
        for name in closed {
            let Some(path) = loaded.modules.get(&name).map(|module| module.path.clone()) else {
                continue;
            };
            let cartridge = loaded.cartridge.clone();
            let disk = self
                .handle
                .spawn_blocking(move || prepare::read_text(&cartridge, &path))
                .await
                .ok()
                .flatten();
            match disk {
                Some(text) => {
                    if let Some(module) = loaded.modules.get_mut(&name) {
                        module.version += 1;
                        module.source = Source::Disk;
                        let _ = self.handle.change_document(
                            &loaded.key,
                            &module.uri,
                            module.version,
                            &text,
                        );
                    }
                }
                None => {
                    if let Some(module) = loaded.modules.remove(&name) {
                        let _ = self.handle.close_document(&loaded.key, &module.uri);
                        loaded.by_uri.remove(&module.uri);
                        if let Ok(parsed) = glaux_modules::ModuleName::parse(&name) {
                            let root = loaded.shadow.root().to_path_buf();
                            // Nobody waits for it: a stray placeholder is harmless.
                            drop(self.handle.spawn_blocking(move || {
                                workspace::remove_placeholder(&root, &parsed);
                            }));
                        }
                    }
                }
            }
        }
    }

    async fn events(&mut self, batch: Vec<Event>) {
        for event in batch {
            match event {
                Event::Diagnostics {
                    key,
                    uri,
                    version,
                    diagnostics,
                } => {
                    let Some(loaded) = self.loaded.as_ref().filter(|l| l.key == key) else {
                        continue;
                    };
                    let Some(module) = loaded
                        .by_uri
                        .get(&uri)
                        .and_then(|name| loaded.modules.get(name))
                    else {
                        continue;
                    };
                    let Source::Tab { revision, text } = &module.source else {
                        continue;
                    };
                    if module.version != version {
                        continue;
                    }
                    self.inbox.found(Found {
                        generation: self.generation,
                        path: module.path.clone(),
                        revision: *revision,
                        items: to_editor(text, &diagnostics, &loaded.callbacks),
                    });
                }
                Event::Ready { key } => {
                    if let Some(loaded) = self.loaded.as_ref().filter(|l| l.key == key) {
                        tracing::info!(
                            elapsed = ?loaded.started_at.elapsed(),
                            "the Luau analyzer is ready"
                        );
                        let status = match loaded.skipped.as_slice() {
                            [] => AnalyzerStatus::Ready,
                            [only] => AnalyzerStatus::Limited(only.clone()),
                            [first, rest @ ..] => {
                                AnalyzerStatus::Limited(format!("{first}, and {} more", rest.len()))
                            }
                        };
                        self.inbox.status(self.generation, status);
                    }
                }
                Event::Failed {
                    key,
                    reason,
                    retry_in,
                } => {
                    if self.loaded.as_ref().is_some_and(|l| l.key == key) {
                        tracing::warn!(%reason, ?retry_in, "the Luau analyzer was lost, restarting");
                        self.inbox
                            .status(self.generation, AnalyzerStatus::Failed { reason, retry_in });
                    }
                }
                Event::Disabled { key, reason } => {
                    if self.loaded.as_ref().is_some_and(|l| l.key == key) {
                        self.give_up(reason);
                    }
                }
                Event::Closed { key } => {
                    tracing::debug!(%key, "the Luau analyzer session closed");
                }
            }
        }
    }

    /// The analyzer will not be back for this cartridge.
    fn give_up(&mut self, reason: String) {
        tracing::warn!(target: crate::logging::DIAGNOSTIC_TARGET, %reason, "the Luau analyzer stopped");
        self.inbox
            .status(self.generation, AnalyzerStatus::Disabled(reason));
        if let Some(loaded) = self.loaded.take() {
            self.handle.close_session(&loaded.key);
            self.discard(loaded.shadow);
        }
        self.inbox.session(None);
    }

    /// Drops a workspace without holding up the runtime: removing it walks
    /// the disk.
    fn discard(&self, shadow: Shadow) {
        drop(self.handle.spawn_blocking(move || drop(shadow)));
    }

    /// Cancels what is being prepared and closes the session, if any.
    async fn teardown(&mut self) {
        if let Some(preparing) = self.preparing.take() {
            preparing.cancel.store(true, Ordering::Relaxed);
            // The job notices the flag, removes what it made and ends. If it
            // had already finished, its workspace is removed here.
            if let Ok(Ok(prepared)) = preparing.job.await {
                self.discard(prepared.shadow);
            }
        }
        if let Some(loaded) = self.loaded.take() {
            self.handle.close_session(&loaded.key);
            self.discard(loaded.shadow);
        }
        self.inbox.session(None);
    }
}

impl Tabs {
    /// Remembers which tabs are open and the texts that changed.
    fn update(&mut self, open: Vec<SourcePath>, changed: Vec<TabText>) {
        self.texts.retain(|path, _| open.contains(path));
        for tab in changed {
            self.texts.insert(tab.path, (tab.revision, tab.text));
        }
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(generation: u64, path: &str, revision: u64) -> Found {
        Found {
            generation,
            path: SourcePath::parse(path).unwrap(),
            revision,
            items: Vec::new(),
        }
    }

    #[test]
    fn answers_from_another_cartridge_are_dropped() {
        let inbox = Inbox::default();
        inbox.found(found(1, "code/main.luau", 3));
        inbox.status(1, AnalyzerStatus::Ready);
        let delivery = inbox.take(2);
        assert!(delivery.found.is_empty());
        assert_eq!(delivery.status, None);
        // They are gone for good, not kept for later.
        assert!(inbox.take(1).found.is_empty());
    }

    #[test]
    fn only_the_newest_answer_for_a_tab_is_kept() {
        let inbox = Inbox::default();
        inbox.found(found(1, "code/main.luau", 3));
        inbox.found(found(1, "code/other.luau", 1));
        inbox.found(found(1, "code/main.luau", 4));
        let revisions: Vec<_> = inbox
            .take(1)
            .found
            .iter()
            .map(|found| (found.path.as_str().to_owned(), found.revision))
            .collect();
        assert_eq!(
            revisions,
            [
                ("code/other.luau".to_owned(), 1),
                ("code/main.luau".to_owned(), 4)
            ]
        );
    }

    #[test]
    fn the_same_path_in_two_cartridges_is_not_confused() {
        let inbox = Inbox::default();
        inbox.found(found(1, "code/main.luau", 5));
        inbox.found(found(2, "code/main.luau", 5));
        let delivery = inbox.take(2);
        assert_eq!(delivery.found.len(), 1);
        assert_eq!(delivery.found[0].generation, 2);
    }
}
