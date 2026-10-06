//! Runtime-neutral cartridge audio for the G1 sound generator.
//!
//! Cartridge runtimes write into an [`AudioState`] in cartridge units: Hz,
//! volume and sustain steps, and envelope times in milliseconds. Whoever owns
//! the sound hardware reads it back and applies it; this crate knows nothing
//! about synthesis.
//!
//! Each channel has two kinds of state:
//!
//! - oscillator registers ([`Tone`]), where only the latest write matters;
//! - an envelope, driven by [`Envelope`] and gate writes. A gate opening
//!   retriggers even while the gate is already open, and envelope and gate
//!   writes interact, so these are kept as ordered [`ChannelWrite`]s until
//!   the next [`AudioState::commit`].

use std::fmt;

/// Number of sound channels.
pub const CHANNELS: usize = 4;

/// Highest channel frequency, in Hz. A frequency of 0 silences the channel.
pub const MAX_FREQUENCY: u16 = 24_000;

/// Loudest channel volume. Volume 0 is silent.
pub const MAX_VOLUME: u8 = 15;

/// Highest sustain level: sustain at this value holds the envelope at full
/// level.
pub const MAX_SUSTAIN: u8 = 15;

/// Longest attack, decay or release, in milliseconds.
pub const MAX_ENVELOPE_MS: u32 = 10_000;

/// Envelope and gate writes that may be pending at once, across all
/// channels. Bounds the memory a script can make the runtime hold between
/// commits.
pub const MAX_ENVELOPE_WRITES: usize = 256;

/// The shape a channel plays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Waveform {
    #[default]
    Square,
    Triangle,
    Saw,
    Noise,
}

impl Waveform {
    pub const ALL: [Self; 4] = [Self::Square, Self::Triangle, Self::Saw, Self::Noise];

    /// The lowercase name scripts use for this waveform.
    pub fn name(self) -> &'static str {
        match self {
            Self::Square => "square",
            Self::Triangle => "triangle",
            Self::Saw => "saw",
            Self::Noise => "noise",
        }
    }

    /// The waveform called `name`, matched exactly.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|waveform| waveform.name() == name)
    }
}

impl fmt::Display for Waveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A channel's oscillator registers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tone {
    pub waveform: Waveform,
    /// Hz, up to [`MAX_FREQUENCY`].
    pub frequency: u16,
    /// Up to [`MAX_VOLUME`].
    pub volume: u8,
}

/// A channel's envelope parameters.
///
/// Each time is how long its ramp takes across the full range, as in the
/// sound generator's linear envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    /// Up to [`MAX_ENVELOPE_MS`].
    pub attack_ms: u32,
    /// Up to [`MAX_ENVELOPE_MS`].
    pub decay_ms: u32,
    /// Up to [`MAX_SUSTAIN`].
    pub sustain: u8,
    /// Up to [`MAX_ENVELOPE_MS`].
    pub release_ms: u32,
}

impl Default for Envelope {
    /// An instant envelope: full level as soon as the gate opens and silence
    /// as soon as it closes.
    fn default() -> Self {
        Self {
            attack_ms: 0,
            decay_ms: 0,
            sustain: MAX_SUSTAIN,
            release_ms: 0,
        }
    }
}

/// Everything a cartridge has set on one channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChannelState {
    pub tone: Tone,
    pub envelope: Envelope,
    /// Whether the cartridge last opened or closed the gate.
    pub gate: bool,
}

/// A write that affects a channel's envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeWrite {
    Envelope(Envelope),
    /// `true` opens or retriggers the gate; `false` releases it.
    Gate(bool),
}

/// An [`EnvelopeWrite`] to one channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelWrite {
    /// Below [`CHANNELS`].
    pub channel: usize,
    pub write: EnvelopeWrite,
}

