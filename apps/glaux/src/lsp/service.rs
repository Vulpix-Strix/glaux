//! Keeps one `luau-lsp` session in step with the loaded cartridge.
//!
//! [`Analysis::sync`] runs every frame and only compares revisions, so an
//! idle editor costs a few integer comparisons. Every module of the
//! cartridge is sent to the server, because a script's findings depend on
//! the modules it requires. Open tabs send their text as edited, the rest
//! send what is on disk.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use glaux_docs::{Kind, Library, ScriptLanguage};
use glaux_lsp::uri::path_to_uri;
use glaux_lsp::{Event, LspHandle, LspRuntime, ServerSpec, SessionKey};

use super::translate::to_editor;
use super::workspace::{Shadow, module_name};
use super::{definitions, locate};
use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::SourcePath;
use crate::system::{AnalysisInput, AnalyzerStatus, System};
use crate::ui_text;

/// The server runs one session per cartridge, and more are never needed.
const MAX_SESSIONS: usize = 2;

pub(crate) struct Analysis {
    cache_dir: Option<PathBuf>,
    /// Where a development copy of the executable can be dropped, `bin`.
    config_dir: Option<PathBuf>,
    state: State,
    /// What the analyzer is doing while nothing is running. See
    /// [`Analysis::status`].
    status: AnalyzerStatus,
    /// The user is told once per run that the executable is missing and
    /// once that the analyzer gave up. Later occurrences only reach the log.
    told_unavailable: bool,
    told_stopped: bool,
    /// Restarts and gives up within milliseconds, and waits this long for the
    /// handshake, for tests of servers that fail.
    #[cfg(test)]
    quick_failures: Option<std::time::Duration>,
}

enum State {
    /// Not started. It starts the first time a cartridge needs it.
    Idle,
    /// There is no executable or no place to keep files. Tried again for the
    /// next cartridge or when the setting is toggled, never every frame.
    Unavailable {
        for_dir: PathBuf,
    },
    Running(Box<Running>),
}

struct Running {
    /// Where the analyzer stands, from the events of its session.
    status: AnalyzerStatus,
    session: Option<Session>,
    /// A cartridge whose session could not be kept, so it is not retried
    /// every frame.
    gave_up_on: Option<PathBuf>,
    handle: LspHandle,
    binary: PathBuf,
    definitions: PathBuf,
    cache_dir: PathBuf,
    callbacks: Vec<String>,
    sessions_started: u32,
    #[cfg(test)]
    quick_failures: Option<std::time::Duration>,
    // Last, so the servers are told to stop before the thread is joined.
    _runtime: LspRuntime,
}

struct Session {
    key: SessionKey,
    started_at: Instant,
    host_dir: PathBuf,
    shadow: Shadow,
    modules: HashMap<String, Module>,
    by_uri: HashMap<String, String>,
}

struct Module {
    path: SourcePath,
    uri: String,
    /// The version the server last received. Only grows.
    version: i64,
    source: Source,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Disk,
    /// An open tab at this revision of its document.
    Tab(u64),
}

