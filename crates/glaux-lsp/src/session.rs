//! One conversation with one server, over any pair of byte streams.
//!
//! The protocol lives here and nothing about processes does, so tests drive
//! it with in-memory streams.
//!
//! Diagnostics are pulled with `textDocument/diagnostic` when the server
//! offers it. A server handles messages in order, so a pull sent after a
//! change is answered for that change, and the answer is tagged with the
//! version the client had sent by then. A result is delivered only if
//! nothing was written to the server since the request, which keeps a
//! dependent module from showing diagnostics computed before its dependency
//! changed. Servers without pull support are accepted in push mode, where a
//! `publishDiagnostics` counts only if it names the version it was computed
//! for and that version is still current.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncWrite};
use tokio::sync::{Notify, mpsc, watch};
use tokio::time::{Instant, interval, timeout};

use crate::documents::{Documents, Wire};
use crate::frame::{FrameError, read_frame, write_frame};
use crate::protocol::{self, parse_diagnostics};
use crate::runtime::{Event, Outbox, SessionKey};

/// What a session shares with the handle that feeds it.
pub(crate) struct Shared {
    pub documents: Mutex<Documents>,
    pub wake: Notify,
}

impl Shared {
    pub fn new(capacity: usize) -> Self {
        Self {
            documents: Mutex::new(Documents::new(capacity)),
            wake: Notify::new(),
        }
    }

    pub fn documents(&self) -> std::sync::MutexGuard<'_, Documents> {
        self.documents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub(crate) struct SessionConfig {
    pub key: SessionKey,
    pub root_uri: String,
    pub initialization_options: Option<Value>,
    pub handshake_timeout: Duration,
    pub request_timeout: Duration,
}

pub(crate) enum SessionExit {
    /// The session was asked to stop and did so cleanly.
    Shutdown,
    /// The server went away or stopped answering.
    Lost(String),
}

const OUTGOING: usize = 256;
const MAX_TIMEOUTS: u32 = 3;
const SHUTDOWN_GRACE: Duration = Duration::from_millis(1500);

struct Pending {
    uri: String,
    version: i64,
    epoch: u64,
    deadline: Instant,
}

type Frames = mpsc::Receiver<Result<Option<Value>, FrameError>>;

pub(crate) async fn run<R, W>(
    reader: R,
    writer: W,
    config: &SessionConfig,
    shared: &Shared,
    outbox: &Outbox,
    mut stop: watch::Receiver<bool>,
) -> SessionExit
where
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (frames_tx, mut frames) = mpsc::channel(64);
    let (out, mut out_rx) = mpsc::channel::<Value>(OUTGOING);
    let reading = tokio::spawn(async move {
        let mut reader = reader;
        loop {
            let frame = read_frame(&mut reader).await;
            let last = !matches!(frame, Ok(Some(_)));
            if frames_tx.send(frame).await.is_err() || last {
                break;
            }
        }
    });
    let writing = tokio::spawn(async move {
        let mut writer = writer;
        while let Some(message) = out_rx.recv().await {
            if write_frame(&mut writer, &message).await.is_err() {
                break;
            }
        }
    });

    let exit = converse(&out, &mut frames, config, shared, outbox, &mut stop).await;

    drop(out);
    let _ = timeout(Duration::from_millis(500), writing).await;
    reading.abort();
    exit
}

async fn converse(
    out: &mpsc::Sender<Value>,
    frames: &mut Frames,
    config: &SessionConfig,
    shared: &Shared,
    outbox: &Outbox,
    stop: &mut watch::Receiver<bool>,
) -> SessionExit {
    let capabilities = match handshake(out, frames, config).await {
        Ok(capabilities) => capabilities,
        Err(reason) => return SessionExit::Lost(reason),
    };
    let pull = capabilities.get("diagnosticProvider").is_some_and(|p| !p.is_null());
    if !send(out, protocol::notification("initialized", json!({}))) {
        return SessionExit::Lost("the server stopped reading".into());
    }
    outbox.push(Event::Ready {
        key: config.key.clone(),
    });

    let mut state = State {
        next_id: 2,
        epoch: 0,
        dirty: false,
        timeouts: 0,
        inflight: HashMap::new(),
        pull,
        request_timeout: config.request_timeout,
    };
    let mut tick = interval((config.request_timeout / 4).max(Duration::from_millis(50)));
    // Documents queued before the server was ready are owed from the start.
    if !flush(&mut state, out, shared) {
        return SessionExit::Lost("the server stopped reading".into());
    }
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => {
                if *stop.borrow() {
                    goodbye(out, frames, &mut state).await;
                    return SessionExit::Shutdown;
                }
            }
            frame = frames.recv() => match frame {
                Some(Ok(Some(message))) => {
                    if let Some(reason) = receive(&mut state, message, out, config, shared, outbox) {
                        return SessionExit::Lost(reason);
                    }
                }
                Some(Ok(None)) | None => return SessionExit::Lost("the server closed its output".into()),
                Some(Err(error)) => return SessionExit::Lost(format!("unreadable server output: {error}")),
            },
            () = shared.wake.notified() => {
                if !flush(&mut state, out, shared) {
                    return SessionExit::Lost("the server stopped reading".into());
                }
            }
            _ = tick.tick() => {
                if let Some(reason) = expire(&mut state, out, config) {
                    return SessionExit::Lost(reason);
                }
            }
        }
    }
}

