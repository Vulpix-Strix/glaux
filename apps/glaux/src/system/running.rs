//! A loaded cartridge together with the script runtime running it.

mod debug;

use std::cell::RefMut;

use glaux_audio::AudioState;
use glaux_input::{InputEvent, InputLatch, InputState};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::hardware::CartridgeHardware;
use glaux_machine::palette::Palette;
use glaux_machine::psg::Psg;
use glaux_script::debug::{Breakpoint, DebugReply, DebugRequest, Resume, Stop};
use glaux_script::{DrawCommand, LogEntry, Phase, RuntimeError, StartFailure};
use glaux_video::draw::{
    ObjPaletteChoice, ObjSprite, clear, draw_obj_sprite, draw_text, fill_rect, pixel,
};

use self::debug::DebugRuntime;
pub use self::debug::Tick;
use super::audio;
use super::video::{
    CartridgeVideo, CartridgeVideoDefaults, VideoAssetError, VideoLoadError, VideoOverrides,
};
use crate::cartridge::{CartridgeError, CartridgeModules, Language, LoadedCartridge};

/// Why a cartridge could not start.
#[derive(Debug)]
pub enum StartError {
    /// The entry script could not be read.
    Cartridge(CartridgeError),
    /// A video asset was not valid native data.
    Video(VideoAssetError),
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
    // Boxed: Rhai's engine is several times the size of a Luau handle.
    Rhai(Box<glaux_rhai::Runtime>),
    /// Either of them on a thread of its own, under the debugger.
    Debugged(Box<DebugRuntime>),
}

impl ScriptRuntime {
    /// Starts `source` with the default budget of `language`'s runtime.
    fn start(
        language: Language,
        entry: &str,
        source: &[u8],
        modules: CartridgeModules,
    ) -> Result<Self, StartFailure> {
        match language {
            Language::Luau => {
                glaux_luau::Runtime::start(entry, source, modules, glaux_luau::Budget::default())
                    .map(Self::Luau)
            }
            Language::Rhai => {
                glaux_rhai::Runtime::start(entry, source, modules, glaux_rhai::Budget::default())
                    .map(|runtime| Self::Rhai(Box::new(runtime)))
            }
        }
    }

    /// Starts `source` under the debugger with the default budget, to be
    /// driven by whoever holds the other end of `port`. Called on the thread
    /// the runtime will live on.
    fn start_debug(
        language: Language,
        entry: &str,
        source: &[u8],
        modules: CartridgeModules,
        port: glaux_script::debug::DebugPort,
        breakpoints: Vec<Breakpoint>,
    ) -> Result<Self, StartFailure> {
        match language {
            Language::Luau => glaux_luau::Runtime::start_debug(
                entry,
                source,
                modules,
                glaux_luau::Budget::default(),
                port,
                breakpoints,
            )
            .map(Self::Luau),
            Language::Rhai => glaux_rhai::Runtime::start_debug(
                entry,
                source,
                modules,
                glaux_rhai::Budget::default(),
                port,
                breakpoints,
            )
            .map(|runtime| Self::Rhai(Box::new(runtime))),
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
            Self::Rhai(runtime) => runtime.run_tick(input, home),
            Self::Debugged(_) => {
                unreachable!("a debugged runtime runs its phases through `advance`")
            }
        }
    }

    /// Runs the `draw()` phase, as [`run_tick`](Self::run_tick) does.
    fn run_draw(&mut self, home: &mut Option<Box<CartridgeHardware>>) -> Result<(), RuntimeError> {
        match self {
            Self::Luau(runtime) => runtime.run_draw(home),
            Self::Rhai(runtime) => runtime.run_draw(home),
            Self::Debugged(_) => {
                unreachable!("a debugged runtime runs its phases through `advance`")
            }
        }
    }

    fn frame(&self) -> &[DrawCommand] {
        match self {
            Self::Luau(runtime) => runtime.frame(),
            Self::Rhai(runtime) => runtime.frame(),
            Self::Debugged(runtime) => runtime.frame(),
        }
    }

    fn take_output(&mut self) -> Vec<LogEntry> {
        match self {
            Self::Luau(runtime) => runtime.take_output(),
            Self::Rhai(runtime) => runtime.take_output(),
            Self::Debugged(runtime) => runtime.take_output(),
        }
    }

    fn take_dropped(&mut self) -> u32 {
        match self {
            Self::Luau(runtime) => runtime.take_dropped(),
            Self::Rhai(runtime) => runtime.take_dropped(),
            Self::Debugged(runtime) => runtime.take_dropped(),
        }
    }

