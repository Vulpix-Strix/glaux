//! The dedicated runtime, the supervised server processes, and the handle
//! the rest of the program uses.

use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;
use tokio::time::{sleep, timeout};

use crate::protocol::Diagnostic;
use crate::session::{self, SessionConfig, SessionExit, Shared};
use crate::uri::path_to_uri;

/// Names one server session. Callers pick the key, typically one per
/// language and workspace.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey(String);

impl SessionKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How to start a server and how patient to be with it.
#[derive(Debug, Clone)]
pub struct ServerSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    /// The workspace root the server is told about.
    pub root: PathBuf,
    pub initialization_options: Option<Value>,
    pub handshake_timeout: Duration,
    pub request_timeout: Duration,
    /// Consecutive failures tolerated before the session is disabled.
    pub max_failures: u32,
    pub backoff_base: Duration,
    pub backoff_cap: Duration,
    /// A session that lasted this long counts as healthy again.
    pub stable_after: Duration,
}

impl ServerSpec {
    pub fn new(program: impl Into<PathBuf>, root: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            root: root.into(),
            initialization_options: None,
            handshake_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(5),
            max_failures: 4,
            backoff_base: Duration::from_millis(500),
            backoff_cap: Duration::from_secs(30),
            stable_after: Duration::from_secs(30),
        }
    }
}

/// What the runtime reports. Read it with [`LspHandle::poll`].
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Ready {
        key: SessionKey,
    },
    /// Diagnostics for exactly the text sent as `version`.
    Diagnostics {
        key: SessionKey,
        uri: String,
        version: i64,
        diagnostics: Vec<Diagnostic>,
    },
    /// The server was lost and will be started again after `retry_in`.
    Failed {
        key: SessionKey,
        reason: String,
        retry_in: Duration,
    },
    /// The session gave up. Nothing more arrives for this key.
    Disabled {
        key: SessionKey,
        reason: String,
    },
    Closed {
        key: SessionKey,
    },
}

const OUTBOX_CAPACITY: usize = 1024;

/// Events waiting for the main thread. A newer diagnostics event for the
/// same document replaces the older one, and the oldest diagnostics are
/// dropped first if the box ever fills.
#[derive(Default)]
pub(crate) struct Outbox {
    events: Mutex<VecDeque<Event>>,
}

impl Outbox {
    pub fn push(&self, event: Event) {
        let mut events = self.events.lock().unwrap_or_else(|p| p.into_inner());
        if let Event::Diagnostics { key, uri, .. } = &event {
            events.retain(|queued| {
                !matches!(queued, Event::Diagnostics { key: k, uri: u, .. } if k == key && u == uri)
            });
        }
        if events.len() >= OUTBOX_CAPACITY {
            let oldest = events
                .iter()
                .position(|queued| matches!(queued, Event::Diagnostics { .. }));
            events.remove(oldest.unwrap_or(0));
        }
        events.push_back(event);
    }

    pub(crate) fn take(&self) -> Vec<Event> {
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .drain(..)
            .collect()
    }
}

/// Why a call on the handle was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LspError {
    #[error("no such session")]
    NoSession,
    #[error("too many sessions are running")]
    TooManySessions,
    #[error("the notification queue is full")]
    QueueFull,
}

struct Entry {
    shared: Arc<Shared>,
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

struct Registry {
    sessions: Mutex<HashMap<SessionKey, Entry>>,
    outbox: Arc<Outbox>,
    runtime: tokio::runtime::Handle,
    max_sessions: usize,
}

impl Registry {
    fn sessions(&self) -> std::sync::MutexGuard<'_, HashMap<SessionKey, Entry>> {
        self.sessions.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// The part of the runtime the rest of the program holds. Cloning is cheap
/// and no method waits for a server.
#[derive(Clone)]
pub struct LspHandle {
    registry: Arc<Registry>,
}

const DOCUMENT_QUEUE: usize = 4096;

impl LspHandle {
    /// Starts a session, or does nothing if `key` already has one.
    pub fn open_session(&self, key: SessionKey, spec: ServerSpec) -> Result<(), LspError> {
        let mut sessions = self.registry.sessions();
        if sessions.contains_key(&key) {
            return Ok(());
        }
        if sessions.len() >= self.registry.max_sessions {
            return Err(LspError::TooManySessions);
        }
        let shared = Arc::new(Shared::new(DOCUMENT_QUEUE));
        let (stop, stopped) = watch::channel(false);
        let task = self.registry.runtime.spawn(supervise(
            key.clone(),
            spec,
            Arc::clone(&shared),
            Arc::clone(&self.registry.outbox),
            stopped,
        ));
        sessions.insert(key, Entry { shared, stop, task });
        Ok(())
    }

