//! Process-level behavior of the runtime, against a scripted server.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use glaux_lsp::{Event, LspError, LspHandle, LspRuntime, ServerSpec, SessionKey};

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake_server.py")
}

fn python() -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|dir| Path::new(dir).join("python3"))
        .find(|candidate| candidate.is_file())
}

fn spec(args: &[&str]) -> Option<ServerSpec> {
    let mut spec = ServerSpec::new(python()?, std::env::temp_dir());
    spec.args = std::iter::once(script().into_os_string())
        .chain(args.iter().map(Into::into))
        .collect();
    spec.handshake_timeout = Duration::from_millis(400);
    spec.backoff_base = Duration::from_millis(20);
    spec.backoff_cap = Duration::from_millis(80);
    spec.max_failures = 2;
    Some(spec)
}

/// Polls until `wanted` accepts an event, collecting everything seen.
fn wait_for(
    handle: &LspHandle,
    wanted: impl Fn(&Event) -> bool,
) -> Result<Vec<Event>, Vec<Event>> {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        let batch = handle.poll();
        let done = batch.iter().any(&wanted);
        seen.extend(batch);
        if done {
            return Ok(seen);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(seen)
}

fn messages(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Diagnostics { diagnostics, .. } => Some(diagnostics[0].message.clone()),
            _ => None,
        })
        .collect()
}

fn is_diagnostics(event: &Event) -> bool {
    matches!(event, Event::Diagnostics { .. })
}

#[test]
fn a_document_gets_diagnostics_for_its_version() {
    let Some(spec) = spec(&["echo"]) else { return };
    let runtime = LspRuntime::start(2).unwrap();
    let handle = runtime.handle();
    let key = SessionKey::new("a");
    handle.open_session(key.clone(), spec).unwrap();
    handle.open_document(&key, "file:///m", "luau", 7, "x").unwrap();
    let seen = wait_for(&handle, is_diagnostics).unwrap();
    assert!(matches!(seen[0], Event::Ready { .. }));
    assert_eq!(messages(&seen), ["v7"]);
}

#[test]
fn a_crashed_server_is_restarted_and_gets_its_documents_back() {
    let marker = std::env::temp_dir().join(format!("glaux-lsp-marker-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let Some(spec) = spec(&["crash-once", marker.to_str().unwrap()]) else { return };
    let runtime = LspRuntime::start(2).unwrap();
    let handle = runtime.handle();
    let key = SessionKey::new("a");
    handle.open_session(key.clone(), spec).unwrap();
    handle.open_document(&key, "file:///m", "luau", 5, "x").unwrap();
    let seen = wait_for(&handle, is_diagnostics).unwrap();
    let _ = std::fs::remove_file(&marker);
    assert!(
        seen.iter().any(|event| matches!(event, Event::Failed { .. })),
        "{seen:?}"
    );
    assert_eq!(messages(&seen), ["v5"]);
}

#[test]
fn a_missing_program_disables_the_session_at_once() {
    let runtime = LspRuntime::start(2).unwrap();
    let handle = runtime.handle();
    let spec = ServerSpec::new("/definitely/not/here", std::env::temp_dir());
    handle.open_session(SessionKey::new("a"), spec).unwrap();
    let seen = wait_for(&handle, |event| matches!(event, Event::Closed { .. })).unwrap();
    let kinds: Vec<_> = seen
        .iter()
        .map(|event| matches!(event, Event::Disabled { .. }))
        .collect();
    assert_eq!(kinds, [true, false], "{seen:?}");
}

#[test]
fn a_server_that_keeps_dying_is_given_up_on() {
    let Some(spec) = spec(&["exit"]) else { return };
    let runtime = LspRuntime::start(2).unwrap();
    let handle = runtime.handle();
    handle.open_session(SessionKey::new("a"), spec).unwrap();
    let seen = wait_for(&handle, |event| matches!(event, Event::Closed { .. })).unwrap();
    assert!(seen.iter().any(|e| matches!(e, Event::Disabled { .. })), "{seen:?}");
    let failures = seen.iter().filter(|e| matches!(e, Event::Failed { .. })).count();
    assert_eq!(failures, 2);
}

#[test]
fn a_silent_server_fails_the_handshake_without_blocking_the_caller() {
    let Some(spec) = spec(&["silent"]) else { return };
    let runtime = LspRuntime::start(2).unwrap();
    let handle = runtime.handle();
    let key = SessionKey::new("a");
    let started = Instant::now();
    handle.open_session(key.clone(), spec).unwrap();
    handle.open_document(&key, "file:///m", "luau", 1, "x").unwrap();
    for version in 2..200 {
        handle.change_document(&key, "file:///m", version, "x").unwrap();
        let _ = handle.poll();
    }
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "the caller waited {:?}",
        started.elapsed()
    );
    wait_for(&handle, |event| matches!(event, Event::Failed { .. })).unwrap();
}

#[test]
fn closing_a_session_reports_closed_and_forgets_the_key() {
    let Some(spec) = spec(&["echo"]) else { return };
    let runtime = LspRuntime::start(2).unwrap();
    let handle = runtime.handle();
    let key = SessionKey::new("a");
    handle.open_session(key.clone(), spec).unwrap();
    wait_for(&handle, |event| matches!(event, Event::Ready { .. })).unwrap();
    handle.close_session(&key);
    wait_for(&handle, |event| matches!(event, Event::Closed { .. })).unwrap();
    assert!(!handle.has_session(&key));
    assert_eq!(
        handle.change_document(&key, "file:///m", 1, "x"),
        Err(LspError::NoSession)
    );
}

#[test]
fn sessions_are_independent_and_bounded() {
    let (Some(first), Some(second), Some(third)) = (spec(&["echo"]), spec(&["echo"]), spec(&["echo"]))
    else {
        return;
    };
    let runtime = LspRuntime::start(2).unwrap();
    let handle = runtime.handle();
    let (a, b) = (SessionKey::new("a"), SessionKey::new("b"));
    handle.open_session(a.clone(), first).unwrap();
    handle.open_session(b.clone(), second).unwrap();
    assert_eq!(
        handle.open_session(SessionKey::new("c"), third),
        Err(LspError::TooManySessions)
    );
    handle.open_document(&a, "file:///m", "luau", 1, "x").unwrap();
    handle.open_document(&b, "file:///m", "luau", 9, "x").unwrap();
    let mut by_key = Vec::new();
    while by_key.len() < 2 {
        for event in wait_for(&handle, is_diagnostics).unwrap() {
            if let Event::Diagnostics { key, version, .. } = event {
                by_key.push((key.as_str().to_owned(), version));
            }
        }
    }
    by_key.sort();
    assert_eq!(by_key, [("a".into(), 1), ("b".into(), 9)]);
}

#[test]
fn dropping_the_runtime_stops_its_servers_promptly() {
    let Some(spec) = spec(&["silent"]) else { return };
    let runtime = LspRuntime::start(2).unwrap();
    let handle = runtime.handle();
    handle.open_session(SessionKey::new("a"), spec).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    drop(runtime);
    assert!(started.elapsed() < Duration::from_secs(6), "{:?}", started.elapsed());
}
