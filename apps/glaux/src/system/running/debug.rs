//! A script runtime on its own thread, for a run under the debugger.
//!
//! The runtime and everything it owns live on that thread, because neither
//! VM can move between threads. The system talks to it through a
//! [`DebugLink`], which carries the stops, and a pair of channels that
//! carry the phases and what they produced.
//!
//! The start, which runs the entry script and `boot()`, takes the cartridge
//! hardware to the thread. Each `tick()` and `draw()` is a command of its
//! own that moves it there too. The report that answers the start or a phase
//! moves the hardware back, whether the script succeeded or failed. The only
//! way the hardware does not come back is that the thread is gone, which
//! ends the run.
//!
//! Waiting for a phase ends in one of two ways: the runtime finishes it, or
//! it stops at a breakpoint or a step and waits to be resumed. Both are
//! prompt, since a script that runs on is bounded by its budget, so the
//! system never waits on the runtime longer than a run without the debugger
//! would.

use std::cell::{RefCell, RefMut};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use glaux_audio::AudioState;
use glaux_input::InputState;
use glaux_machine::font::BitmapFont;
use glaux_machine::hardware::CartridgeHardware;
use glaux_machine::map_store::MapStore;
use glaux_script::debug::{
    Breakpoint, DebugEvent, DebugLink, DebugReply, DebugRequest, Resume, Stop, session,
};
use glaux_script::{LoanError, LogEntry, Phase, RuntimeError, ScriptError, StartFailure};

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

/// A phase to run, with the hardware it runs on.
enum Command {
    Tick(InputState, Box<CartridgeHardware>),
    Draw(Box<CartridgeHardware>),
}

/// What the runtime thread reports once a call into it is over.
enum Report {
    /// The start ended.
    Started(Box<StartReport>),
    Phase(Box<PhaseReport>),
}

/// How the start ended: the entry script and `boot()`.
struct StartReport {
    /// What the script logged while it ran, or why it failed to start.
    result: Result<(Vec<LogEntry>, u32), StartFailure>,
    /// The hardware the start ran on. It is always here, also after a script
    /// error.
    hardware: Box<CartridgeHardware>,
}

/// How a `tick()` or `draw()` ended.
struct PhaseReport {
    result: Result<(), RuntimeError>,
    /// The hardware the phase ran on. It is always here, also after a script
    /// error.
    hardware: Box<CartridgeHardware>,
    output: Vec<LogEntry>,
    dropped: u32,
    /// A copy of the audio the script wrote, taken before the thread's own
    /// is committed, so each write reaches the PSG once. It comes with the
    /// report that ends the logical tick: after a `draw()`, or after a
    /// `tick()` that failed and so has no `draw()`.
    audio: Option<AudioState>,
}

/// The system's end of a runtime running under the debugger.
#[derive(Debug)]
pub struct DebugRuntime {
    link: DebugLink,
    commands: Option<Sender<Command>>,
    reports: Receiver<Report>,
    thread: Option<JoinHandle<()>>,
    output: Vec<LogEntry>,
    dropped: u32,
    audio: RefCell<AudioState>,
    /// Whether a tick is under way: stopped, or not yet finished.
    ticking: bool,
}