struct State {
    next_id: i64,
    epoch: u64,
    dirty: bool,
    timeouts: u32,
    inflight: HashMap<i64, Pending>,
    pull: bool,
    request_timeout: Duration,
}

/// `false` when the writer is gone or backed up, which the caller treats as
/// a lost server.
fn send(out: &mpsc::Sender<Value>, message: Value) -> bool {
    out.try_send(message).is_ok()
}

async fn handshake(
    out: &mpsc::Sender<Value>,
    frames: &mut Frames,
    config: &SessionConfig,
) -> Result<Value, String> {
    let mut params = json!({
        "processId": std::process::id(),
        "rootUri": config.root_uri,
        "workspaceFolders": [{"uri": config.root_uri, "name": "workspace"}],
        "capabilities": {
            "textDocument": {
                "diagnostic": {"dynamicRegistration": false},
                "publishDiagnostics": {"versionSupport": true},
            },
            "workspace": {"configuration": false},
        },
    });
    if let Some(options) = &config.initialization_options {
        params["initializationOptions"] = options.clone();
    }
    if !send(out, protocol::request(1, "initialize", params)) {
        return Err("the server stopped reading".into());
    }
    let wait = async {
        loop {
            match frames.recv().await {
                Some(Ok(Some(message))) => {
                    if message.get("method").is_some() {
                        answer_server_request(&message, out);
                    } else if message.get("id") == Some(&json!(1)) {
                        return match message.get("error") {
                            Some(error) => Err(format!("initialize failed: {error}")),
                            None => Ok(message["result"]["capabilities"].clone()),
                        };
                    }
                }
                Some(Ok(None)) | None => return Err("the server exited during startup".into()),
                Some(Err(error)) => return Err(format!("unreadable server output: {error}")),
            }
        }
    };
    timeout(config.handshake_timeout, wait)
        .await
        .unwrap_or_else(|_| Err("the server did not answer initialize in time".into()))
}

/// Writes whatever the documents owe the server and starts a round of
/// pulls when something changed. Returns `false` if the writer is gone.
fn flush(state: &mut State, out: &mpsc::Sender<Value>, shared: &Shared) -> bool {
    let wire = shared.documents().drain();
    if !wire.is_empty() {
        state.epoch += 1;
        state.dirty = true;
    }
    for item in wire {
        let sent = match item {
            Wire::Open {
                uri,
                language,
                version,
                text,
            } => send(
                out,
                protocol::notification(
                    "textDocument/didOpen",
                    json!({"textDocument": {
                        "uri": uri, "languageId": language, "version": version, "text": text,
                    }}),
                ),
            ),
            Wire::Change { uri, version, text } => send(
                out,
                protocol::notification(
                    "textDocument/didChange",
                    json!({
                        "textDocument": {"uri": uri, "version": version},
                        "contentChanges": [{"text": text}],
                    }),
                ),
            ),
            Wire::Close { uri } => send(
                out,
                protocol::notification(
                    "textDocument/didClose",
                    json!({"textDocument": {"uri": uri}}),
                ),
            ),
        };
        if !sent {
            return false;
        }
    }
    start_round(state, out, shared)
}

