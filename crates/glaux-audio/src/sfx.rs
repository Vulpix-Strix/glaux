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
mod tests {
    use super::*;
    use crate::{CHANNELS, ChannelWrite, EnvelopeWrite, Waveform};
    use pretty_assertions::assert_eq;

    fn step(tick: u32, action: SfxAction) -> SfxStep {
        SfxStep { tick, action }
    }

    fn gate(channel: usize, open: bool) -> SfxAction {
        SfxAction::Gate { channel, open }
    }

    fn tone(channel: usize, frequency: u16) -> SfxAction {
        SfxAction::Tone {
            channel,
            tone: Tone {
                waveform: Waveform::Square,
                frequency,
                volume: 8,
            },
        }
    }

    fn sfx(steps: &[SfxStep]) -> Sfx {
        Sfx::new(steps.to_vec()).unwrap()
    }

    fn gate_write(channel: usize, open: bool) -> ChannelWrite {
        ChannelWrite {
            channel,
            write: EnvelopeWrite::Gate(open),
        }
    }

    /// Ticks `player` `n` times, committing after each tick, and returns the
    /// envelope writes each tick made.
    fn writes_per_tick(
        player: &mut SfxPlayer,
        audio: &mut AudioState,
        n: usize,
    ) -> Vec<Vec<ChannelWrite>> {
        (0..n)
            .map(|_| {
                player.tick(audio).unwrap();
                let writes = audio.writes().to_vec();
                audio.commit();
                writes
            })
            .collect()
    }

    #[test]
    fn actions_apply_at_their_tick_offsets() {
        let mut player = SfxPlayer::new();
        player.play(sfx(&[
            step(0, gate(0, true)),
            step(2, gate(1, true)),
            step(5, gate(0, false)),
        ]));
        let mut audio = AudioState::new();
        let writes = writes_per_tick(&mut player, &mut audio, 6);
        assert_eq!(
            writes,
            [
                vec![gate_write(0, true)],
                vec![],
                vec![gate_write(1, true)],
                vec![],
                vec![],
                vec![gate_write(0, false)],
            ]
        );
    }

    #[test]
    fn actions_on_one_tick_apply_in_order() {
        let mut player = SfxPlayer::new();
        player.play(sfx(&[
            step(0, tone(0, 440)),
            step(0, gate(0, true)),
            step(0, gate(0, false)),
            step(0, tone(0, 880)),
        ]));
        let mut audio = AudioState::new();
        player.tick(&mut audio).unwrap();
        assert_eq!(audio.writes(), [gate_write(0, true), gate_write(0, false)]);
        assert_eq!(audio.channels()[0].tone.frequency, 880);
    }

    #[test]
    fn reopening_an_open_gate_is_kept_as_a_retrigger() {
        let mut player = SfxPlayer::new();
        player.play(sfx(&[step(0, gate(2, true)), step(3, gate(2, true))]));
        let mut audio = AudioState::new();
        let writes = writes_per_tick(&mut player, &mut audio, 4);
        assert_eq!(writes[0], [gate_write(2, true)]);
        assert_eq!(writes[3], [gate_write(2, true)]);
        assert!(audio.channels()[2].gate);
    }

    #[test]
    fn the_player_stops_after_its_last_step() {
        let mut player = SfxPlayer::new();
        player.play(sfx(&[step(0, gate(0, true)), step(2, gate(0, false))]));
        let mut audio = AudioState::new();
        for _ in 0..2 {
            player.tick(&mut audio).unwrap();
            assert!(player.is_active());
        }
        player.tick(&mut audio).unwrap();
        assert!(!player.is_active());

        audio.commit();
        let before = audio.clone();
        player.tick(&mut audio).unwrap();
        assert_eq!(audio, before);
    }

    #[test]
    fn an_empty_sound_ends_on_its_first_tick() {
        let mut player = SfxPlayer::new();
        player.play(Sfx::default());
        assert!(player.is_active());
        player.tick(&mut AudioState::new()).unwrap();
        assert!(!player.is_active());
    }

    #[test]
    fn playing_again_restarts_from_tick_zero() {
        let first = sfx(&[step(0, gate(0, true)), step(10, gate(0, false))]);
        let second = sfx(&[step(0, gate(1, true)), step(2, gate(1, false))]);
        let mut player = SfxPlayer::new();
        let mut audio = AudioState::new();
        player.play(first);
        writes_per_tick(&mut player, &mut audio, 4);

        player.play(second);
        let writes = writes_per_tick(&mut player, &mut audio, 3);
        assert_eq!(
            writes,
            [
                vec![gate_write(1, true)],
                vec![],
                vec![gate_write(1, false)]
            ]
        );
        assert!(!player.is_active());
        // The first sound's later steps never play.
        assert!(audio.channels()[0].gate);
    }

    #[test]
    fn identical_playbacks_produce_identical_audio_state() {
        let sound = sfx(&[
            step(
                0,
                SfxAction::Envelope {
                    channel: 3,
                    envelope: Envelope {
                        attack_ms: 5,
                        decay_ms: 40,
                        sustain: 6,
                        release_ms: 100,
                    },
                },
            ),
            step(0, tone(3, 300)),
            step(0, gate(3, true)),
            step(4, tone(3, 600)),
            step(4, gate(3, true)),
            step(7, gate(3, false)),
        ]);
        let run = || {
            let mut player = SfxPlayer::new();
            let mut audio = AudioState::new();
            player.play(sound.clone());
            (0..10)
                .map(|_| {
                    player.tick(&mut audio).unwrap();
                    let snapshot = audio.clone();
                    audio.commit();
                    snapshot
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn steps_must_not_go_back_in_time() {
        let steps = vec![
            step(0, gate(0, true)),
            step(3, gate(0, false)),
            step(3, gate(1, true)),
            step(2, gate(1, false)),
        ];
        assert_eq!(Sfx::new(steps), Err(SfxError::Unordered { index: 3 }));
    }

    #[test]
    fn a_rejected_write_stops_the_player() {
        let mut player = SfxPlayer::new();
        player.play(sfx(&[
            step(0, gate(0, true)),
            step(0, gate(CHANNELS, true)),
            step(1, gate(0, false)),
        ]));
        let mut audio = AudioState::new();
        assert_eq!(player.tick(&mut audio), Err(AudioError::Channel(CHANNELS)));
        assert!(!player.is_active());
        assert_eq!(audio.writes(), [gate_write(0, true)]);
    }
}
