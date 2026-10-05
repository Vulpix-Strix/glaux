//! A loaded cartridge together with the Luau VM running it.

use glaux_input::{ButtonLatch, InputEvent};
use glaux_luau::{Budget, DrawCommand, LogEntry, Runtime, RuntimeError, StartFailure};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{clear, draw_text, pixel};

use crate::cartridge::{CartridgeError, LoadedCartridge};

/// Why a cartridge could not start.
#[derive(Debug)]
pub enum StartError {
    /// The entry script could not be read.
    Cartridge(CartridgeError),
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
    /// Starts with every button released, so nothing held during a previous
    /// run reaches this one.
    buttons: ButtonLatch,
}

impl RunningCartridge {
    /// Creates a fresh VM for `cartridge` and runs its entry script and
    /// `boot()`. On failure the cartridge is handed back, still loaded.
    pub fn start(cartridge: LoadedCartridge) -> Result<Self, Box<FailedStart>> {
        let source = match cartridge.entry_source() {
            Ok(source) => source,
            Err(err) => return Err(failed(cartridge, StartError::Cartridge(err))),
        };
        let entry = &cartridge.manifest().entry;
        match Runtime::start(entry, &source, Budget::default()) {
            Ok(runtime) => Ok(Self {
                cartridge,
                runtime,
                buttons: ButtonLatch::new(),
            }),
            Err(failure) => Err(failed(cartridge, StartError::Script(failure))),
        }
    }

    pub fn cartridge(&self) -> &LoadedCartridge {
        &self.cartridge
    }

    /// Advances the cartridge by one logical tick, applying the button
    /// events in `input` received since the previous tick.
    pub fn tick(&mut self, input: &[InputEvent]) -> Result<(), RuntimeError> {
        let buttons = self.buttons.advance(input);
        self.runtime.tick(buttons)
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
                DrawCommand::Text {
                    ref text,
                    x,
                    y,
                    color,
                } => draw_text(framebuffer, font, text, x, y, color),
            }
        }
    }

    /// Disposes of the VM and gives back the cartridge.
    pub fn stop(self) -> LoadedCartridge {
        self.cartridge
    }
}

fn failed(cartridge: LoadedCartridge, error: StartError) -> Box<FailedStart> {
    Box::new(FailedStart { cartridge, error })
}
