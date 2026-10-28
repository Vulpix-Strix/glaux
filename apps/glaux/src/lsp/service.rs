//! The part of the analyzer that lives on the thread that draws frames.
//!
//! It does almost nothing: [`Analysis::sync`] notices which cartridge is
//! loaded and which tabs changed and sends that to the actor, and
//! [`Analysis::poll`] hands the system what the actor found. Locating the
//! executable, preparing the workspace, reading modules and talking to the
//! server all happen elsewhere, see `actor`.
//!
//! Each call compares a handful of integers when nothing changed, so an idle
//! editor pays next to nothing.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use glaux_lsp::LspRuntime;
use tokio::sync::mpsc::{self, error::TrySendError};

use super::actor::{self, Command, Inbox, Settings, TabText};
use crate::cartridge::source::SourcePath;
use crate::system::{AnalysisInput, AnalyzerStatus, System};
use crate::ui_text;

/// The server runs one session per cartridge, and more are never needed.
const MAX_SESSIONS: usize = 2;

/// Commands waiting for the actor, which it takes as fast as they come.
const COMMANDS: usize = 64;

pub(crate) struct Analysis {
    cache_dir: Option<PathBuf>,
    /// Where a development copy of the executable can be dropped, `bin`.
    config_dir: Option<PathBuf>,
    /// Started the first time a cartridge needs it and kept until the
    /// program ends.
    started: Option<Started>,
    /// The cartridge the actor was last told about. Its commands carry this
    /// number, and the answers that come back with another one are old.
    generation: u64,
    loaded: Option<Loaded>,
    /// Commands the actor had no room for. They go first, in order.
    queued: VecDeque<Command>,
    status: AnalyzerStatus,
    /// The user is told once per run that the executable is missing, that
    /// the analyzer gave up, and that some scripts are not analyzed. Later
    /// occurrences only reach the log.
    told_unavailable: bool,
    told_stopped: bool,
    told_limited: bool,
    #[cfg(test)]
    quick_failures: Option<std::time::Duration>,
}

struct Started {
    commands: mpsc::Sender<Command>,
    inbox: Arc<Inbox>,
    /// The actor says here that it has closed everything.
    finished: std::sync::mpsc::Receiver<()>,
    #[cfg(test)]
    handle: glaux_lsp::LspHandle,
    /// Last, so it outlives the actor it runs.
    _runtime: LspRuntime,
}

/// How long closing the program waits for the actor to cancel what it is
/// doing and remove its files.
const CLOSING: std::time::Duration = std::time::Duration::from_secs(3);

impl Drop for Analysis {
    fn drop(&mut self) {
        let Some(started) = self.started.take() else {
            return;
        };
        // The actor sees its commands end, cancels the preparation under way,
        // closes the session and removes the workspace. The runtime is only
        // stopped after that, or it would cut the actor short and the
        // blocking work it started would go on unattended.
        let Started {
            commands,
            finished,
            _runtime,
            ..
        } = started;
        drop(commands);
        if finished.recv_timeout(CLOSING).is_err() {
            tracing::warn!("the Luau analyzer did not close in time");
        }
    }
}

/// The cartridge the actor works on and what it has been told of its tabs.
struct Loaded {
    dir: PathBuf,
    /// The revision of each tab as the actor last received it.
    sent: HashMap<SourcePath, u64>,
    /// The actor will not run this cartridge, so nothing more is sent.
    halted: bool,
}

