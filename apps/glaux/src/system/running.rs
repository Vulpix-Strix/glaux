//! A loaded cartridge together with the script runtime running it.

use std::cell::RefMut;

use glaux_audio::AudioState;
use glaux_input::{InputEvent, InputLatch, InputState};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::palette::Palette;
use glaux_machine::psg::Psg;
use glaux_script::{DrawCommand, LogEntry, RuntimeError, StartFailure};
use glaux_video::draw::{
    ObjPaletteChoice, ObjSprite, clear, draw_obj_sprite, draw_text, fill_rect, pixel,
};

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

    fn tick(&mut self, input: InputState) -> Result<(), RuntimeError> {
        match self {
            Self::Luau(runtime) => runtime.tick(input),
            Self::Rhai(runtime) => runtime.tick(input),
        }
    }

    fn frame(&self) -> &[DrawCommand] {
        match self {
            Self::Luau(runtime) => runtime.frame(),
            Self::Rhai(runtime) => runtime.frame(),
        }
    }

    fn take_output(&mut self) -> Vec<LogEntry> {
        match self {
            Self::Luau(runtime) => runtime.take_output(),
            Self::Rhai(runtime) => runtime.take_output(),
        }
    }

    fn take_dropped(&mut self) -> u32 {
        match self {
            Self::Luau(runtime) => runtime.take_dropped(),
            Self::Rhai(runtime) => runtime.take_dropped(),
        }
    }

    fn audio_mut(&mut self) -> RefMut<'_, AudioState> {
        match self {
            Self::Luau(runtime) => runtime.audio_mut(),
            Self::Rhai(runtime) => runtime.audio_mut(),
        }
    }
}

/// A cartridge whose VM is alive. Dropping it disposes of the VM.
#[derive(Debug)]
pub struct RunningCartridge {
    cartridge: LoadedCartridge,
    runtime: ScriptRuntime,
    video: CartridgeVideo,
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
        let source = match cartridge.entry_source() {
            Ok(source) => source,
            Err(err) => return Err(failed(cartridge, StartError::Cartridge(err))),
        };
        let video = match CartridgeVideo::load(&cartridge, defaults) {
            Ok(mut video) => {
                overrides.apply(&mut video);
                video
            }
            Err(VideoLoadError::Cartridge(err)) => {
                return Err(failed(cartridge, StartError::Cartridge(err)));
            }
            Err(VideoLoadError::Asset(err)) => {
                return Err(failed(cartridge, StartError::Video(err)));
            }
        };
        let spec = &cartridge.manifest().runtime;
        match ScriptRuntime::start(spec.language, &spec.entry, &source, cartridge.modules()) {
            Ok(runtime) => Ok(Self {
                cartridge,
                runtime,
                video,
                input: InputLatch::new(),
                origin: RunOrigin::default(),
            }),
            Err(failure) => Err(failed(cartridge, StartError::Script(failure))),
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

    /// The master palette the cartridge's frames resolve to.
    pub fn palette(&self) -> &Palette {
        &self.video.palette
    }

    /// Advances the cartridge by one logical tick, applying the button and
    /// key events in `input` received since the previous tick.
    pub fn tick(&mut self, input: &[InputEvent]) -> Result<(), RuntimeError> {
        let state = self.input.advance(input);
        self.runtime.tick(state)
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
                    &self.video.obj,
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

fn failed(cartridge: LoadedCartridge, error: StartError) -> Box<FailedStart> {
    Box::new(FailedStart { cartridge, error })
}

#[cfg(test)]
mod tests;
