//! A script runtime on its own thread, for a run under the debugger.
//!
//! The runtime and everything it owns live on that thread, because neither
//! VM can move between threads. The system talks to it through a
//! [`DebugLink`], which carries the stops, and a pair of channels that
//! carry the ticks and what they produced.
//!
//! Waiting for a tick ends in one of two ways: the runtime finishes it, or
//! it stops at a breakpoint or a step and waits to be resumed. Both are
//! prompt, since a script that runs on is bounded by its budget, so the
//! system never waits on the runtime longer than a run without the debugger
//! would.

use std::cell::{RefCell, RefMut};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use glaux_audio::AudioState;
use glaux_input::InputState;
use glaux_script::debug::{
    Breakpoint, DebugEvent, DebugLink, DebugReply, DebugRequest, Resume, Stop, session,
};
use glaux_script::{DrawCommand, LogEntry, Phase, RuntimeError, ScriptError, StartFailure};

use super::ScriptRuntime;
use crate::cartridge::{CartridgeModules, Language};

/// Stack of the thread a debugged runtime runs on. Rhai recurses on the
/// native stack up to its call limit and needs more than a spawned thread
/// gets by default, and the debugger's hooks run on top of it.
const STACK: usize = 8 * 1024 * 1024;

/// How often a wait for the runtime looks again at a runtime that is gone.
const POLL: Duration = Duration::from_millis(50);

/// How a tick under the debugger ended, so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tick {
    /// The tick finished.
    Done,
    /// The runtime stopped inside the tick and waits to be resumed.
    Stopped(Stop),
}

enum Command {
    Tick(InputState),
}

/// What the runtime thread reports once a call into it is over.
enum Report {
    /// The start ended, with what the script logged while it ran.
    Started(Result<(Vec<LogEntry>, u32), StartFailure>),
    Ticked(Box<Ticked>),
}

struct Ticked {
    result: Result<(), RuntimeError>,
    frame: Vec<DrawCommand>,
    output: Vec<LogEntry>,
    dropped: u32,
    /// A copy of the audio the script wrote, taken before the thread's own
    /// is committed, so each write reaches the PSG once.
    audio: AudioState,
}

/// The system's end of a runtime running under the debugger.
#[derive(Debug)]
pub struct DebugRuntime {
    link: DebugLink,
    commands: Option<Sender<Command>>,
    reports: Receiver<Report>,
    thread: Option<JoinHandle<()>>,
    /// What the last finished tick left, as [`ScriptRuntime`] would hold it.
    frame: Vec<DrawCommand>,
    output: Vec<LogEntry>,
    dropped: u32,
    audio: RefCell<AudioState>,
    /// Whether a tick is under way: stopped, or not yet finished.
    ticking: bool,
}

impl DebugRuntime {
    /// Starts `source` on a new thread, stopping at `breakpoints`. The start
    /// itself may stop, so the answer says whether the runtime is waiting.
    pub fn start(
        language: Language,
        entry: &str,
        source: Vec<u8>,
        modules: CartridgeModules,
        breakpoints: Vec<Breakpoint>,
    ) -> Result<(Self, Option<Stop>), StartFailure> {
        let (link, port) = session();
        let (commands, command_inbox) = mpsc::channel();
        let (report_outbox, reports) = mpsc::channel();
        let entry = entry.to_owned();
        let notifier = port.notifier();
        let thread = thread::Builder::new()
            .name("debugged-script".into())
            .stack_size(STACK)
            .spawn(move || {
                let runtime = ScriptRuntime::start_debug(
                    language,
                    &entry,
                    &source,
                    modules,
                    port,
                    breakpoints,
                );
                let mut runtime = match runtime {
                    Ok(mut runtime) => {
                        let started = (runtime.take_output(), runtime.take_dropped());
                        let _ = report_outbox.send(Report::Started(Ok(started)));
                        runtime
                    }
                    Err(failure) => {
                        let _ = report_outbox.send(Report::Started(Err(failure)));
                        notifier.idle();
                        return;
                    }
                };
                notifier.idle();
                while let Ok(Command::Tick(input)) = command_inbox.recv() {
                    let result = runtime.tick(input);
                    let audio = {
                        let mut audio = runtime.audio_mut();
                        let copy = audio.clone();
                        audio.commit();
                        copy
                    };
                    let report = Ticked {
                        result,
                        frame: runtime.frame().to_vec(),
                        output: runtime.take_output(),
                        dropped: runtime.take_dropped(),
                        audio,
                    };
                    let _ = report_outbox.send(Report::Ticked(Box::new(report)));
                    notifier.idle();
                }
            });
        let thread = match thread {
            Ok(thread) => Some(thread),
            Err(err) => {
                return Err(StartFailure {
                    error: RuntimeError {
                        phase: Phase::Entry,
                        error: ScriptError::Runtime(format!("cannot start a thread: {err}")),
                    },
                    output: Vec::new(),
                    dropped: 0,
                });
            }
        };
        let mut runtime = Self {
            link,
            commands: Some(commands),
            reports,
            thread,
            frame: Vec::new(),
            output: Vec::new(),
            dropped: 0,
            audio: RefCell::default(),
            ticking: false,
        };
        match runtime.wait(Phase::Entry) {
            Ok(Tick::Stopped(stop)) => Ok((runtime, Some(stop))),
            Ok(Tick::Done) => Ok((runtime, None)),
            Err(error) => Err(StartFailure {
                error,
                output: std::mem::take(&mut runtime.output),
                dropped: std::mem::take(&mut runtime.dropped),
            }),
        }
    }