    /// Asks a session to shut its server down. [`Event::Closed`] follows.
    pub fn close_session(&self, key: &SessionKey) {
        if let Some(entry) = self.registry.sessions().remove(key) {
            let _ = entry.stop.send(true);
        }
    }

    pub fn has_session(&self, key: &SessionKey) -> bool {
        self.registry.sessions().contains_key(key)
    }

    /// Opens a document, or replaces the text of one already open.
    pub fn open_document(
        &self,
        key: &SessionKey,
        uri: &str,
        language: &str,
        version: i64,
        text: &str,
    ) -> Result<(), LspError> {
        self.with_shared(key, |shared| {
            shared
                .documents()
                .open(uri, language, version, text)
                .map_err(|_| LspError::QueueFull)
        })
    }

    pub fn change_document(
        &self,
        key: &SessionKey,
        uri: &str,
        version: i64,
        text: &str,
    ) -> Result<(), LspError> {
        self.with_shared(key, |shared| {
            shared.documents().change(uri, version, text);
            Ok(())
        })
    }

    pub fn close_document(&self, key: &SessionKey, uri: &str) -> Result<(), LspError> {
        self.with_shared(key, |shared| {
            shared
                .documents()
                .close(uri)
                .map_err(|_| LspError::QueueFull)
        })
    }

    fn with_shared(
        &self,
        key: &SessionKey,
        action: impl FnOnce(&Shared) -> Result<(), LspError>,
    ) -> Result<(), LspError> {
        let shared = self
            .registry
            .sessions()
            .get(key)
            .map(|entry| Arc::clone(&entry.shared))
            .ok_or(LspError::NoSession)?;
        let result = action(&shared);
        shared.wake.notify_one();
        result
    }

    /// Takes every event produced since the last call. Never blocks.
    pub fn poll(&self) -> Vec<Event> {
        self.registry.outbox.take()
    }
}

/// Owns the runtime thread. Dropping it closes every session and waits a
/// bounded time for the servers to exit.
pub struct LspRuntime {
    handle: LspHandle,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl LspRuntime {
    /// Starts the runtime thread. `max_sessions` bounds concurrent servers.
    pub fn start(max_sessions: usize) -> io::Result<Self> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("glaux-lsp".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                let registry = Arc::new(Registry {
                    sessions: Mutex::new(HashMap::new()),
                    outbox: Arc::new(Outbox::default()),
                    runtime: runtime.handle().clone(),
                    max_sessions,
                });
                let _ = ready_tx.send(Ok(LspHandle {
                    registry: Arc::clone(&registry),
                }));
                runtime.block_on(async {
                    let _ = stop_rx.await;
                    let entries: Vec<Entry> = registry.sessions().drain().map(|(_, e)| e).collect();
                    for entry in &entries {
                        let _ = entry.stop.send(true);
                    }
                    for entry in entries {
                        let _ = timeout(Duration::from_secs(3), entry.task).await;
                    }
                });
                runtime.shutdown_timeout(Duration::from_secs(1));
            })?;
        let handle = ready_rx
            .recv()
            .map_err(|_| io::Error::other("the language server thread did not start"))??;
        Ok(Self {
            handle,
            stop: Some(stop_tx),
            thread: Some(thread),
        })
    }

    pub fn handle(&self) -> LspHandle {
        self.handle.clone()
    }
}

impl Drop for LspRuntime {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Keeps one server alive for a session: starts it, restarts it with
/// exponential backoff when it is lost, and stops when told to.
async fn supervise(
    key: SessionKey,
    spec: ServerSpec,
    shared: Arc<Shared>,
    outbox: Arc<Outbox>,
    mut stop: watch::Receiver<bool>,
) {
    let mut failures = 0u32;
    loop {
        if *stop.borrow() {
            break;
        }
        // A new process knows nothing, so every open document is owed again.
        shared.documents().resync();
        let started = Instant::now();
        let reason = match start_process(&spec) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                outbox.push(Event::Disabled {
                    key: key.clone(),
                    reason: format!("{} was not found", spec.program.display()),
                });
                break;
            }
            Err(error) => format!("could not start the server: {error}"),
            Ok(mut process) => {
                let config = SessionConfig {
                    key: key.clone(),
                    root_uri: path_to_uri(&spec.root),
                    initialization_options: spec.initialization_options.clone(),
                    handshake_timeout: spec.handshake_timeout,
                    request_timeout: spec.request_timeout,
                };
                let exit = session::run(
                    process.stdout,
                    process.stdin,
                    &config,
                    &shared,
                    &outbox,
                    stop.clone(),
                )
                .await;
                let status = reap(&mut process.child).await;
                // The pipe closes with the process, so the tail is complete
                // a moment later.
                let _ = timeout(Duration::from_millis(200), process.stderr_task).await;
                match exit {
                    SessionExit::Shutdown => break,
                    SessionExit::Lost(reason) => explain(reason, status, &process.stderr_tail),
                }
            }
        };
        if started.elapsed() >= spec.stable_after {
            failures = 0;
        }
        failures += 1;
        if failures > spec.max_failures {
            outbox.push(Event::Disabled {
                key: key.clone(),
                reason,
            });
            break;
        }
        let retry_in = spec
            .backoff_base
            .saturating_mul(1 << (failures - 1).min(16))
            .min(spec.backoff_cap);
        tracing::warn!(target: "glaux_lsp", %key, %reason, ?retry_in, "language server lost");
        outbox.push(Event::Failed {
            key: key.clone(),
            reason,
            retry_in,
        });
        tokio::select! {
            () = sleep(retry_in) => {}
            _ = stop.changed() => {}
        }
    }
    outbox.push(Event::Closed { key });
}