impl Analysis {
    pub(crate) fn new(cache_dir: Option<PathBuf>, config_dir: Option<PathBuf>) -> Self {
        Self {
            cache_dir,
            config_dir,
            state: State::Idle,
            status: AnalyzerStatus::Off,
            told_unavailable: false,
            told_stopped: false,
            #[cfg(test)]
            quick_failures: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_quick_failures(mut self, handshake: std::time::Duration) -> Self {
        self.quick_failures = Some(handshake);
        self
    }

    /// What the analyzer is doing, with the reason when it is not working.
    pub(crate) fn status(&self) -> &AnalyzerStatus {
        match &self.state {
            State::Running(running) => &running.status,
            _ => &self.status,
        }
    }

    /// Brings the server up to date with the system. Call once per frame.
    #[hotpath::measure(label = "lsp::sync", impl_type = "Analysis")]
    pub(crate) fn sync(&mut self, system: &mut System) {
        if !system.analysis_enabled() {
            if matches!(self.state, State::Running(_)) {
                system.analysis_lost();
            }
            self.state = State::Idle;
            self.status = AnalyzerStatus::Off;
            return;
        }
        let Some(input) = system.analysis_input() else {
            // No cartridge: loading one again is a fresh chance to find the
            // executable, even the same cartridge.
            if matches!(self.state, State::Unavailable { .. }) {
                self.state = State::Idle;
            }
            if let State::Running(running) = &mut self.state {
                running.end_session();
            }
            return;
        };
        if let State::Unavailable { for_dir } = &self.state
            && for_dir != input.cartridge.host_dir()
        {
            self.state = State::Idle;
        }
        if matches!(self.state, State::Idle) {
            let dir = input.cartridge.host_dir().to_path_buf();
            if let Some(text) = self.start(dir) {
                system.analysis_notice(text);
            }
            // Go on in this frame, so the session opens as soon as it can.
            if let (State::Running(running), Some(input)) =
                (&mut self.state, system.analysis_input())
            {
                running.sync(&input);
            }
            return;
        }
        if let State::Running(running) = &mut self.state {
            running.sync(&input);
        }
    }

    /// Hands the system the findings that arrived since the last frame, and
    /// what the analyzer is doing.
    #[hotpath::measure(label = "lsp::poll", impl_type = "Analysis")]
    pub(crate) fn poll(&mut self, system: &mut System) {
        if let State::Running(running) = &mut self.state {
            let mut lost = false;
            let mut stopped = false;
            let mut arrived = Vec::new();
            for event in running.handle.poll() {
                match event {
                    Event::Diagnostics {
                        key,
                        uri,
                        version,
                        diagnostics,
                    } => {
                        if let Some(found) =
                            running.deliver(&key, &uri, version, &diagnostics, system)
                        {
                            arrived.push(found);
                        }
                    }
                    Event::Disabled { key, reason } => {
                        tracing::warn!(target: crate::logging::DIAGNOSTIC_TARGET, %reason, "the Luau analyzer stopped");
                        if running.is_current(&key) {
                            running.status = AnalyzerStatus::Disabled(reason);
                            lost = true;
                            stopped = true;
                        }
                        running.abandon(&key);
                    }
                    Event::Failed {
                        key,
                        reason,
                        retry_in,
                    } => {
                        tracing::warn!(%reason, ?retry_in, "the Luau analyzer was lost, restarting");
                        if running.is_current(&key) {
                            running.status = AnalyzerStatus::Failed { reason, retry_in };
                        }
                    }
                    Event::Ready { key } => {
                        if let Some(session) = running.session.as_ref().filter(|s| s.key == key) {
                            tracing::info!(
                                elapsed = ?session.started_at.elapsed(),
                                "the Luau analyzer is ready"
                            );
                            running.status = AnalyzerStatus::Ready;
                        }
                    }
                    Event::Closed { key } => {
                        tracing::debug!(%key, "the Luau analyzer session closed");
                    }
                }
            }
            if lost {
                system.analysis_lost();
            }
            for (path, revision, items) in arrived {
                system.analysis_arrived(&path, revision, items);
            }
            if stopped && !std::mem::replace(&mut self.told_stopped, true) {
                system.analysis_notice(ui_text::get("code-analyzer-stopped"));
            }
        }
        system.analysis_status(self.status());
    }

    /// Looks for the executable and starts the runtime. On failure the
    /// analyzer waits for the next cartridge, and the returned text is the
    /// notice to show, the first time only.
    fn start(&mut self, dir: PathBuf) -> Option<String> {
        let began = Instant::now();
        match self.try_start() {
            Ok(running) => {
                tracing::info!(
                    binary = %running.binary.display(),
                    elapsed = ?began.elapsed(),
                    "diagnostics from luau-lsp are on"
                );
                self.state = State::Running(Box::new(running));
                None
            }
            Err(reason) => {
                let first = !std::mem::replace(&mut self.told_unavailable, true);
                if first {
                    tracing::warn!(target: crate::logging::DIAGNOSTIC_TARGET, %reason, "diagnostics are off");
                } else {
                    tracing::debug!(%reason, "diagnostics are still off");
                }
                self.status = AnalyzerStatus::Unavailable(reason);
                self.state = State::Unavailable { for_dir: dir };
                first.then(|| ui_text::get("code-analyzer-not-found"))
            }
        }
    }

    fn try_start(&self) -> Result<Running, String> {
        let cache_dir = self
            .cache_dir
            .clone()
            .ok_or("there is no cache directory to keep files in")?;
        let binary = locate::find(self.config_dir.as_deref()).map_err(|error| error.to_string())?;
        let definitions = definitions::extract(&cache_dir)
            .map_err(|error| format!("could not write the Luau definitions: {error}"))?;
        let runtime = LspRuntime::start(MAX_SESSIONS)
            .map_err(|error| format!("could not start the analyzer runtime: {error}"))?;
        Ok(Running {
            status: AnalyzerStatus::Starting,
            session: None,
            gave_up_on: None,
            handle: runtime.handle(),
            binary,
            definitions,
            cache_dir,
            callbacks: callback_names(),
            sessions_started: 0,
            #[cfg(test)]
            quick_failures: self.quick_failures,
            _runtime: runtime,
        })
    }
}

fn callback_names() -> Vec<String> {
    let library = Library::embedded();
    library
        .api()
        .entries_for(ScriptLanguage::Luau)
        .filter(|entry| entry.kind() == Kind::Callback)
        .filter_map(|entry| entry.binding(ScriptLanguage::Luau))
        .map(|binding| binding.name.clone())
        .collect()
}

impl Running {
    fn sync(&mut self, input: &AnalysisInput<'_>) {
        let host_dir = input.cartridge.host_dir();
        if self
            .session
            .as_ref()
            .is_some_and(|s| s.host_dir != host_dir)
        {
            self.end_session();
        }
        if self.session.is_none() {
            if self.gave_up_on.as_deref() == Some(host_dir) {
                return;
            }
            self.gave_up_on = None;
            match self.begin_session(input.cartridge) {
                Ok(session) => {
                    self.status = AnalyzerStatus::Starting;
                    self.session = Some(session);
                }
                Err(reason) => {
                    tracing::warn!(target: crate::logging::DIAGNOSTIC_TARGET, %reason, "the Luau analyzer could not start");
                    self.status = AnalyzerStatus::Disabled(reason);
                    self.gave_up_on = Some(host_dir.to_path_buf());
                    return;
                }
            }
        }
        if let Some(session) = &mut self.session {
            session.update(&self.handle, input);
        }
    }

    fn begin_session(&mut self, cartridge: &LoadedCartridge) -> Result<Session, String> {
        let began = Instant::now();
        self.sessions_started += 1;
        let key = SessionKey::new(format!("luau-{}", self.sessions_started));
        let root = self.cache_dir.join("lsp").join("workspaces").join(format!(
            "{}-{}",
            std::process::id(),
            self.sessions_started
        ));
        let shadow = Shadow::create(root)
            .map_err(|error| format!("could not prepare the analyzer workspace: {error}"))?;
        let mut spec = ServerSpec::new(&self.binary, shadow.root());
        spec.args = vec![
            "lsp".into(),
            format!("--definitions={}", self.definitions.display()).into(),
        ];
        #[cfg(test)]
        if let Some(handshake) = self.quick_failures {
            spec.max_failures = 1;
            spec.backoff_base = std::time::Duration::from_millis(20);
            spec.backoff_cap = std::time::Duration::from_millis(20);
            spec.handshake_timeout = handshake;
        }
        self.handle
            .open_session(key.clone(), spec)
            .map_err(|error| format!("could not start the Luau analyzer: {error}"))?;
        let mut session = Session {
            key,
            started_at: began,
            host_dir: cartridge.host_dir().to_path_buf(),
            shadow,
            modules: HashMap::new(),
            by_uri: HashMap::new(),
        };
        let mut modules = 0;
        if let Ok(listing) = cartridge.script_files() {
            for path in listing.paths {
                if let Some(text) = read_text(cartridge, &path) {
                    session.add(&self.handle, &path, &text, Source::Disk);
                    modules += 1;
                }
            }
        }
        tracing::info!(modules, elapsed = ?began.elapsed(), "the Luau analyzer's workspace is prepared");
        Ok(session)
    }

    fn end_session(&mut self) {
        if let Some(session) = self.session.take() {
            self.handle.close_session(&session.key);
        }
    }

    fn is_current(&self, key: &SessionKey) -> bool {
        self.session.as_ref().is_some_and(|s| &s.key == key)
    }

    /// The server gave up on `key`. Its cartridge is not retried until the
    /// user loads another or toggles the setting.
    fn abandon(&mut self, key: &SessionKey) {
        if self.is_current(key)
            && let Some(session) = self.session.take()
        {
            self.gave_up_on = Some(session.host_dir.clone());
            self.handle.close_session(&session.key);
        }
    }

    /// The findings in `diagnostics`, if they describe the text of an open
    /// tab as it is now.
    fn deliver(
        &self,
        key: &SessionKey,
        uri: &str,
        version: i64,
        diagnostics: &[glaux_lsp::protocol::Diagnostic],
        system: &System,
    ) -> Option<(SourcePath, u64, Vec<crate::system::EditorDiagnostic>)> {
        let session = self.session.as_ref().filter(|s| &s.key == key)?;
        let module = session.modules.get(session.by_uri.get(uri)?)?;
        let Source::Tab(revision) = module.source else {
            return None;
        };
        if module.version != version {
            return None;
        }
        let input = system.analysis_input()?;
        let script = input
            .scripts
            .iter()
            .find(|s| s.path == &module.path && s.revision == revision)?;
        Some((
            module.path.clone(),
            revision,
            to_editor(script.text, diagnostics, &self.callbacks),
        ))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.end_session();
    }
}

impl Session {
    fn add(&mut self, handle: &LspHandle, path: &SourcePath, text: &str, source: Source) {
        let Some(name) = module_name(path) else {
            return;
        };
        let file = match self.shadow.add(&name) {
            Ok(file) => file,
            Err(error) => {
                tracing::debug!(%error, module = name.as_str(), "no placeholder for a module");
                return;
            }
        };
        let uri = path_to_uri(&file);
        if handle
            .open_document(&self.key, &uri, "luau", 1, text)
            .is_err()
        {
            return;
        }
        self.by_uri.insert(uri.clone(), name.as_str().to_owned());
        self.modules.insert(
            name.as_str().to_owned(),
            Module {
                path: path.clone(),
                uri,
                version: 1,
                source,
            },
        );
    }

    fn change(&mut self, handle: &LspHandle, name: &str, text: &str, source: Source) {
        if let Some(module) = self.modules.get_mut(name) {
            module.version += 1;
            module.source = source;
            let _ = handle.change_document(&self.key, &module.uri, module.version, text);
        }
    }

    fn remove(&mut self, handle: &LspHandle, name: &str) {
        if let Some(module) = self.modules.remove(name) {
            let _ = handle.close_document(&self.key, &module.uri);
            self.by_uri.remove(&module.uri);
            if let Ok(parsed) = glaux_modules::ModuleName::parse(name) {
                self.shadow.remove(&parsed);
            }
        }
    }

    /// Sends what changed since the last frame.
    fn update(&mut self, handle: &LspHandle, input: &AnalysisInput<'_>) {
        let mut open = HashSet::new();
        for script in &input.scripts {
            let Some(name) = module_name(script.path) else {
                continue;
            };
            let name = name.as_str().to_owned();
            match self.modules.get(&name) {
                None => self.add(
                    handle,
                    script.path,
                    script.text,
                    Source::Tab(script.revision),
                ),
                Some(module) if module.source == Source::Tab(script.revision) => {}
                Some(_) => {
                    if let Some(module) = self.modules.get_mut(&name) {
                        module.path = script.path.clone();
                    }
                    self.change(handle, &name, script.text, Source::Tab(script.revision));
                }
            }
            open.insert(name);
        }
        let closed: Vec<String> = self
            .modules
            .iter()
            .filter(|(name, module)| {
                matches!(module.source, Source::Tab(_)) && !open.contains(*name)
            })
            .map(|(name, _)| name.clone())
            .collect();
        for name in closed {
            let path = self.modules[&name].path.clone();
            match read_text(input.cartridge, &path) {
                Some(text) => self.change(handle, &name, &text, Source::Disk),
                None => self.remove(handle, &name),
            }
        }
    }
}

fn read_text(cartridge: &LoadedCartridge, path: &SourcePath) -> Option<String> {
    String::from_utf8(cartridge.read_source(path).ok()?).ok()
}
