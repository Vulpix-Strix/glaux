//! A loaded cartridge together with the script runtime running it.

mod debug;

use std::cell::RefMut;
use std::sync::Arc;

use glaux_audio::AudioState;
use glaux_input::{InputEvent, InputLatch, InputState};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::hardware::CartridgeHardware;
use glaux_machine::map_store::MapStore;
use glaux_machine::palette::Palette;
use glaux_machine::psg::Psg;
use glaux_machine::published::PublishedFrame;
use glaux_script::debug::{Breakpoint, DebugReply, DebugRequest, Resume, Stop};
use glaux_script::{LogEntry, Phase, RuntimeError, ScriptError, StartFailure};
use glaux_video::compose::compose_layers;

use self::debug::DebugRuntime;
pub use self::debug::Tick;
use super::audio;
use super::maps::{self, RunOverrides};
use super::video::{CartridgeVideo, CartridgeVideoDefaults, VideoAssetError, VideoLoadError};
use crate::cartridge::map::{MapError, MapId};
use crate::cartridge::{CartridgeError, CartridgeModules, Language, LoadedCartridge};

/// Why a cartridge could not start.
#[derive(Debug)]
pub enum StartError {
    /// The entry script could not be read.
    Cartridge(CartridgeError),
    /// A video asset was not valid native data.
    Video(VideoAssetError),
    /// A map of a Luau cartridge could not be read.
    Maps { id: MapId, error: MapError },
    /// The entry chunk or `boot()` failed.
    Script(StartFailure),
}

/// A cartridge that could not start, handed back still loaded.
#[derive(Debug)]
pub struct FailedStart {
    pub cartridge: LoadedCartridge,
    pub error: StartError,
}

/// Where a run was started from, which decides where it returns to when it
/// ends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RunOrigin {
    /// The shell's `run` command. A run that ends returns to the shell.
    #[default]
    Shell,
    /// The Code editor. A run that ends returns to the editor.
    Code,
}

/// The runtime of the language a cartridge's manifest names.
#[derive(Debug)]
enum ScriptRuntime {
    Luau(glaux_luau::Runtime),
    /// A runtime on a thread of its own, under the debugger.
    Debugged(Box<DebugRuntime>),
}

impl ScriptRuntime {
    /// Starts `source` with the default budget of `language`'s runtime.
    ///
    /// Luau lends the hardware in `home` to the entry script and `boot()`.
    fn start(
        language: Language,
        entry: &str,
        source: &[u8],
        modules: CartridgeModules,
        font: BitmapFont,
        maps: Arc<MapStore>,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<Self, StartFailure> {
        match language {
            Language::Luau => glaux_luau::Runtime::start(
                entry,
                source,
                modules,
                glaux_luau::Budget::default(),
                font,
                maps,
                home,
            )
            .map(Self::Luau),
        }
    }

    /// Starts `source` under the debugger with the default budget, to be
    /// driven by whoever holds the other end of `port`. Called on the thread
    /// the runtime will live on. See [`start`](Self::start) for `home`.
    #[allow(clippy::too_many_arguments)]
    fn start_debug(
        language: Language,
        entry: &str,
        source: &[u8],
        modules: CartridgeModules,
        font: BitmapFont,
        maps: Arc<MapStore>,
        home: &mut Option<Box<CartridgeHardware>>,
        port: glaux_script::debug::DebugPort,
        breakpoints: Vec<Breakpoint>,
    ) -> Result<Self, StartFailure> {
        match language {
            Language::Luau => glaux_luau::Runtime::start_debug(
                entry,
                source,
                modules,
                glaux_luau::Budget::default(),
                font,
                maps,
                home,
                port,
                breakpoints,
            )
            .map(Self::Luau),
        }
    }

    /// Runs the `tick()` phase with the hardware lent from `home`. A
    /// debugged runtime runs its phases through [`DebugRuntime`], see
    /// [`RunningCartridge::advance`].
    fn run_tick(
        &mut self,
        input: InputState,
        home: &mut Option<Box<CartridgeHardware>>,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Luau(runtime) => runtime.run_tick(input, home),
            Self::Debugged(_) => {
                unreachable!("a debugged runtime runs its phases through `advance`")
            }
        }
    }