    fn audio_mut(&mut self) -> RefMut<'_, AudioState> {
        match self {
            Self::Luau(runtime) => runtime.audio_mut(),
            Self::Rhai(runtime) => runtime.audio_mut(),
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
    /// cartridge's files hold, so a run can show edits not written back. On
    /// failure the cartridge is handed back, still loaded.
    pub fn start(
        cartridge: LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
        overrides: VideoOverrides<'_>,
    ) -> Result<Self, Box<FailedStart>> {
        let (cartridge, source, hardware) = prepare(cartridge, defaults, overrides)?;
        let spec = &cartridge.manifest().runtime;
        match ScriptRuntime::start(spec.language, &spec.entry, &source, cartridge.modules()) {
            Ok(runtime) => Ok(Self::new(cartridge, runtime, hardware)),
            Err(failure) => Err(failed(cartridge, StartError::Script(failure))),
        }
    }

    /// Like [`start`](Self::start), for a run under the debugger that stops
    /// at `breakpoints`. The runtime lives on a thread of its own. The start
    /// itself can stop, at a breakpoint in the entry script or in `boot()`,
    /// and then the stop comes with the cartridge and the run goes on with
    /// [`resume`](Self::resume).
    pub fn start_debug(
        cartridge: LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
        overrides: VideoOverrides<'_>,
        breakpoints: Vec<Breakpoint>,
    ) -> Result<(Self, Option<Stop>), Box<FailedStart>> {
        let (cartridge, source, hardware) = prepare(cartridge, defaults, overrides)?;
        let spec = &cartridge.manifest().runtime;
        match DebugRuntime::start(
            spec.language,
            &spec.entry,
            source,
            cartridge.modules(),
            breakpoints,
        ) {
            Ok((runtime, stop)) => {
                let runtime = ScriptRuntime::Debugged(Box::new(runtime));
                let mut running = Self::new(cartridge, runtime, hardware);
                running.stopped_in = stop.as_ref().map(|_| Phase::Entry);
                Ok((running, stop))
            }
            Err(failure) => Err(failed(cartridge, StartError::Script(failure))),
        }
    }

    fn new(
        cartridge: LoadedCartridge,
        runtime: ScriptRuntime,
        hardware: Box<CartridgeHardware>,
    ) -> Self {
        Self {
            cartridge,
            runtime,
            hardware: Some(hardware),
            stopped_in: None,
            input: InputLatch::new(),
            origin: RunOrigin::default(),
        }
    }

    fn debugged(&mut self) -> Option<&mut DebugRuntime> {
        match &mut self.runtime {
            ScriptRuntime::Debugged(runtime) => Some(runtime),
            ScriptRuntime::Luau(_) | ScriptRuntime::Rhai(_) => None,
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
    #[hotpath::measure(label = "running::advance", impl_type = "RunningCartridge")]
    pub fn advance(&mut self, input: &[InputEvent]) -> Result<Tick, RuntimeError> {
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

    fn run_draw(&mut self) -> Result<Tick, RuntimeError> {
        let tick = match &mut self.runtime {
            ScriptRuntime::Debugged(runtime) => runtime.run_draw(&mut self.hardware)?,
            runtime => {
                runtime.run_draw(&mut self.hardware)?;
                Tick::Done
            }
        };
        Ok(self.note_stop(Phase::Draw, tick))
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

    /// The hardware, for whoever draws or inspects the run.
    ///
    /// It is present whenever the run is running or paused, which are the
    /// only states that read it: a phase hands it back before it returns,
    /// and a run that fails is stopped by the caller before anything else
    /// reads it. While a debugged run is stopped in a phase the runtime
    /// holds it, and the system does not read it.
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

    /// The master palette the cartridge's frames resolve to.
    pub fn palette(&self) -> &Palette {
        &self.hardware().palette
    }

    /// Advances the cartridge by one logical tick, applying the button and
    /// key events in `input` received since the previous tick.
    #[cfg(test)]
    pub fn tick(&mut self, input: &[InputEvent]) -> Result<(), RuntimeError> {
        let state = self.input.advance(input);
        self.runtime.run_tick(state, &mut self.hardware)?;
        self.runtime.run_draw(&mut self.hardware)
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

    /// Redraws the cartridge's last completed frame: a screen cleared to
    /// palette index 0, then its draw commands in the order they were made.
    #[hotpath::measure(label = "running::render", impl_type = "RunningCartridge")]
    pub fn render(&self, framebuffer: &mut Framebuffer, font: &BitmapFont) {
        clear(framebuffer, 0);
        for command in self.runtime.frame() {
            match *command {
                DrawCommand::Clear(color) => clear(framebuffer, color),
                DrawCommand::Pixel { x, y, color } => pixel(framebuffer, x, y, color),
                DrawCommand::Rect {
                    x,
                    y,
                    width,
                    height,
                    color,
                } => fill_rect(framebuffer, x, y, width, height, color),
                DrawCommand::Sprite {
                    tile,
                    x,
                    y,
                    width,
                    height,
                    flip_x,
                    flip_y,
                    palette,
                } => draw_obj_sprite(
                    framebuffer,
                    &self.hardware().obj,
                    &ObjSprite {
                        first: usize::from(tile),
                        x,
                        y,
                        width: usize::from(width),
                        height: usize::from(height),
                        flip_x,
                        flip_y,
                        palette: palette
                            .map_or(ObjPaletteChoice::TileDefault, ObjPaletteChoice::Bank),
                    },
                ),
                DrawCommand::Text {
                    ref text,
                    x,
                    y,
                    color,
                } => draw_text(framebuffer, font, text, x, y, color),
            }
        }
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

/// Reads the entry script and the video assets a run needs.
fn prepare(
    cartridge: LoadedCartridge,
    defaults: &CartridgeVideoDefaults,
    overrides: VideoOverrides<'_>,
) -> Result<(LoadedCartridge, Vec<u8>, Box<CartridgeHardware>), Box<FailedStart>> {
    let source = match cartridge.entry_source() {
        Ok(source) => source,
        Err(err) => return Err(failed(cartridge, StartError::Cartridge(err))),
    };
    let hardware = match CartridgeVideo::load(&cartridge, defaults) {
        Ok(mut video) => {
            overrides.apply(&mut video);
            Box::new(video.into_hardware())
        }
        Err(VideoLoadError::Cartridge(err)) => {
            return Err(failed(cartridge, StartError::Cartridge(err)));
        }
        Err(VideoLoadError::Asset(err)) => {
            return Err(failed(cartridge, StartError::Video(err)));
        }
    };
    Ok((cartridge, source, hardware))
}

fn failed(cartridge: LoadedCartridge, error: StartError) -> Box<FailedStart> {
    Box::new(FailedStart { cartridge, error })
}

#[cfg(test)]
mod tests;
