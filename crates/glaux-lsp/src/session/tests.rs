use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tokio::io::{BufReader, DuplexStream, duplex};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::*;
use crate::frame::{read_frame, write_frame};
use crate::protocol::Severity;

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            timeout(Duration::from_secs(10), future)
                .await
                .expect("the test hung")
        })
}

/// The server side of an in-memory conversation.
struct Server {
    from_client: BufReader<DuplexStream>,
    to_client: DuplexStream,
}

impl Server {
    async fn next(&mut self) -> Value {
        timeout(Duration::from_secs(2), read_frame(&mut self.from_client))
            .await
            .expect("the client sent nothing")
            .unwrap()
            .expect("the client closed")
    }

    async fn next_method(&mut self, method: &str) -> Value {
        let message = self.next().await;
        assert_eq!(message["method"], method, "{message}");
        message
    }

    async fn send(&mut self, message: Value) {
        write_frame(&mut self.to_client, &message).await.unwrap();
    }

    /// Answers `initialize` with the given capabilities and consumes
    /// `initialized`.
    async fn handshake(&mut self, capabilities: Value) -> Value {
        let init = self.next_method("initialize").await;
        self.send(
            json!({"jsonrpc": "2.0", "id": init["id"], "result": {"capabilities": capabilities}}),
        )
        .await;
        self.next_method("initialized").await;
        init
    }

    async fn answer(&mut self, request: &Value, items: Value) {
        self.send(json!({"jsonrpc": "2.0", "id": request["id"],
                         "result": {"kind": "full", "items": items}}))
            .await;
    }
}

struct Rig {
    server: Server,
    shared: Arc<Shared>,
    outbox: Arc<Outbox>,
    stop: watch::Sender<bool>,
    session: JoinHandle<SessionExit>,
}

fn rig(request_timeout: Duration) -> Rig {
    let (client_out, server_in) = duplex(1 << 16);
    let (server_out, client_in) = duplex(1 << 16);
    let shared = Arc::new(Shared::new(64));
    let outbox = Arc::new(Outbox::default());
    let (stop, stopped) = watch::channel(false);
    let config = SessionConfig {
        key: SessionKey::new("k"),
        root_uri: "file:///root".into(),
        initialization_options: None,
        handshake_timeout: Duration::from_millis(300),
        request_timeout,
    };
    let session = {
        let (shared, outbox) = (Arc::clone(&shared), Arc::clone(&outbox));
        tokio::spawn(async move {
            run(
                BufReader::new(client_in),
                client_out,
                &config,
                &shared,
                &outbox,
                stopped,
            )
            .await
        })
    };
    Rig {
        server: Server {
            from_client: BufReader::new(server_in),
            to_client: server_out,
        },
        shared,
        outbox,
        stop,
        session,
    }
}

impl Rig {
    fn edit(&self, action: impl FnOnce(&mut Documents)) {
        action(&mut self.shared.documents());
        self.shared.wake.notify_one();
    }

    async fn events(&self) -> Vec<Event> {
        tokio::time::sleep(Duration::from_millis(40)).await;
        self.outbox.take()
    }
}

const PULL: fn() -> Value = || json!({"diagnosticProvider": {"interFileDependencies": true}});

fn one_error(message: &str) -> Value {
    json!([{"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
            "severity": 1, "message": message}])
}