/// An invalid audio write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AudioError {
    #[error("channel 0-{max} expected, got {0}", max = CHANNELS - 1)]
    Channel(usize),
    #[error("frequency 0-{MAX_FREQUENCY} expected, got {0}")]
    Frequency(u16),
    #[error("volume 0-{MAX_VOLUME} expected, got {0}")]
    Volume(u8),
    #[error("sustain 0-{MAX_SUSTAIN} expected, got {0}")]
    Sustain(u8),
    #[error("envelope time 0-{MAX_ENVELOPE_MS} ms expected, got {0}")]
    Time(u32),
    #[error("too many envelope and gate writes in one tick (limit {MAX_ENVELOPE_WRITES})")]
    TooManyWrites,
}

/// A cartridge's audio: the registers of every channel, and the envelope
/// writes made since the last [`commit`](Self::commit).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioState {
    channels: [ChannelState; CHANNELS],
    baseline: [ChannelState; CHANNELS],
    writes: Vec<ChannelWrite>,
}

impl AudioState {
    /// Every channel silent, with an instant envelope and its gate closed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a channel's oscillator registers. It records no envelope write,
    /// so it never retriggers or releases the envelope.
    pub fn set_tone(&mut self, channel: usize, tone: Tone) -> Result<(), AudioError> {
        check_channel(channel)?;
        if tone.frequency > MAX_FREQUENCY {
            return Err(AudioError::Frequency(tone.frequency));
        }
        if tone.volume > MAX_VOLUME {
            return Err(AudioError::Volume(tone.volume));
        }
        self.channels[channel].tone = tone;
        Ok(())
    }

    /// Sets a channel's envelope parameters, recording the write.
    pub fn set_envelope(&mut self, channel: usize, envelope: Envelope) -> Result<(), AudioError> {
        check_channel(channel)?;
        for time in [envelope.attack_ms, envelope.decay_ms, envelope.release_ms] {
            if time > MAX_ENVELOPE_MS {
                return Err(AudioError::Time(time));
            }
        }
        if envelope.sustain > MAX_SUSTAIN {
            return Err(AudioError::Sustain(envelope.sustain));
        }
        self.record(channel, EnvelopeWrite::Envelope(envelope))?;
        self.channels[channel].envelope = envelope;
        Ok(())
    }

    /// Opens, retriggers or releases a channel's gate, recording the write.
    pub fn set_gate(&mut self, channel: usize, open: bool) -> Result<(), AudioError> {
        check_channel(channel)?;
        self.record(channel, EnvelopeWrite::Gate(open))?;
        self.channels[channel].gate = open;
        Ok(())
    }

    /// Every channel's registers, including writes not yet committed.
    pub fn channels(&self) -> &[ChannelState; CHANNELS] {
        &self.channels
    }

    /// Every channel's registers as of the last commit, before
    /// [`writes`](Self::writes).
    pub fn baseline(&self) -> &[ChannelState; CHANNELS] {
        &self.baseline
    }

    /// Envelope writes since the last commit, in the order they were made.
    pub fn writes(&self) -> &[ChannelWrite] {
        &self.writes
    }

    /// Marks every write as applied: the current registers become the
    /// baseline and the pending writes are cleared.
    pub fn commit(&mut self) {
        self.baseline = self.channels;
        self.writes.clear();
    }

    /// Appends a pending write.
    ///
    /// A write equal to the last pending one is dropped. Pending writes are
    /// all applied at the same instant, so nothing can play between the two,
    /// and applying the same envelope or gate write twice in a row at one
    /// instant has the effect of applying it once. Nothing else is merged:
    /// any write in between, to any channel, keeps both.
    fn record(&mut self, channel: usize, write: EnvelopeWrite) -> Result<(), AudioError> {
        let write = ChannelWrite { channel, write };
        if self.writes.last() == Some(&write) {
            return Ok(());
        }
        if self.writes.len() >= MAX_ENVELOPE_WRITES {
            return Err(AudioError::TooManyWrites);
        }
        self.writes.push(write);
        Ok(())
    }
}

