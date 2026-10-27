//! What a debugger and a script runtime say to each other.
//!
//! A runtime that is being debugged runs on its own thread. When a
//! breakpoint or a step stops it, it calls [`DebugPort::pause`], which
//! publishes a [`Stop`] and blocks until the debugger sends a
//! [`Resume`]. The debugger holds the other end, a [`DebugLink`], and never
//! blocks on the runtime except to wait for the answer to a request it
//! made.
//!
//! The two ends share nothing but channels and one atomic flag, so a stopped
//! runtime never holds anything the debugger needs. Dropping the link
//! releases a stopped runtime with [`Resume::Stop`].

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::Duration;

/// A line of a script where execution should stop.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Breakpoint {
    /// Cartridge-relative path of the script, as in `code/main.luau`.
    pub path: String,
    /// 1-based. A runtime may stop on a later line when this one holds no
    /// code.
    pub line: u32,
}

/// One entry of the call stack, innermost first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub function: String,
    /// `None` for frames that are not script code, such as the engine.
    pub path: Option<String>,
    /// 1-based.
    pub line: Option<u32>,
    /// Whether [`DebugRequest::Variables`] can answer for this frame.
    /// Rhai only exposes the variables of the innermost frame.
    pub has_variables: bool,
}

/// A named value, rendered without running any script code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    pub value: String,
    /// Whether [`DebugRequest::Expand`] can list its entries.
    pub expandable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Breakpoint,
    Step,
}

/// The state of a runtime that has stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stop {
    pub reason: StopReason,
    /// Innermost first. The first frame is where the runtime stopped.
    pub frames: Vec<Frame>,
    /// The variables of the first frame.
    pub variables: Vec<Variable>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// To the next line of the current function, skipping calls.
    Over,
    /// To the next line, entering calls.
    Into,
    /// Until the current function returns.
    Out,
}

/// How a stopped runtime goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resume {
    Continue,
    Step(Step),
    /// End the call with [`ScriptError::Stopped`](crate::ScriptError::Stopped).
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugRequest {
    Resume(Resume),
    /// The variables of a frame of the current [`Stop`].
    Variables {
        frame: usize,
    },
    /// The entries of a table or map. `path` is the index of the variable
    /// in the frame's list, followed by the index of the entry at each
    /// level. Indexes follow the order the previous answers listed.
    Expand {
        frame: usize,
        path: Vec<usize>,
    },
    /// Replaces the breakpoints. A runtime applies them when it next stops
    /// or when it is next asked to.
    Breakpoints(Vec<Breakpoint>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugReply {
    Variables(Vec<Variable>),
    /// The runtime cannot answer, for instance for an outer Rhai frame.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugEvent {
    Stopped(Stop),
    Reply(DebugReply),
}

/// What a runtime can tell about the state it stopped in.
pub trait Inspect {
    fn variables(&mut self, frame: usize) -> DebugReply;
    fn expand(&mut self, frame: usize, path: &[usize]) -> DebugReply;
    /// The debugger replaced its breakpoints while the runtime was stopped.
    fn breakpoints(&mut self, breakpoints: &[Breakpoint]);
}

/// Creates the two ends of a debugging session.
pub fn session() -> (DebugLink, DebugPort) {
    let (requests, runtime_requests) = mpsc::channel();
    let (runtime_events, events) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    (
        DebugLink {
            requests,
            events,
            stop: Arc::clone(&stop),
        },
        DebugPort {
            events: runtime_events,
            requests: runtime_requests,
            stop,
        },
    )
}

/// The debugger's end.
#[derive(Debug)]
pub struct DebugLink {
    requests: Sender<DebugRequest>,
    events: Receiver<DebugEvent>,
    stop: Arc<AtomicBool>,
}

impl DebugLink {
    /// Sends a request. Returns `false` when the runtime is gone.
    pub fn send(&self, request: DebugRequest) -> bool {
        self.requests.send(request).is_ok()
    }

    /// The next event if one is waiting.
    pub fn try_event(&self) -> Result<Option<DebugEvent>, Disconnected> {
        match self.events.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(Disconnected),
        }
    }

    /// Waits for the next event, for at most `limit`.
    pub fn wait_event(&self, limit: Duration) -> Result<Option<DebugEvent>, Disconnected> {
        match self.events.recv_timeout(limit) {
            Ok(event) => Ok(Some(event)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(Disconnected),
        }
    }

    /// Ends the runtime's current call, whether it is running or stopped.
    ///
    /// The flag is checked by the runtime at its safepoints, so a script
    /// that catches the resulting error cannot keep running.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.requests.send(DebugRequest::Resume(Resume::Stop));
    }
}

/// The runtime's end of a debugging session has gone away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disconnected;

/// The runtime's end.
#[derive(Debug)]
pub struct DebugPort {
    events: Sender<DebugEvent>,
    requests: Receiver<DebugRequest>,
    stop: Arc<AtomicBool>,
}

impl DebugPort {
    /// Whether the debugger asked to end the run. Sticky: once set it stays
    /// set, so it is safe to check from an interrupt.
    pub fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    /// Records that the run is ending, for runtimes that decide to end it.
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Publishes `stop` and blocks until the debugger resumes the runtime.
    /// Requests for variables are answered through `inspect` meanwhile. A
    /// debugger that went away resumes with [`Resume::Stop`].
    pub fn pause(&self, stop: Stop, inspect: &mut dyn Inspect) -> Resume {
        if self.events.send(DebugEvent::Stopped(stop)).is_err() {
            self.request_stop();
            return Resume::Stop;
        }
        loop {
            let reply = match self.requests.recv() {
                Ok(DebugRequest::Resume(resume)) => {
                    if resume == Resume::Stop {
                        self.request_stop();
                    }
                    return resume;
                }
                Ok(DebugRequest::Variables { frame }) => inspect.variables(frame),
                Ok(DebugRequest::Expand { frame, path }) => inspect.expand(frame, &path),
                Ok(DebugRequest::Breakpoints(breakpoints)) => {
                    inspect.breakpoints(&breakpoints);
                    continue;
                }
                Err(_) => {
                    self.request_stop();
                    return Resume::Stop;
                }
            };
            if self.events.send(DebugEvent::Reply(reply)).is_err() {
                self.request_stop();
                return Resume::Stop;
            }
        }
    }

    /// The latest breakpoints the debugger sent while the runtime was
    /// running, if it sent any. Other requests are dropped: they only make
    /// sense while the runtime is stopped.
    pub fn take_breakpoints(&self) -> Option<Vec<Breakpoint>> {
        let mut latest = None;
        while let Ok(request) = self.requests.try_recv() {
            match request {
                DebugRequest::Breakpoints(breakpoints) => latest = Some(breakpoints),
                DebugRequest::Resume(Resume::Stop) => self.request_stop(),
                _ => {}
            }
        }
        latest
    }
}