    /// Runs the `draw()` phase, as [`run_tick`](Self::run_tick) does.
    fn run_draw(&mut self, home: &mut Option<Box<CartridgeHardware>>) -> Result<(), RuntimeError> {
        match self {
            Self::Luau(runtime) => runtime.run_draw(home),
            Self::Debugged(_) => {
                unreachable!("a debugged runtime runs its phases through `advance`")
            }
        }
    }

    fn take_output(&mut self) -> Vec<LogEntry> {
        match self {
            Self::Luau(runtime) => runtime.take_output(),
            Self::Debugged(runtime) => runtime.take_output(),
        }
    }

    fn take_dropped(&mut self) -> u32 {
        match self {
            Self::Luau(runtime) => runtime.take_dropped(),
            Self::Debugged(runtime) => runtime.take_dropped(),
        }
    }

    fn audio_mut(&mut self) -> RefMut<'_, AudioState> {
        match self {
            Self::Luau(runtime) => runtime.audio_mut(),
            Self::Debugged(runtime) => runtime.audio_mut(),
        }
    }
}

/// A cartridge whose VM is alive. Dropping it disposes of the VM.
#[derive(Debug)]
pub struct RunningCartridge {
    cartridge: LoadedCartridge,
    runtime: ScriptRuntime,
    /// The one canonical hardware of the run. It is `None` only while the
    /// runtime holds it for a phase, and a phase hands it back before
    /// returning, with one exception: a thread that died under the debugger
    /// takes it along, and the run ends. See [`hardware`](Self::hardware).
    hardware: Option<Box<CartridgeHardware>>,
    /// What the user sees: the work framebuffer and master palette as the
    /// last `draw()` that finished left them. It changes only when one
    /// does, so a failed or stopped `draw()` never shows a partial frame.
    published: PublishedFrame,
    /// The phase a debugged run is stopped in. Only a stop in `tick()` is
    /// followed by a `draw()` when the run is resumed. A stop while starting
    /// is recorded as [`Phase::Entry`] whether it was in the entry script or
    /// in `boot()`.
    stopped_in: Option<Phase>,
    /// Starts with every button and key released, so nothing held during a
    /// previous run reaches this one, and is reset whenever the system
    /// resumes the cartridge from a pause.
    input: InputLatch,
    origin: RunOrigin,
}