fn check_channel(channel: usize) -> Result<(), AudioError> {
    if channel < CHANNELS {
        Ok(())
    } else {
        Err(AudioError::Channel(channel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn gate(channel: usize, open: bool) -> ChannelWrite {
        ChannelWrite {
            channel,
            write: EnvelopeWrite::Gate(open),
        }
    }

    fn envelope(attack_ms: u32, decay_ms: u32, sustain: u8, release_ms: u32) -> Envelope {
        Envelope {
            attack_ms,
            decay_ms,
            sustain,
            release_ms,
        }
    }

    #[test]
    fn starts_silent_with_instant_envelopes_and_closed_gates() {
        let state = AudioState::new();
        let silent = ChannelState {
            tone: Tone {
                waveform: Waveform::Square,
                frequency: 0,
                volume: 0,
            },
            envelope: envelope(0, 0, MAX_SUSTAIN, 0),
            gate: false,
        };
        assert_eq!(state.channels(), &[silent; CHANNELS]);
        assert_eq!(state.baseline(), &[silent; CHANNELS]);
        assert!(state.writes().is_empty());
    }

    #[test]
    fn waveform_names_round_trip() {
        for waveform in Waveform::ALL {
            assert_eq!(Waveform::from_name(waveform.name()), Some(waveform));
        }
        assert_eq!(
            Waveform::ALL.map(Waveform::name),
            ["square", "triangle", "saw", "noise"]
        );
        for name in ["", "Square", "SAW", "sine", "noise "] {
            assert_eq!(Waveform::from_name(name), None, "{name:?}");
        }
    }

    #[test]
    fn writes_accept_the_range_limits() {
        let mut state = AudioState::new();
        let loudest = Tone {
            waveform: Waveform::Noise,
            frequency: MAX_FREQUENCY,
            volume: MAX_VOLUME,
        };
        let slowest = envelope(MAX_ENVELOPE_MS, MAX_ENVELOPE_MS, 0, MAX_ENVELOPE_MS);
        state.set_tone(CHANNELS - 1, loudest).unwrap();
        state.set_envelope(CHANNELS - 1, slowest).unwrap();
        state.set_gate(CHANNELS - 1, true).unwrap();
        let channel = state.channels()[CHANNELS - 1];
        assert_eq!(channel.tone, loudest);
        assert_eq!(channel.envelope, slowest);
        assert!(channel.gate);
    }

    #[test]
    fn writes_reject_values_past_the_limits_and_change_nothing() {
        let mut state = AudioState::new();
        let tone = Tone::default();
        let too_long = MAX_ENVELOPE_MS + 1;
        let errors = [
            state.set_tone(CHANNELS, tone),
            state.set_envelope(CHANNELS, Envelope::default()),
            state.set_gate(CHANNELS, true),
            state.set_tone(
                0,
                Tone {
                    frequency: MAX_FREQUENCY + 1,
                    ..tone
                },
            ),
            state.set_tone(
                0,
                Tone {
                    volume: MAX_VOLUME + 1,
                    ..tone
                },
            ),
            state.set_envelope(0, envelope(too_long, 0, 0, 0)),
            state.set_envelope(0, envelope(0, too_long, 0, 0)),
            state.set_envelope(0, envelope(0, 0, 0, too_long)),
            state.set_envelope(0, envelope(0, 0, MAX_SUSTAIN + 1, 0)),
        ];
        assert_eq!(
            errors,
            [
                Err(AudioError::Channel(CHANNELS)),
                Err(AudioError::Channel(CHANNELS)),
                Err(AudioError::Channel(CHANNELS)),
                Err(AudioError::Frequency(MAX_FREQUENCY + 1)),
                Err(AudioError::Volume(MAX_VOLUME + 1)),
                Err(AudioError::Time(too_long)),
                Err(AudioError::Time(too_long)),
                Err(AudioError::Time(too_long)),
                Err(AudioError::Sustain(MAX_SUSTAIN + 1)),
            ]
        );
        assert_eq!(state, AudioState::new());
    }

    #[test]
    fn errors_name_the_range() {
        assert_eq!(
            AudioError::Frequency(30_000).to_string(),
            "frequency 0-24000 expected, got 30000"
        );
        assert_eq!(
            AudioError::Channel(4).to_string(),
            "channel 0-3 expected, got 4"
        );
    }

    #[test]
    fn tone_writes_record_no_envelope_write() {
        let mut state = AudioState::new();
        state.set_gate(0, true).unwrap();
        state
            .set_tone(
                0,
                Tone {
                    waveform: Waveform::Saw,
                    frequency: 440,
                    volume: 8,
                },
            )
            .unwrap();
        assert_eq!(state.writes(), [gate(0, true)]);
    }

    #[test]
    fn envelope_writes_are_kept_in_order() {
        let mut state = AudioState::new();
        let shape = envelope(10, 20, 5, 30);
        state.set_gate(1, true).unwrap();
        state.set_envelope(1, shape).unwrap();
        state.set_gate(1, false).unwrap();
        assert_eq!(
            state.writes(),
            [
                gate(1, true),
                ChannelWrite {
                    channel: 1,
                    write: EnvelopeWrite::Envelope(shape),
                },
                gate(1, false),
            ]
        );
        assert_eq!(state.channels()[1].envelope, shape);
        assert!(!state.channels()[1].gate);
    }

    #[test]
    fn only_an_immediate_repeat_is_compacted() {
        let mut state = AudioState::new();
        for open in [true, true, false, true] {
            state.set_gate(0, open).unwrap();
        }
        assert_eq!(
            state.writes(),
            [gate(0, true), gate(0, false), gate(0, true)]
        );

        let mut state = AudioState::new();
        for channel in [0, 1, 0] {
            state.set_gate(channel, true).unwrap();
        }
        assert_eq!(
            state.writes(),
            [gate(0, true), gate(1, true), gate(0, true)]
        );

        let mut state = AudioState::new();
        let shape = envelope(1, 2, 3, 4);
        state.set_envelope(2, shape).unwrap();
        state.set_envelope(2, shape).unwrap();
        state.set_envelope(2, envelope(1, 2, 3, 5)).unwrap();
        assert_eq!(state.writes().len(), 2);
    }

    #[test]
    fn compaction_does_not_reach_across_a_commit() {
        let mut state = AudioState::new();
        state.set_gate(0, true).unwrap();
        state.commit();
        state.set_gate(0, true).unwrap();
        assert_eq!(state.writes(), [gate(0, true)]);
    }

    #[test]
    fn pending_writes_are_capped() {
        let mut state = AudioState::new();
        let open = |index: usize| index.is_multiple_of(2);
        for index in 0..MAX_ENVELOPE_WRITES {
            state.set_gate(0, open(index)).unwrap();
        }
        let last = open(MAX_ENVELOPE_WRITES - 1);
        let before = state.clone();
        assert_eq!(state.set_gate(0, !last), Err(AudioError::TooManyWrites));
        assert_eq!(state, before);
        // A repeat of the last write records nothing, so it still fits.
        assert_eq!(state.set_gate(0, last), Ok(()));
        // Committing frees the budget.
        state.commit();
        assert_eq!(state.set_gate(0, true), Ok(()));
    }

    #[test]
    fn commit_moves_the_registers_into_the_baseline() {
        let mut state = AudioState::new();
        let tone = Tone {
            waveform: Waveform::Triangle,
            frequency: 220,
            volume: 9,
        };
        state.set_tone(3, tone).unwrap();
        state.set_gate(3, true).unwrap();
        assert_eq!(state.baseline(), &[ChannelState::default(); CHANNELS]);

        state.commit();
        assert_eq!(state.baseline(), state.channels());
        assert_eq!(state.baseline()[3].tone, tone);
        assert!(state.baseline()[3].gate);
        assert!(state.writes().is_empty());
    }
}
