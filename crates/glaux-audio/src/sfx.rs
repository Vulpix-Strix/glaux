//! Sound effects: timed register and gate writes played into an
//! [`AudioState`].
//!
//! An [`Sfx`] is an ordered list of [`SfxStep`]s, each scheduled at a tick
//! offset from the start of the sound. An [`SfxPlayer`] walks one sound a
//! logical tick at a time and makes its writes through the same
//! [`AudioState`] calls a cartridge uses, so whoever applies that state to
//! the sound hardware needs no knowledge of sound effects.

use crate::{AudioError, AudioState, Envelope, Tone};

/// One write a sound effect makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfxAction {
    /// Sets a channel's oscillator registers, as [`AudioState::set_tone`].
    Tone { channel: usize, tone: Tone },
    /// Sets a channel's envelope, as [`AudioState::set_envelope`].
    Envelope { channel: usize, envelope: Envelope },
    /// Opens, retriggers or releases a channel's gate, as
    /// [`AudioState::set_gate`]. Opening an open gate retriggers it.
    Gate { channel: usize, open: bool },
}

impl SfxAction {
    fn apply(self, audio: &mut AudioState) -> Result<(), AudioError> {
        match self {
            Self::Tone { channel, tone } => audio.set_tone(channel, tone),
            Self::Envelope { channel, envelope } => audio.set_envelope(channel, envelope),
            Self::Gate { channel, open } => audio.set_gate(channel, open),
        }
    }
}

/// An action scheduled `tick` logical ticks after the sound starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SfxStep {
    pub tick: u32,
    pub action: SfxAction,
}

/// An invalid sound effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SfxError {
    #[error("step {index} is scheduled before the step preceding it")]
    Unordered { index: usize },
}

/// A sound effect: steps in the order they play.
///
/// Steps on the same tick play in list order. The default sound has no
/// steps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sfx {
    steps: Vec<SfxStep>,
}

impl Sfx {
    /// A sound playing `steps`, whose ticks must never decrease.
    pub fn new(steps: Vec<SfxStep>) -> Result<Self, SfxError> {
        if let Some(index) = steps
            .windows(2)
            .position(|pair| pair[1].tick < pair[0].tick)
        {
            return Err(SfxError::Unordered { index: index + 1 });
        }
        Ok(Self { steps })
    }

    pub fn steps(&self) -> &[SfxStep] {
        &self.steps
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Playback {
    sfx: Sfx,
    /// Ticks since the sound started.
    tick: u32,
    /// The first step not yet applied.
    next: usize,
}

/// Plays one [`Sfx`] at a time, a logical tick per [`tick`](Self::tick).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SfxPlayer {
    playing: Option<Playback>,
}

impl SfxPlayer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts `sfx` from tick 0, dropping whatever was playing. Writes
    /// already made are not undone.
    pub fn play(&mut self, sfx: Sfx) {
        self.playing = Some(Playback {
            sfx,
            tick: 0,
            next: 0,
        });
    }

    /// Drops the sound being played without making further writes.
    pub fn stop(&mut self) {
        self.playing = None;
    }

    /// Whether steps remain to be applied.
    ///
    /// This says nothing about whether the sound is still audible: once the
    /// final step closes a gate the player is inactive, while the envelope it
    /// released may still be fading out.
    pub fn is_active(&self) -> bool {
        self.playing.is_some()
    }

    /// Applies, in order, every step scheduled for the current tick, then
    /// advances one tick. The player becomes inactive once its last step
    /// has been applied. Does nothing while inactive.
    ///
    /// A write that `audio` rejects stops the player and is returned; steps
    /// before it on this tick have already been applied.
    pub fn tick(&mut self, audio: &mut AudioState) -> Result<(), AudioError> {
        let Some(playback) = &mut self.playing else {
            return Ok(());
        };
        let steps = playback.sfx.steps();
        while let Some(step) = steps.get(playback.next)
            && step.tick == playback.tick
        {
            if let Err(err) = step.action.apply(audio) {
                self.playing = None;
                return Err(err);
            }
            playback.next += 1;
        }
        if playback.next >= steps.len() {
            self.playing = None;
        } else {
            playback.tick += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