impl RunningCartridge {
    /// Creates a fresh VM for `cartridge` and runs its entry script and
    /// `boot()`, with `defaults` standing in for video assets the cartridge
    /// does not supply. The assets in `overrides` replace what the
    /// cartridge's files hold, so a run can show edits not written back.
    /// `font` is the System ROM font that `print` draws with. On failure the
    /// cartridge is handed back, still loaded.
    pub fn start<'a>(
        cartridge: LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
        overrides: impl Into<RunOverrides<'a>>,
        font: &BitmapFont,
    ) -> Result<Self, Box<FailedStart>> {
        let Prepared {
            cartridge,
            source,
            hardware,
            maps,
        } = prepare(cartridge, defaults, overrides.into())?;
        // Taken before any script runs, so the first frame shown is the same
        // whether or not the start stopped in the debugger.
        let published = PublishedFrame::new(&hardware);
        let mut home = Some(hardware);
        let spec = &cartridge.manifest().runtime;
        let started = ScriptRuntime::start(
            spec.language,
            &spec.entry,
            &source,
            cartridge.modules(),
            font.clone(),
            maps,
            &mut home,
        );
        match started {
            Ok(runtime) => Ok(Self::new(cartridge, runtime, home, published)),
            Err(failure) => Err(failed(cartridge, StartError::Script(failure))),
        }
    }

    /// Like [`start`](Self::start), for a run under the debugger that stops
    /// at `breakpoints`. The runtime lives on a thread of its own. The start
    /// itself can stop, at a breakpoint in the entry script or in `boot()`,
    /// and then the stop comes with the cartridge and the run goes on with
    /// [`resume`](Self::resume).
    pub fn start_debug<'a>(
        cartridge: LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
        overrides: impl Into<RunOverrides<'a>>,
        font: &BitmapFont,
        breakpoints: Vec<Breakpoint>,
    ) -> Result<(Self, Option<Stop>), Box<FailedStart>> {
        let Prepared {
            cartridge,
            source,
            hardware,
            maps,
        } = prepare(cartridge, defaults, overrides.into())?;
        let published = PublishedFrame::new(&hardware);
        let mut home = Some(hardware);
        let spec = &cartridge.manifest().runtime;
        match DebugRuntime::start(
            spec.language,
            &spec.entry,
            source,
            cartridge.modules(),
            font.clone(),
            maps,
            breakpoints,
            &mut home,
        ) {
            Ok((runtime, stop)) => {
                let runtime = ScriptRuntime::Debugged(Box::new(runtime));
                let mut running = Self::new(cartridge, runtime, home, published);
                running.stopped_in = stop.as_ref().map(|_| Phase::Entry);
                Ok((running, stop))
            }
            Err(failure) => Err(failed(cartridge, StartError::Script(failure))),
        }
    }

    /// `hardware` is `None` when a debugged start stopped before it
    /// finished, and the runtime still holds it. `published` is the frame
    /// of the hardware before any script ran.
    fn new(
        cartridge: LoadedCartridge,
        runtime: ScriptRuntime,
        hardware: Option<Box<CartridgeHardware>>,
        published: PublishedFrame,
    ) -> Self {
        Self {
            cartridge,
            runtime,
            published,
            hardware,
            stopped_in: None,
            input: InputLatch::new(),
            origin: RunOrigin::default(),
        }
    }

    fn debugged(&mut self) -> Option<&mut DebugRuntime> {
        match &mut self.runtime {
            ScriptRuntime::Debugged(runtime) => Some(runtime),
            ScriptRuntime::Luau(_) => None,
        }
    }

    /// Whether the run is under the debugger.
    pub fn is_debugged(&self) -> bool {
        matches!(self.runtime, ScriptRuntime::Debugged(_))
    }

    /// Advances the cartridge by one logical tick: its `tick()` and then its
    /// `draw()`. Under the debugger either can also stop at a breakpoint or
    /// a step, and then the run waits to be [resumed](Self::resume). A run
    /// that is not debugged never stops.
    ///
    /// Each phase borrows the hardware and returns it, so between the two
    /// it is here. A failed `tick()` skips `draw()`.
    ///
    /// A run stopped in the debugger has to be [resumed](Self::resume)
    /// first. Advancing it is refused before the input is read and before
    /// anything is sent to the debugged thread.
    #[hotpath::measure(label = "running::advance", impl_type = "RunningCartridge")]
    pub fn advance(&mut self, input: &[InputEvent]) -> Result<Tick, RuntimeError> {
        if let Some(phase) = self.stopped_in {
            return Err(RuntimeError {
                phase,
                error: ScriptError::Runtime(
                    "internal error: the run is stopped in the debugger and must be resumed first"
                        .into(),
                ),
            });
        }
        let state = self.input.advance(input);
        match self.run_tick(state)? {
            Tick::Done => self.run_draw(),
            stopped @ Tick::Stopped(_) => Ok(stopped),
        }
    }

    fn run_tick(&mut self, state: InputState) -> Result<Tick, RuntimeError> {
        let tick = match &mut self.runtime {
            ScriptRuntime::Debugged(runtime) => runtime.run_tick(state, &mut self.hardware)?,
            runtime => {
                runtime.run_tick(state, &mut self.hardware)?;
                Tick::Done
            }
        };
        Ok(self.note_stop(Phase::Tick, tick))
    }

    /// Starts the frame of a tick that finished its `tick()`: the work
    /// framebuffer is cleared to the backdrop color, the background layers are
    /// composed over it, and then `draw()` draws on top. Whatever `tick()` left
    /// in the framebuffer is gone.
    ///
    /// A `draw()` that resumes from a breakpoint does not come through here,
    /// so the frame is composed once per tick.
    fn run_draw(&mut self) -> Result<Tick, RuntimeError> {
        if let Some(hardware) = self.hardware.as_deref_mut() {
            hardware
                .framebuffer
                .clear(hardware.registers.backdrop_color());
            compose_frame(hardware);
        }
        let tick = match &mut self.runtime {
            ScriptRuntime::Debugged(runtime) => runtime.run_draw(&mut self.hardware)?,
            runtime => {
                runtime.run_draw(&mut self.hardware)?;
                Tick::Done
            }
        };
        Ok(self.finish_draw(tick))
    }

    /// Publishes the frame if `draw()` finished, and otherwise remembers
    /// that it stopped. Only a `draw()` that finished publishes. The
    /// hardware keeps whatever a failed one wrote, but the frame shown does
    /// not change.
    fn finish_draw(&mut self, tick: Tick) -> Tick {
        if matches!(tick, Tick::Done)
            && let Some(hardware) = self.hardware.as_deref()
        {
            self.published.publish(hardware);
        }
        self.note_stop(Phase::Draw, tick)
    }

    /// Remembers which phase a debugged run stopped in.
    fn note_stop(&mut self, phase: Phase, tick: Tick) -> Tick {
        if matches!(tick, Tick::Stopped(_)) {
            self.stopped_in = Some(phase);
        }
        tick
    }

    /// Resumes a stopped, debugged cartridge. The input held while it was
    /// stopped is forgotten.
    ///
    /// The phase it stopped in finishes first. If that was `tick()`, the
    /// `draw()` of the same tick follows, and can stop in turn. A run that
    /// stopped while starting just finishes starting.
    pub fn resume(&mut self, resume: Resume) -> Result<Tick, RuntimeError> {
        self.input = InputLatch::new();
        let (Some(phase), ScriptRuntime::Debugged(runtime)) =
            (self.stopped_in.take(), &mut self.runtime)
        else {
            return Ok(Tick::Done);
        };
        let tick = runtime.resume(resume, phase, &mut self.hardware)?;
        match tick {
            Tick::Done if phase == Phase::Tick => self.run_draw(),
            tick if phase == Phase::Draw => Ok(self.finish_draw(tick)),
            tick => Ok(self.note_stop(phase, tick)),
        }
    }

    /// Asks a stopped, debugged cartridge for its variables or its output.
    pub fn ask(&mut self, request: DebugRequest) -> DebugReply {
        match self.debugged() {
            Some(runtime) => runtime.ask(request),
            None => DebugReply::Unavailable,
        }
    }

    /// Moves what a stopped, debugged cartridge logged so far to where
    /// [`take_output`](Self::take_output) finds it.
    pub fn pull_stopped_output(&mut self) {
        if let Some(runtime) = self.debugged() {
            runtime.pull_output();
        }
    }

    /// Tells a debugged cartridge where to stop from now on.
    pub fn set_breakpoints(&mut self, breakpoints: Vec<Breakpoint>) {
        if let Some(runtime) = self.debugged() {
            runtime.set_breakpoints(breakpoints);
        }
    }

    /// Marks the run as started from `origin`.
    pub fn with_origin(mut self, origin: RunOrigin) -> Self {
        self.origin = origin;
        self
    }

    pub fn origin(&self) -> RunOrigin {
        self.origin
    }

    pub fn cartridge(&self) -> &LoadedCartridge {
        &self.cartridge
    }

    /// The hardware, for tests that read it.
    ///
    /// It is present whenever no phase holds it. While a debugged run is
    /// stopped in a phase the runtime holds it, and the system does not
    /// read it.
    #[cfg(test)]
    fn hardware(&self) -> &CartridgeHardware {
        self.hardware
            .as_deref()
            .expect("the hardware is back with the system between phases")
    }

    /// The hardware, for tests that write to it.
    #[cfg(test)]
    fn hardware_mut(&mut self) -> &mut CartridgeHardware {
        self.hardware
            .as_deref_mut()
            .expect("the hardware is back with the system between phases")
    }

    /// The master palette of the published frame. A palette change made
    /// since does not recolor it until the next frame is published.
    pub fn palette(&self) -> &Palette {
        self.published.palette()
    }

    /// Advances the cartridge by one logical tick, applying the button and
    /// key events in `input` received since the previous tick.
    #[cfg(test)]
    pub fn tick(&mut self, input: &[InputEvent]) -> Result<(), RuntimeError> {
        self.advance(input).map(drop)
    }

    /// Releases every button and key as seen by the cartridge, so its next
    /// tick sees nothing held and no presses from before this call.
    pub fn reset_input(&mut self) {
        self.input = InputLatch::new();
    }

    /// Entries the cartridge logged since the last call.
    pub fn take_output(&mut self) -> Vec<LogEntry> {
        self.runtime.take_output()
    }

    /// How many entries the cartridge logged that were refused since the
    /// last call, because it logged more than a tick keeps.
    pub fn take_dropped(&mut self) -> u32 {
        self.runtime.take_dropped()
    }

    /// Copies the published frame into `framebuffer`, which is what gets
    /// presented. The published frame itself is never drawn over.
    #[hotpath::measure(label = "running::render", impl_type = "RunningCartridge")]
    pub fn render(&self, framebuffer: &mut Framebuffer) {
        framebuffer.copy_from(self.published.framebuffer());
    }

    /// Writes the cartridge's audio into `psg` and commits it, as
    /// [`audio::apply`] describes.
    pub fn apply_audio(&mut self, psg: &mut Psg, fresh: bool) {
        audio::apply(&mut self.runtime.audio_mut(), psg, fresh);
    }

    /// Disposes of the VM and gives back the cartridge.
    pub fn stop(self) -> LoadedCartridge {
        self.cartridge
    }
}