impl DebugRuntime {
    /// Starts `source` on a new thread, stopping at `breakpoints`. The start
    /// itself may stop, so the answer says whether the runtime is waiting.
    ///
    /// The hardware in `home` moves to the thread for the entry script and
    /// `boot()`. It is back in `home` when the start finishes, whether it
    /// succeeded or not. If the start stops, the runtime still holds it and
    /// `home` stays empty until the run is resumed and the start finishes.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        language: Language,
        entry: &str,
        source: Vec<u8>,
        modules: CartridgeModules,
        font: BitmapFont,
        maps: Arc<MapStore>,
        breakpoints: Vec<Breakpoint>,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<(Self, Option<Stop>), StartFailure> {
        let Some(hardware) = home.take() else {
            return Err(StartFailure {
                error: LoanError::Unavailable.in_phase(Phase::Entry),
                output: Vec::new(),
                dropped: 0,
            });
        };
        let (link, port) = session();
        let (commands, command_inbox) = mpsc::channel();
        let (report_outbox, reports) = mpsc::channel();
        let entry = entry.to_owned();
        let notifier = port.notifier();
        let thread = thread::Builder::new()
            .name("debugged-script".into())
            .stack_size(STACK)
            .spawn(move || {
                let mut start_home = Some(hardware);
                let runtime = ScriptRuntime::start_debug(
                    language,
                    &entry,
                    &source,
                    modules,
                    font,
                    maps,
                    &mut start_home,
                    port,
                    breakpoints,
                );
                // The loan hands the hardware back on every path.
                let Some(hardware) = start_home.take() else {
                    return;
                };
                let mut runtime = match runtime {
                    Ok(mut runtime) => {
                        let started = (runtime.take_output(), runtime.take_dropped());
                        let report = StartReport {
                            result: Ok(started),
                            hardware,
                        };
                        let _ = report_outbox.send(Report::Started(Box::new(report)));
                        runtime
                    }
                    Err(failure) => {
                        let report = StartReport {
                            result: Err(failure),
                            hardware,
                        };
                        let _ = report_outbox.send(Report::Started(Box::new(report)));
                        notifier.idle();
                        return;
                    }
                };
                notifier.idle();
                while let Ok(command) = command_inbox.recv() {
                    let (phase, mut home, result) = match command {
                        Command::Tick(input, hardware) => {
                            let mut home = Some(hardware);
                            let result = runtime.run_tick(input, &mut home);
                            (Phase::Tick, home, result)
                        }
                        Command::Draw(hardware) => {
                            let mut home = Some(hardware);
                            let result = runtime.run_draw(&mut home);
                            (Phase::Draw, home, result)
                        }
                    };
                    // The loan always hands the hardware back. If it did not,
                    // the ownership of the hardware is broken and the thread
                    // ends, which the system sees as a lost runtime.
                    let Some(hardware) = home.take() else {
                        return;
                    };
                    let audio = (phase == Phase::Draw || result.is_err()).then(|| {
                        let mut audio = runtime.audio_mut();
                        let copy = audio.clone();
                        audio.commit();
                        copy
                    });
                    let report = PhaseReport {
                        result,
                        hardware,
                        output: runtime.take_output(),
                        dropped: runtime.take_dropped(),
                        audio,
                    };
                    if report_outbox.send(Report::Phase(Box::new(report))).is_err() {
                        return;
                    }
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
            output: Vec::new(),
            dropped: 0,
            audio: RefCell::default(),
            ticking: false,
        };
        match runtime.wait(Phase::Entry, home) {
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
    /// The hardware the phase ran on comes back into `home` with the report.
    /// While the runtime is stopped it still holds it.
    fn wait(
        &mut self,
        phase: Phase,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<Tick, RuntimeError> {
        self.ticking = true;
        loop {
            match self.link.wait_event(POLL) {
                Ok(Some(DebugEvent::Stopped(stop))) => return Ok(Tick::Stopped(stop)),
                Ok(Some(DebugEvent::Idle)) => {
                    self.ticking = false;
                    return self.collect(phase, home);
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
    /// ended, or what a phase produced and the hardware it ran on.
    fn collect(
        &mut self,
        phase: Phase,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<Tick, RuntimeError> {
        match self.reports.try_recv() {
            Ok(Report::Started(report)) => {
                let StartReport { result, hardware } = *report;
                *home = Some(hardware);
                match result {
                    Ok((output, dropped)) => {
                        self.output.extend(output);
                        self.dropped += dropped;
                        Ok(Tick::Done)
                    }
                    Err(failure) => {
                        self.output.extend(failure.output);
                        self.dropped += failure.dropped;
                        Err(failure.error)
                    }
                }
            }
            Ok(Report::Phase(report)) => {
                let PhaseReport {
                    result,
                    hardware,
                    output,
                    dropped,
                    audio,
                } = *report;
                *home = Some(hardware);
                self.output.extend(output);
                self.dropped += dropped;
                if let Some(audio) = audio {
                    *self.audio.get_mut() = audio;
                }
                result.map(|()| Tick::Done)
            }
            Err(_) => Err(lost(phase)),
        }
    }

    /// Runs `tick()` on the hardware in `home`, until it finishes or stops.
    pub fn run_tick(
        &mut self,
        input: InputState,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<Tick, RuntimeError> {
        self.dispatch(Phase::Tick, |hardware| Command::Tick(input, hardware), home)
    }

    /// Runs `draw()` on the hardware in `home`, until it finishes or stops.
    pub fn run_draw(
        &mut self,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<Tick, RuntimeError> {
        self.dispatch(Phase::Draw, Command::Draw, home)
    }

    /// Sends a phase to the thread together with the hardware, and waits.
    ///
    /// With no hardware in `home` the phase has nothing to run on, and
    /// nothing is sent. If the thread is gone the command goes nowhere and
    /// takes the hardware with it. The run cannot go on without its thread.
    fn dispatch(
        &mut self,
        phase: Phase,
        command: impl FnOnce(Box<CartridgeHardware>) -> Command,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<Tick, RuntimeError> {
        let Some(hardware) = home.take() else {
            return Err(LoanError::Unavailable.in_phase(phase));
        };
        let sent = self
            .commands
            .as_ref()
            .is_some_and(|commands| commands.send(command(hardware)).is_ok());
        if !sent {
            return Err(lost(phase));
        }
        self.wait(phase, home)
    }

    /// Resumes a runtime stopped in `phase`, until the phase finishes or it
    /// stops again. The hardware is still with the runtime and comes back
    /// into `home` when the phase finishes.
    pub fn resume(
        &mut self,
        resume: Resume,
        phase: Phase,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<Tick, RuntimeError> {
        if !self.link.send(DebugRequest::Resume(resume)) {
            return Err(lost(phase));
        }
        self.wait(phase, home)
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

#[cfg(test)]
mod tests;