impl Analysis {
    pub(crate) fn new(cache_dir: Option<PathBuf>, config_dir: Option<PathBuf>) -> Self {
        Self {
            cache_dir,
            config_dir,
            started: None,
            generation: 0,
            loaded: None,
            queued: VecDeque::new(),
            status: AnalyzerStatus::Off,
            told_unavailable: false,
            told_stopped: false,
            told_limited: false,
            #[cfg(test)]
            quick_failures: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_quick_failures(mut self, handshake: std::time::Duration) -> Self {
        self.quick_failures = Some(handshake);
        self
    }

    /// What the server has been sent and has answered so far.
    #[cfg(test)]
    pub(crate) fn lsp_stats(&self) -> Option<glaux_lsp::StatsSnapshot> {
        let started = self.started.as_ref()?;
        started.handle.stats(&started.inbox.session_key()?)
    }

    /// What the analyzer is doing, with the reason when it is not working.
    pub(crate) fn status(&self) -> &AnalyzerStatus {
        &self.status
    }

    /// Tells the actor what changed since the last frame. Call once per
    /// frame.
    #[hotpath::measure(label = "lsp::sync", impl_type = "Analysis")]
    pub(crate) fn sync(&mut self, system: &mut System) {
        if !system.analysis_enabled() {
            if self.loaded.is_some() {
                self.unload();
                system.analysis_lost();
            }
            self.status = AnalyzerStatus::Off;
            return;
        }
        let Some(input) = system.analysis_input() else {
            // No cartridge: loading one again is a fresh chance to find the
            // executable, even the same cartridge.
            if self.loaded.is_some() {
                self.unload();
            }
            return;
        };
        self.send_queued();
        let host_dir = input.cartridge.host_dir();
        match &self.loaded {
            Some(loaded) if loaded.dir == host_dir => {
                if !loaded.halted {
                    self.send_tabs(&input);
                }
            }
            _ => self.load(&input),
        }
    }

    /// Hands the system the findings that arrived since the last frame, and
    /// what the analyzer is doing.
    #[hotpath::measure(label = "lsp::poll", impl_type = "Analysis")]
    pub(crate) fn poll(&mut self, system: &mut System) {
        if let Some(started) = &self.started {
            let delivery = started.inbox.take(self.generation);
            if let Some(status) = delivery.status {
                self.apply(status, system);
            }
            for found in delivery.found {
                system.analysis_arrived(&found.path, found.revision, found.items);
            }
        }
        system.analysis_status(self.status());
    }

    fn apply(&mut self, status: AnalyzerStatus, system: &mut System) {
        match &status {
            AnalyzerStatus::Unavailable(reason) => {
                self.halt();
                if !std::mem::replace(&mut self.told_unavailable, true) {
                    tracing::warn!(target: crate::logging::DIAGNOSTIC_TARGET, %reason, "diagnostics are off");
                    system.analysis_notice(ui_text::get("code-analyzer-not-found"));
                } else {
                    tracing::debug!(%reason, "diagnostics are still off");
                }
            }
            AnalyzerStatus::Disabled(_) => {
                self.halt();
                system.analysis_lost();
                if !std::mem::replace(&mut self.told_stopped, true) {
                    system.analysis_notice(ui_text::get("code-analyzer-stopped"));
                }
            }
            AnalyzerStatus::Limited(_) => {
                if !std::mem::replace(&mut self.told_limited, true) {
                    system.analysis_notice(ui_text::get("code-analyzer-limited-notice"));
                }
            }
            AnalyzerStatus::Off
            | AnalyzerStatus::Starting
            | AnalyzerStatus::Ready
            | AnalyzerStatus::Failed { .. } => {}
        }
        self.status = status;
    }

    fn halt(&mut self) {
        if let Some(loaded) = &mut self.loaded {
            loaded.halted = true;
        }
    }

    /// Sends the cartridge and its tabs, starting the actor if this is the
    /// first one.
    fn load(&mut self, input: &AnalysisInput<'_>) {
        if self.loaded.is_some() {
            self.unload();
        }
        let dir = input.cartridge.host_dir().to_path_buf();
        if let Err(reason) = self.start() {
            // Nothing runs, so the actor cannot say why. Said here, once.
            if !std::mem::replace(&mut self.told_unavailable, true) {
                tracing::warn!(target: crate::logging::DIAGNOSTIC_TARGET, %reason, "diagnostics are off");
            }
            self.status = AnalyzerStatus::Unavailable(reason);
            self.loaded = Some(Loaded {
                dir,
                sent: HashMap::new(),
                halted: true,
            });
            return;
        }
        self.generation += 1;
        let tabs: Vec<TabText> = input
            .scripts
            .iter()
            .map(|script| TabText {
                path: script.path.clone(),
                revision: script.revision,
                text: script.text.to_owned(),
            })
            .collect();
        self.loaded = Some(Loaded {
            dir,
            sent: revisions(input),
            halted: false,
        });
        self.status = AnalyzerStatus::Starting;
        self.send(Command::Load {
            generation: self.generation,
            cartridge: input.cartridge.clone(),
            tabs,
        });
    }

    fn unload(&mut self) {
        self.loaded = None;
        if self.started.is_some() {
            self.generation += 1;
            self.send(Command::Unload {
                generation: self.generation,
            });
        }
    }

    /// Sends the tabs that changed, if any did.
    fn send_tabs(&mut self, input: &AnalysisInput<'_>) {
        let Some(loaded) = &mut self.loaded else {
            return;
        };
        let unchanged = input.scripts.len() == loaded.sent.len()
            && input
                .scripts
                .iter()
                .all(|script| loaded.sent.get(script.path) == Some(&script.revision));
        if unchanged || !self.queued.is_empty() {
            return;
        }
        let Some(started) = &self.started else {
            return;
        };
        let changed: Vec<TabText> = input
            .scripts
            .iter()
            .filter(|script| loaded.sent.get(script.path) != Some(&script.revision))
            .map(|script| TabText {
                path: script.path.clone(),
                revision: script.revision,
                text: script.text.to_owned(),
            })
            .collect();
        let command = Command::Tabs {
            generation: self.generation,
            open: input
                .scripts
                .iter()
                .map(|script| script.path.clone())
                .collect(),
            changed,
        };
        // If the actor has no room, nothing is recorded and the same tabs are
        // sent again next frame.
        if started.commands.try_send(command).is_ok() {
            loaded.sent = revisions(input);
        }
    }

    /// Sends `command` now, or keeps it for the next frame if the actor has
    /// no room.
    fn send(&mut self, command: Command) {
        self.queued.push_back(command);
        self.send_queued();
    }

    fn send_queued(&mut self) {
        let Some(started) = &self.started else {
            return;
        };
        while let Some(command) = self.queued.pop_front() {
            match started.commands.try_send(command) {
                Ok(()) => {}
                Err(TrySendError::Full(command)) => {
                    self.queued.push_front(command);
                    return;
                }
                Err(TrySendError::Closed(_)) => return,
            }
        }
    }

    /// Starts the runtime and the actor on it, once.
    fn start(&mut self) -> Result<(), String> {
        if self.started.is_some() {
            return Ok(());
        }
        let cache_dir = self
            .cache_dir
            .clone()
            .ok_or("there is no cache directory to keep files in")?;
        let began = Instant::now();
        let runtime = LspRuntime::start(MAX_SESSIONS)
            .map_err(|error| format!("could not start the analyzer runtime: {error}"))?;
        let handle = runtime.handle();
        let inbox = Arc::new(Inbox::default());
        let (commands, receiver) = mpsc::channel(COMMANDS);
        let (done, finished) = std::sync::mpsc::channel();
        let settings = Settings {
            cache_dir,
            config_dir: self.config_dir.clone(),
            #[cfg(test)]
            quick_failures: self.quick_failures,
        };
        handle.spawn(actor::run(
            receiver,
            handle.clone(),
            Arc::clone(&inbox),
            settings,
            done,
        ));
        tracing::info!(elapsed = ?began.elapsed(), "the Luau analyzer's runtime started");
        self.started = Some(Started {
            commands,
            inbox,
            finished,
            #[cfg(test)]
            handle,
            _runtime: runtime,
        });
        Ok(())
    }
}

fn revisions(input: &AnalysisInput<'_>) -> HashMap<SourcePath, u64> {
    input
        .scripts
        .iter()
        .map(|script| (script.path.clone(), script.revision))
        .collect()
}