    /// Waits until the runtime is idle, and reports what it did, or stopped.
    fn wait(&mut self, phase: Phase) -> Result<Tick, RuntimeError> {
        self.ticking = true;
        loop {
            match self.link.wait_event(POLL) {
                Ok(Some(DebugEvent::Stopped(stop))) => return Ok(Tick::Stopped(stop)),
                Ok(Some(DebugEvent::Idle)) => {
                    self.ticking = false;
                    return self.collect(phase);
                }
                Ok(Some(DebugEvent::Reply(_))) | Ok(None) => {}
                Err(_) => {
                    self.ticking = false;
                    return Err(lost(phase));
                }
            }
        }
    }

    /// Takes what the thread reported when it went idle: that the start
    /// ended, or what a tick produced.
    fn collect(&mut self, phase: Phase) -> Result<Tick, RuntimeError> {
        match self.reports.try_recv() {
            Ok(Report::Started(Ok((output, dropped)))) => {
                self.output.extend(output);
                self.dropped += dropped;
                Ok(Tick::Done)
            }
            Ok(Report::Started(Err(failure))) => {
                self.output.extend(failure.output);
                self.dropped += failure.dropped;
                Err(failure.error)
            }
            Ok(Report::Ticked(ticked)) => {
                let Ticked {
                    result,
                    frame,
                    output,
                    dropped,
                    audio,
                } = *ticked;
                self.frame = frame;
                self.output.extend(output);
                self.dropped += dropped;
                *self.audio.get_mut() = audio;
                result.map(|()| Tick::Done)
            }
            Err(_) => Err(lost(phase)),
        }
    }

    /// Runs one tick, until it finishes or stops.
    pub fn tick(&mut self, input: InputState) -> Result<Tick, RuntimeError> {
        let sent = self
            .commands
            .as_ref()
            .is_some_and(|commands| commands.send(Command::Tick(input)).is_ok());
        if !sent {
            return Err(lost(Phase::Tick));
        }
        self.wait(Phase::Tick)
    }

    /// Resumes a stopped runtime, until its tick finishes or it stops again.
    pub fn resume(&mut self, resume: Resume) -> Result<Tick, RuntimeError> {
        if !self.link.send(DebugRequest::Resume(resume)) {
            return Err(lost(Phase::Tick));
        }
        self.wait(Phase::Tick)
    }

    /// Asks a stopped runtime something and waits for its answer.
    pub fn ask(&mut self, request: DebugRequest) -> DebugReply {
        if !self.link.send(request) {
            return DebugReply::Unavailable;
        }
        loop {
            match self.link.wait_event(POLL) {
                Ok(Some(DebugEvent::Reply(reply))) => return reply,
                Ok(Some(_)) | Ok(None) => {}
                Err(_) => return DebugReply::Unavailable,
            }
        }
    }

    /// Reads what the stopped runtime logged since the last call.
    pub fn pull_output(&mut self) {
        if let DebugReply::Output { entries, dropped } = self.ask(DebugRequest::Output) {
            self.output.extend(entries);
            self.dropped += dropped;
        }
    }

    /// Sends the breakpoints the runtime should stop at from now on.
    pub fn set_breakpoints(&self, breakpoints: Vec<Breakpoint>) {
        self.link.send(DebugRequest::Breakpoints(breakpoints));
    }

    pub fn frame(&self) -> &[DrawCommand] {
        &self.frame
    }

    pub fn take_output(&mut self) -> Vec<LogEntry> {
        std::mem::take(&mut self.output)
    }

    pub fn take_dropped(&mut self) -> u32 {
        std::mem::take(&mut self.dropped)
    }

    /// The audio the last finished tick wrote.
    pub fn audio_mut(&self) -> RefMut<'_, AudioState> {
        self.audio.borrow_mut()
    }
}

impl Drop for DebugRuntime {
    fn drop(&mut self) {
        // Ends a call in progress, then the thread's loop.
        self.link.stop();
        self.commands = None;
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::warn!("the debugged script's thread panicked");
        }
    }
}

/// The runtime thread ended without answering.
fn lost(phase: Phase) -> RuntimeError {
    RuntimeError {
        phase,
        error: ScriptError::Runtime("the script's thread ended unexpectedly".into()),
    }
}
