use std::thread;
use std::time::Duration;

use pretty_assertions::assert_eq;

use crate::LogEntry;
use crate::LogLevel;
use crate::debug::*;

const LIMIT: Duration = Duration::from_secs(5);

fn stop_at(line: u32) -> Stop {
    Stop {
        reason: StopReason::Breakpoint,
        frames: vec![Frame {
            function: "tick".into(),
            path: Some("code/main.luau".into()),
            line: Some(line),
            has_variables: true,
        }],
        variables: vec![Variable {
            name: "x".into(),
            value: "1".into(),
            expandable: false,
        }],
    }
}

#[derive(Default)]
struct Recorder {
    breakpoints: Vec<Breakpoint>,
}

impl Inspect for Recorder {
    fn variables(&mut self, frame: usize) -> DebugReply {
        if frame == 0 {
            DebugReply::Variables(vec![Variable {
                name: "x".into(),
                value: "2".into(),
                expandable: true,
            }])
        } else {
            DebugReply::Unavailable
        }
    }

    fn expand(&mut self, _: usize, path: &[usize]) -> DebugReply {
        DebugReply::Variables(
            path.iter()
                .map(|index| Variable {
                    name: index.to_string(),
                    value: String::new(),
                    expandable: false,
                })
                .collect(),
        )
    }

    fn breakpoints(&mut self, breakpoints: &[Breakpoint]) {
        self.breakpoints = breakpoints.to_vec();
    }

    fn output(&mut self) -> (Vec<LogEntry>, u32) {
        (
            vec![LogEntry {
                level: LogLevel::Info,
                text: "hello".into(),
            }],
            2,
        )
    }
}

fn next(link: &DebugLink) -> DebugEvent {
    link.wait_event(LIMIT)
        .expect("runtime alive")
        .expect("an event before the timeout")
}

#[test]
fn a_stopped_runtime_waits_for_the_debugger_and_resumes_as_told() {
    let (link, port) = session();
    let runtime = thread::spawn(move || port.pause(stop_at(7), &mut Recorder::default()));

    assert_eq!(next(&link), DebugEvent::Stopped(stop_at(7)));
    assert!(link.send(DebugRequest::Resume(Resume::Step(Step::Over))));
    assert_eq!(runtime.join().unwrap(), Resume::Step(Step::Over));
}

#[test]
fn requests_made_while_stopped_are_answered_without_resuming() {
    let (link, port) = session();
    let runtime = thread::spawn(move || port.pause(stop_at(1), &mut Recorder::default()));
    next(&link);

    link.send(DebugRequest::Variables { frame: 0 });
    let DebugEvent::Reply(DebugReply::Variables(variables)) = next(&link) else {
        panic!("expected variables");
    };
    assert_eq!(variables[0].value, "2");

    link.send(DebugRequest::Variables { frame: 3 });
    assert_eq!(next(&link), DebugEvent::Reply(DebugReply::Unavailable));

    link.send(DebugRequest::Expand {
        frame: 0,
        path: vec![4, 9],
    });
    let DebugEvent::Reply(DebugReply::Variables(entries)) = next(&link) else {
        panic!("expected entries");
    };
    assert_eq!(entries.len(), 2);

    link.send(DebugRequest::Resume(Resume::Continue));
    assert_eq!(runtime.join().unwrap(), Resume::Continue);
}

#[test]
fn the_output_so_far_is_asked_for_while_stopped() {
    let (link, port) = session();
    let runtime = thread::spawn(move || port.pause(stop_at(1), &mut Recorder::default()));
    next(&link);
    link.send(DebugRequest::Output);
    assert_eq!(
        next(&link),
        DebugEvent::Reply(DebugReply::Output {
            entries: vec![LogEntry {
                level: LogLevel::Info,
                text: "hello".into()
            }],
            dropped: 2
        })
    );
    link.send(DebugRequest::Resume(Resume::Continue));
    runtime.join().unwrap();
}

#[test]
fn breakpoints_sent_while_stopped_reach_the_runtime_before_it_resumes() {
    let (link, port) = session();
    let runtime = thread::spawn(move || {
        let mut recorder = Recorder::default();
        let resume = port.pause(stop_at(1), &mut recorder);
        (resume, recorder.breakpoints)
    });
    next(&link);
    let wanted = vec![Breakpoint {
        path: "code/main.luau".into(),
        line: 12,
    }];
    link.send(DebugRequest::Breakpoints(wanted.clone()));
    link.send(DebugRequest::Resume(Resume::Continue));
    assert_eq!(runtime.join().unwrap(), (Resume::Continue, wanted));
}

#[test]
fn a_running_runtime_takes_only_the_latest_breakpoints() {
    let (link, port) = session();
    let at = |line| Breakpoint {
        path: "code/main.luau".into(),
        line,
    };
    assert_eq!(port.take_breakpoints(), None);
    link.send(DebugRequest::Breakpoints(vec![at(1)]));
    link.send(DebugRequest::Variables { frame: 0 });
    link.send(DebugRequest::Breakpoints(vec![at(2), at(3)]));
    assert_eq!(port.take_breakpoints(), Some(vec![at(2), at(3)]));
    assert_eq!(port.take_breakpoints(), None);
}

#[test]
fn dropping_the_debugger_releases_a_stopped_runtime() {
    let (link, port) = session();
    let runtime = thread::spawn(move || {
        let resume = port.pause(stop_at(1), &mut Recorder::default());
        (resume, port.stop_requested())
    });
    next(&link);
    drop(link);
    assert_eq!(runtime.join().unwrap(), (Resume::Stop, true));
}

#[test]
fn stop_releases_a_stopped_runtime_and_stays_requested() {
    let (link, port) = session();
    let runtime = thread::spawn(move || {
        let resume = port.pause(stop_at(1), &mut Recorder::default());
        (resume, port.stop_requested())
    });
    next(&link);
    link.stop();
    assert_eq!(runtime.join().unwrap(), (Resume::Stop, true));
}

#[test]
fn stop_is_seen_by_a_runtime_that_never_stopped() {
    let (link, port) = session();
    assert!(!port.stop_requested());
    link.stop();
    assert!(port.stop_requested());
}

#[test]
fn a_runtime_that_pauses_with_no_debugger_does_not_block() {
    let (link, port) = session();
    drop(link);
    assert_eq!(
        port.pause(stop_at(1), &mut Recorder::default()),
        Resume::Stop
    );
}

#[test]
fn the_debugger_learns_that_the_runtime_is_gone() {
    let (link, port) = session();
    drop(port);
    assert!(!link.send(DebugRequest::Resume(Resume::Continue)));
    assert_eq!(link.try_event(), Err(Disconnected));
    assert_eq!(link.wait_event(Duration::from_millis(1)), Err(Disconnected));
}

#[test]
fn polling_with_nothing_to_report_is_not_an_error() {
    let (link, _port) = session();
    assert_eq!(link.try_event(), Ok(None));
    assert_eq!(link.wait_event(Duration::from_millis(1)), Ok(None));
}

#[test]
fn the_debugger_hears_when_the_runtime_goes_idle() {
    let (link, port) = session();
    let notifier = port.notifier();
    thread::spawn(move || notifier.idle()).join().unwrap();
    assert_eq!(next(&link), DebugEvent::Idle);
    drop(port);
    // The notifier outlived nothing: a gone debugger is not an error.
    let (link, port) = session();
    let notifier = port.notifier();
    drop(link);
    notifier.idle();
}