/// Pulls every open document once, unless a round is already in flight.
fn start_round(state: &mut State, out: &mpsc::Sender<Value>, shared: &Shared) -> bool {
    if !state.pull || !state.dirty || !state.inflight.is_empty() {
        return true;
    }
    state.dirty = false;
    let documents = shared.documents().versions();
    for (uri, version) in documents {
        let id = state.next_id;
        state.next_id += 1;
        let sent = send(
            out,
            protocol::request(
                id,
                "textDocument/diagnostic",
                json!({"textDocument": {"uri": uri}}),
            ),
        );
        if !sent {
            return false;
        }
        state.inflight.insert(
            id,
            Pending {
                uri,
                version,
                epoch: state.epoch,
                deadline: Instant::now() + state.request_timeout,
            },
        );
    }
    true
}

fn receive(
    state: &mut State,
    message: Value,
    out: &mpsc::Sender<Value>,
    config: &SessionConfig,
    shared: &Shared,
    outbox: &Outbox,
) -> Option<String> {
    if message.get("method").is_some() {
        if message.get("id").is_some() {
            answer_server_request(&message, out);
        } else if message["method"] == "textDocument/publishDiagnostics" && !state.pull {
            push(&message["params"], config, shared, outbox);
        }
        return None;
    }
    let id = message.get("id").and_then(Value::as_i64)?;
    let pending = state.inflight.remove(&id)?;
    state.timeouts = 0;
    let result = message.get("result").filter(|result| result["kind"] == "full");
    if let Some(result) = result {
        let current = state.epoch == pending.epoch
            && shared.documents().version(&pending.uri) == Some(pending.version);
        if current {
            outbox.push(Event::Diagnostics {
                key: config.key.clone(),
                uri: pending.uri,
                version: pending.version,
                diagnostics: parse_diagnostics(result.get("items")),
            });
        }
    }
    if !start_round(state, out, shared) {
        return Some("the server stopped reading".into());
    }
    None
}

fn push(params: &Value, config: &SessionConfig, shared: &Shared, outbox: &Outbox) {
    let (Some(uri), Some(version)) = (
        params.get("uri").and_then(Value::as_str),
        params.get("version").and_then(Value::as_i64),
    ) else {
        // Without a version there is no way to know which text it describes.
        return;
    };
    if shared.documents().version(uri) == Some(version) {
        outbox.push(Event::Diagnostics {
            key: config.key.clone(),
            uri: uri.to_owned(),
            version,
            diagnostics: parse_diagnostics(params.get("diagnostics")),
        });
    }
}

fn answer_server_request(message: &Value, out: &mpsc::Sender<Value>) {
    let id = &message["id"];
    let method = message["method"].as_str().unwrap_or_default();
    let reply = match method {
        "workspace/configuration" => {
            let count = message["params"]["items"].as_array().map_or(0, Vec::len);
            protocol::response(id, Value::Array(vec![Value::Null; count]))
        }
        "client/registerCapability"
        | "client/unregisterCapability"
        | "window/workDoneProgress/create"
        | "workspace/diagnostic/refresh" => protocol::response(id, Value::Null),
        _ => protocol::method_not_found(id, method),
    };
    let _ = send(out, reply);
}

/// Cancels requests that outlived their deadline. Returns a reason when the
/// server has missed too many in a row.
fn expire(state: &mut State, out: &mpsc::Sender<Value>, config: &SessionConfig) -> Option<String> {
    let now = Instant::now();
    let late: Vec<i64> = state
        .inflight
        .iter()
        .filter(|(_, pending)| now >= pending.deadline)
        .map(|(id, _)| *id)
        .collect();
    for id in late {
        state.inflight.remove(&id);
        let _ = send(out, protocol::notification("$/cancelRequest", json!({"id": id})));
        state.timeouts += 1;
        tracing::warn!(target: "glaux_lsp", key = %config.key, "a diagnostics request timed out");
    }
    (state.timeouts >= MAX_TIMEOUTS).then(|| "the server stopped answering".into())
}

/// `shutdown` then `exit`, giving up quietly if the server is not listening.
async fn goodbye(out: &mpsc::Sender<Value>, frames: &mut Frames, state: &mut State) {
    let id = state.next_id;
    if !send(out, protocol::request(id, "shutdown", Value::Null)) {
        return;
    }
    let wait = async {
        while let Some(Ok(Some(message))) = frames.recv().await {
            if message.get("id") == Some(&json!(id)) && message.get("method").is_none() {
                break;
            }
        }
    };
    let _ = timeout(SHUTDOWN_GRACE, wait).await;
    let _ = send(out, protocol::notification("exit", Value::Null));
}

#[cfg(test)]
mod tests;