fn diagnostics_of(events: &[Event]) -> Vec<(String, i64, Vec<String>)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Diagnostics {
                uri,
                version,
                diagnostics,
                ..
            } => Some((
                uri.clone(),
                *version,
                diagnostics.iter().map(|d| d.message.clone()).collect(),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn the_handshake_asks_for_pull_and_reports_ready() {
    block_on(async {
        let mut rig = rig(Duration::from_secs(5));
        let init = rig.server.handshake(PULL()).await;
        assert_eq!(init["params"]["rootUri"], "file:///root");
        assert_eq!(
            init["params"]["capabilities"]["textDocument"]["diagnostic"]["dynamicRegistration"],
            false
        );
        assert_eq!(
            rig.events().await,
            vec![Event::Ready {
                key: SessionKey::new("k")
            }]
        );
    });
}

#[test]
fn documents_queued_early_are_sent_after_initialized_then_pulled() {
    block_on(async {
        let mut rig = rig(Duration::from_secs(5));
        rig.edit(|d| d.open("file:///a", "luau", 3, "x").unwrap());
        rig.server.handshake(PULL()).await;
        let open = rig.server.next_method("textDocument/didOpen").await;
        assert_eq!(open["params"]["textDocument"]["version"], 3);
        let pull = rig.server.next_method("textDocument/diagnostic").await;
        rig.server.answer(&pull, one_error("boom")).await;
        let events = rig.events().await;
        assert_eq!(
            diagnostics_of(&events),
            vec![("file:///a".into(), 3, vec!["boom".into()])]
        );
        let Event::Diagnostics { diagnostics, .. } = &events[1] else {
            panic!()
        };
        assert_eq!(diagnostics[0].severity, Some(Severity::Error));
    });
}

#[test]
fn a_result_computed_before_a_newer_change_is_discarded() {
    block_on(async {
        let mut rig = rig(Duration::from_secs(5));
        rig.server.handshake(PULL()).await;
        rig.edit(|d| d.open("file:///a", "luau", 1, "old").unwrap());
        rig.server.next_method("textDocument/didOpen").await;
        let first = rig.server.next_method("textDocument/diagnostic").await;

        rig.edit(|d| d.change("file:///a", 2, "new"));
        let change = rig.server.next_method("textDocument/didChange").await;
        assert_eq!(change["params"]["textDocument"]["version"], 2);

        rig.server
            .answer(&first, one_error("about the old text"))
            .await;
        let second = rig.server.next_method("textDocument/diagnostic").await;
        rig.server
            .answer(&second, one_error("about the new text"))
            .await;

        assert_eq!(
            diagnostics_of(&rig.events().await),
            vec![("file:///a".into(), 2, vec!["about the new text".into()])]
        );
    });
}

#[test]
fn changing_one_module_pulls_every_open_module_again() {
    block_on(async {
        let mut rig = rig(Duration::from_secs(5));
        rig.server.handshake(PULL()).await;
        rig.edit(|d| {
            d.open("file:///a", "luau", 1, "a").unwrap();
            d.open("file:///b", "luau", 1, "b").unwrap();
        });
        for _ in 0..2 {
            rig.server.next_method("textDocument/didOpen").await;
        }
        for _ in 0..2 {
            let pull = rig.server.next_method("textDocument/diagnostic").await;
            rig.server.answer(&pull, json!([])).await;
        }
        rig.events().await;

        rig.edit(|d| d.change("file:///a", 2, "a2"));
        rig.server.next_method("textDocument/didChange").await;
        let mut pulled = Vec::new();
        for _ in 0..2 {
            let pull = rig.server.next_method("textDocument/diagnostic").await;
            pulled.push(
                pull["params"]["textDocument"]["uri"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            );
            rig.server.answer(&pull, json!([])).await;
        }
        pulled.sort();
        assert_eq!(pulled, ["file:///a", "file:///b"]);
    });
}

#[test]
fn a_document_closed_while_pulling_gets_no_diagnostics() {
    block_on(async {
        let mut rig = rig(Duration::from_secs(5));
        rig.server.handshake(PULL()).await;
        rig.edit(|d| d.open("file:///a", "luau", 1, "x").unwrap());
        rig.server.next_method("textDocument/didOpen").await;
        let pull = rig.server.next_method("textDocument/diagnostic").await;
        rig.edit(|d| d.close("file:///a").unwrap());
        rig.server.next_method("textDocument/didClose").await;
        rig.server.answer(&pull, one_error("late")).await;
        assert_eq!(diagnostics_of(&rig.events().await), vec![]);
    });
}

#[test]
fn push_mode_accepts_only_the_current_version() {
    block_on(async {
        let mut rig = rig(Duration::from_secs(5));
        rig.server.handshake(json!({})).await;
        rig.edit(|d| d.open("file:///a", "luau", 4, "x").unwrap());
        rig.server.next_method("textDocument/didOpen").await;
        let publish = |version: Option<i64>, message: &str| {
            let mut params = json!({"uri": "file:///a", "diagnostics": one_error(message)});
            if let Some(version) = version {
                params["version"] = json!(version);
            }
            json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": params})
        };
        rig.server.send(publish(None, "unversioned")).await;
        rig.server.send(publish(Some(3), "stale")).await;
        rig.server.send(publish(Some(4), "current")).await;
        assert_eq!(
            diagnostics_of(&rig.events().await),
            vec![("file:///a".into(), 4, vec!["current".into()])]
        );
    });
}

#[test]
fn server_requests_are_answered_so_the_server_never_waits() {
    block_on(async {
        let mut rig = rig(Duration::from_secs(5));
        rig.server.handshake(PULL()).await;
        rig.server
            .send(
                json!({"jsonrpc": "2.0", "id": 77, "method": "workspace/configuration",
                         "params": {"items": [{}, {}]}}),
            )
            .await;
        let reply = rig.server.next().await;
        assert_eq!(reply["id"], 77);
        assert_eq!(reply["result"], json!([null, null]));
        rig.server
            .send(json!({"jsonrpc": "2.0", "id": 78, "method": "unheard/of"}))
            .await;
        assert_eq!(rig.server.next().await["error"]["code"], -32601);
    });
}

#[test]
fn unanswered_requests_are_cancelled_and_repeated_silence_loses_the_server() {
    block_on(async {
        let mut rig = rig(Duration::from_millis(80));
        rig.server.handshake(PULL()).await;
        rig.edit(|d| d.open("file:///a", "luau", 1, "x").unwrap());
        rig.server.next_method("textDocument/didOpen").await;
        let mut cancels = 0;
        loop {
            let message = rig.server.next().await;
            if message["method"] == "$/cancelRequest" {
                cancels += 1;
                rig.edit(|d| d.change("file:///a", 1 + cancels, "x"));
            }
            if cancels == 3 {
                break;
            }
        }
        match timeout(Duration::from_secs(2), rig.session)
            .await
            .unwrap()
            .unwrap()
        {
            SessionExit::Lost(reason) => assert!(reason.contains("stopped answering"), "{reason}"),
            SessionExit::Shutdown => panic!("expected the server to be lost"),
        }
    });
}

#[test]
fn a_server_that_never_answers_initialize_is_lost() {
    block_on(async {
        let mut rig = rig(Duration::from_secs(5));
        rig.server.next_method("initialize").await;
        match rig.session.await.unwrap() {
            SessionExit::Lost(reason) => assert!(reason.contains("in time"), "{reason}"),
            SessionExit::Shutdown => panic!("expected a timeout"),
        }
    });
}

#[test]
fn a_server_that_hangs_up_is_lost() {
    block_on(async {
        let mut rig = rig(Duration::from_secs(5));
        rig.server.handshake(PULL()).await;
        drop(rig.server.to_client);
        assert!(matches!(rig.session.await.unwrap(), SessionExit::Lost(_)));
    });
}

#[test]
fn stopping_sends_shutdown_then_exit() {
    block_on(async {
        let mut rig = rig(Duration::from_secs(5));
        rig.server.handshake(PULL()).await;
        rig.stop.send(true).unwrap();
        let shutdown = rig.server.next_method("shutdown").await;
        rig.server
            .send(json!({"jsonrpc": "2.0", "id": shutdown["id"], "result": null}))
            .await;
        rig.server.next_method("exit").await;
        assert!(matches!(rig.session.await.unwrap(), SessionExit::Shutdown));
    });
}
