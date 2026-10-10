//! A loaded cartridge together with the Luau VM running it.

use glaux_input::{InputEvent, InputLatch};
use glaux_luau::{Budget, DrawCommand, LogEntry, Runtime, RuntimeError, StartFailure};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::palette::Palette;
use glaux_machine::psg::Psg;
use glaux_machine::sprite::ObjTileBank;
use glaux_video::draw::{
    ObjPaletteChoice, ObjSprite, clear, draw_obj_sprite, draw_text, fill_rect, pixel,
};

use super::audio;
use super::video::{CartridgeVideo, CartridgeVideoDefaults, VideoAssetError, VideoLoadError};
use crate::cartridge::{CartridgeError, LoadedCartridge};

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

/// A cartridge whose VM is alive. Dropping it disposes of the VM.
#[derive(Debug)]
pub struct RunningCartridge {
    cartridge: LoadedCartridge,
    runtime: Runtime,
    video: CartridgeVideo,
    /// Starts with every button and key released, so nothing held during a
    /// previous run reaches this one, and is reset whenever the system
    /// resumes the cartridge from a pause.
    input: InputLatch,
}

impl RunningCartridge {
    /// Creates a fresh VM for `cartridge` and runs its entry script and
    /// `boot()`, with `defaults` standing in for video assets the cartridge
    /// does not supply. `tiles`, when given, replaces the object tiles read
    /// from the cartridge, so a run can show edits not written back. On
    /// failure the cartridge is handed back, still loaded.
    pub fn start(
        cartridge: LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
        tiles: Option<&ObjTileBank>,
    ) -> Result<Self, Box<FailedStart>> {
        let source = match cartridge.entry_source() {
            Ok(source) => source,
            Err(err) => return Err(failed(cartridge, StartError::Cartridge(err))),
        };
        let video = match CartridgeVideo::load(&cartridge, defaults) {
            Ok(mut video) => {
                if let Some(tiles) = tiles {
                    *video.obj.tiles_mut() = tiles.clone();
                }
                video
            }
            Err(VideoLoadError::Cartridge(err)) => {
                return Err(failed(cartridge, StartError::Cartridge(err)));
            }
            Err(VideoLoadError::Asset(err)) => {
                return Err(failed(cartridge, StartError::Video(err)));
            }
        };
        let entry = &cartridge.manifest().runtime.entry;
        match Runtime::start(entry, &source, cartridge.modules(), Budget::default()) {
            Ok(runtime) => Ok(Self {
                cartridge,
                runtime,
                video,
                input: InputLatch::new(),
            }),
            Err(failure) => Err(failed(cartridge, StartError::Script(failure))),
        }
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