struct Process {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    /// The last lines the server wrote to stderr, to say why it died.
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    stderr_task: tokio::task::JoinHandle<()>,
}

const STDERR_LINES: usize = 5;
const STDERR_CHARS: usize = 300;

fn start_process(spec: &ServerSpec) -> io::Result<Process> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .current_dir(&spec.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let missing = || io::Error::other("the server has no stdio");
    let stdin = child.stdin.take().ok_or_else(missing)?;
    let stdout = BufReader::new(child.stdout.take().ok_or_else(missing)?);
    let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
    let stderr = child.stderr.take();
    let tail = Arc::clone(&stderr_tail);
    let stderr_task = tokio::spawn(async move {
        let Some(stderr) = stderr else { return };
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(target: "glaux_lsp::stderr", "{line}");
            let mut tail = tail.lock().unwrap_or_else(|p| p.into_inner());
            if tail.len() == STDERR_LINES {
                tail.pop_front();
            }
            tail.push_back(line);
        }
    });
    Ok(Process {
        child,
        stdin,
        stdout,
        stderr_tail,
        stderr_task,
    })
}

/// `reason` with how the process ended and the last thing it said, so a
/// log tells why a server was lost without running it again.
fn explain(
    reason: String,
    status: Option<std::process::ExitStatus>,
    tail: &Mutex<VecDeque<String>>,
) -> String {
    let mut details = Vec::new();
    match status {
        Some(status) => details.push(status.to_string()),
        None => details.push("it had to be killed".to_owned()),
    }
    let tail = tail.lock().unwrap_or_else(|p| p.into_inner());
    if !tail.is_empty() {
        let text = tail
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" | ");
        let text: String = text.chars().take(STDERR_CHARS).collect();
        details.push(format!("stderr: {text}"));
    }
    format!("{reason} ({})", details.join("; "))
}

/// Gives the process a moment to exit on its own, then kills it. Returns
/// how it ended, or `None` if it had to be killed.
async fn reap(child: &mut tokio::process::Child) -> Option<std::process::ExitStatus> {
    match timeout(Duration::from_millis(1000), child.wait()).await {
        Ok(status) => status.ok(),
        Err(_) => {
            let _ = child.kill().await;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostics(uri: &str, version: i64) -> Event {
        Event::Diagnostics {
            key: SessionKey::new("k"),
            uri: uri.into(),
            version,
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn a_newer_result_replaces_the_older_one_for_the_same_document() {
        let outbox = Outbox::default();
        outbox.push(diagnostics("a", 1));
        outbox.push(diagnostics("b", 1));
        outbox.push(diagnostics("a", 2));
        assert_eq!(
            outbox.take(),
            vec![diagnostics("b", 1), diagnostics("a", 2)]
        );
        assert_eq!(outbox.take(), vec![]);
    }

    #[test]
    fn a_full_box_sheds_diagnostics_before_state_changes() {
        let outbox = Outbox::default();
        outbox.push(Event::Ready {
            key: SessionKey::new("k"),
        });
        for n in 0..OUTBOX_CAPACITY {
            outbox.push(diagnostics(&format!("u{n}"), 1));
        }
        outbox.push(Event::Closed {
            key: SessionKey::new("k"),
        });
        let events = outbox.take();
        assert_eq!(events.len(), OUTBOX_CAPACITY);
        assert!(matches!(events.first(), Some(Event::Ready { .. })));
        assert!(matches!(events.last(), Some(Event::Closed { .. })));
    }
}
