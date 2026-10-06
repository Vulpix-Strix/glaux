//! A loaded cartridge together with the Luau VM running it.

use glaux_audio::{ChannelState, Envelope, EnvelopeWrite};
use glaux_input::{ButtonLatch, InputEvent};
use glaux_luau::{Budget, DrawCommand, LogEntry, Runtime, RuntimeError, StartFailure};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::psg::{self, Adsr, Psg, SAMPLES_PER_MS};
use glaux_machine::sprite::{ObjTileBank, ObjTileBankError};
use glaux_video::draw::{clear, draw_obj_sprite, draw_text, pixel};

use crate::cartridge::{CartridgeError, LoadedCartridge};

/// Why a cartridge could not start.
#[derive(Debug)]
pub enum StartError {
    /// The entry script could not be read.
    Cartridge(CartridgeError),
    /// `sprites.bin` was not valid native object-tile data.
    ObjTiles(ObjTileBankError),
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
    obj_tiles: ObjTileBank,
    /// Starts with every button released, so nothing held during a previous
    /// run reaches this one, and is reset whenever the system resumes the
    /// cartridge from a pause.
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
        let obj_bytes = match cartridge.read_obj_tiles() {
            Ok(bytes) => bytes,
            Err(err) => return Err(failed(cartridge, StartError::Cartridge(err))),
        };
        let obj_tiles = match obj_bytes {
            Some(bytes) => match ObjTileBank::from_bytes(&bytes) {
                Ok(tiles) => tiles,
                Err(err) => return Err(failed(cartridge, StartError::ObjTiles(err))),
            },
            None => ObjTileBank::new(),
        };
        let entry = &cartridge.manifest().entry;
        match Runtime::start(entry, &source, Budget::default()) {
            Ok(runtime) => Ok(Self {
                cartridge,
                runtime,
                obj_tiles,
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

    /// Releases every game button as seen by the cartridge, so its next tick
    /// sees no held buttons and no presses from before this call.
    pub fn reset_input(&mut self) {
        self.buttons = ButtonLatch::new();
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
                DrawCommand::Sprite {
                    tile,
                    x,
                    y,
                    width,
                    height,
                } => draw_obj_sprite(
                    framebuffer,
                    &self.obj_tiles,
                    usize::from(tile),
                    x,
                    y,
                    usize::from(width),
                    usize::from(height),
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

    /// Writes the cartridge's audio into `psg` and commits it.
    ///
    /// `fresh` means `psg` was just reset for this cartridge: each channel is
    /// first rebuilt from the registers as of the last commit, so a channel
    /// whose gate the cartridge left open starts a new attack. Then every
    /// channel takes its current oscillator registers, and the envelope and
    /// gate writes since the last commit are replayed in order. Each gate
    /// opening reaches the PSG, so an open gate is retriggered.
    pub fn apply_audio(&mut self, psg: &mut Psg, fresh: bool) {
        let mut audio = self.runtime.audio_mut();
        if fresh {
            for (index, state) in audio.baseline().iter().enumerate() {
                let channel = psg_channel(psg, index);
                channel.set_adsr(adsr(state.envelope));
                if state.gate {
                    channel.gate_on();
                }
            }
        }
        for (index, &ChannelState { tone, .. }) in audio.channels().iter().enumerate() {
            let channel = psg_channel(psg, index);
            channel.set_waveform(waveform(tone.waveform));
            channel.set_frequency(tone.frequency);
            channel.set_volume(tone.volume);
        }
        for write in audio.writes() {
            let channel = psg_channel(psg, write.channel);
            match write.write {
                EnvelopeWrite::Envelope(envelope) => channel.set_adsr(adsr(envelope)),
                EnvelopeWrite::Gate(true) => channel.gate_on(),
                EnvelopeWrite::Gate(false) => channel.gate_off(),
            }
        }
        audio.commit();
    }

    /// Disposes of the VM and gives back the cartridge.
    pub fn stop(self) -> LoadedCartridge {
        self.cartridge
    }
}

fn failed(cartridge: LoadedCartridge, error: StartError) -> Box<FailedStart> {
    Box::new(FailedStart { cartridge, error })
}

fn psg_channel(psg: &mut Psg, index: usize) -> &mut psg::Channel {
    let Some(channel) = psg.channel_mut(index) else {
        unreachable!("cartridge audio and the PSG have the same channels");
    };
    channel
}

fn waveform(waveform: glaux_audio::Waveform) -> psg::Waveform {
    match waveform {
        glaux_audio::Waveform::Square => psg::Waveform::Square,
        glaux_audio::Waveform::Triangle => psg::Waveform::Triangle,
        glaux_audio::Waveform::Saw => psg::Waveform::Saw,
        glaux_audio::Waveform::Noise => psg::Waveform::Noise,
    }
}

/// Envelope times converted from milliseconds to PSG samples, exactly.
fn adsr(envelope: Envelope) -> Adsr {
    Adsr {
        attack: envelope.attack_ms * SAMPLES_PER_MS,
        decay: envelope.decay_ms * SAMPLES_PER_MS,
        sustain: envelope.sustain,
        release: envelope.release_ms * SAMPLES_PER_MS,
    }
}