/// Composes the layers over the cleared framebuffer. It is a function of its
/// own so that hotpath can time it apart from `draw()`.
#[hotpath::measure(label = "running::compose")]
fn compose_frame(hardware: &mut CartridgeHardware) {
    compose_layers(hardware);
}

/// What a run needs, read from its cartridge.
struct Prepared {
    cartridge: LoadedCartridge,
    source: Vec<u8>,
    hardware: Box<CartridgeHardware>,
    maps: Arc<MapStore>,
}

/// Reads the entry script, the video assets and, for Luau, the maps a run
/// needs.
fn prepare(
    cartridge: LoadedCartridge,
    defaults: &CartridgeVideoDefaults,
    overrides: RunOverrides<'_>,
) -> Result<Prepared, Box<FailedStart>> {
    let source = match cartridge.entry_source() {
        Ok(source) => source,
        Err(err) => return Err(failed(cartridge, StartError::Cartridge(err))),
    };
    let hardware = match CartridgeVideo::load(&cartridge, defaults) {
        Ok(mut video) => {
            overrides.video.apply(&mut video);
            Box::new(video.into_hardware())
        }
        Err(VideoLoadError::Cartridge(err)) => {
            return Err(failed(cartridge, StartError::Cartridge(err)));
        }
        Err(VideoLoadError::Asset(err)) => {
            return Err(failed(cartridge, StartError::Video(err)));
        }
    };
    let maps = match cartridge.manifest().runtime.language {
        Language::Luau => match maps::load(&cartridge, overrides.maps) {
            Ok(maps) => maps,
            Err(error) => return Err(failed(cartridge, error)),
        },
    };
    Ok(Prepared {
        cartridge,
        source,
        hardware,
        maps,
    })
}

fn failed(cartridge: LoadedCartridge, error: StartError) -> Box<FailedStart> {
    Box::new(FailedStart { cartridge, error })
}

#[cfg(test)]
mod tests;
