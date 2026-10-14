//! Programmable sound generator.
//!
//! The PSG turns its channel state into mono signed 16-bit samples at
//! [`SAMPLE_RATE`]. Synthesis uses integer math only, so the same sequence of
//! register writes and render calls always yields the same samples.
//!
//! Each channel's output is its waveform scaled by its volume and by its
//! envelope level. Each sample is produced at the current envelope level, and
//! the envelope advances afterwards.

use crate::machine::TICK_RATE;

/// Output samples per second.
pub const SAMPLE_RATE: u32 = 48_000;

/// Samples the PSG produces for one machine tick.
pub const SAMPLES_PER_TICK: usize = (SAMPLE_RATE / TICK_RATE) as usize;

const _: () = assert!(
    SAMPLE_RATE.is_multiple_of(TICK_RATE),
    "a tick must span a whole number of samples"
);

// The limits cartridges see are part of the cartridge audio contract. The
// PSG takes them from there rather than defining its own.
pub use glaux_audio::{CHANNELS, MAX_FREQUENCY, MAX_SUSTAIN, MAX_VOLUME};

const _: () = assert!(
    MAX_FREQUENCY as u32 <= SAMPLE_RATE / 2,
    "the highest frequency must stay within the Nyquist limit"
);

/// PSG samples in one millisecond.
pub const SAMPLES_PER_MS: u32 = SAMPLE_RATE / 1000;

const _: () = assert!(
    SAMPLE_RATE.is_multiple_of(1000),
    "milliseconds must convert to whole samples"
);

/// Sample magnitude of one channel at [`MAX_VOLUME`].
///
/// Four channels at full volume sum to 32764, so mixing up to four never
/// leaves the `i16` range.
pub const CHANNEL_PEAK: i16 = 8191;

const _: () = assert!(
    CHANNEL_PEAK as i32 * CHANNELS as i32 <= i16::MAX as i32,
    "every channel at full volume must mix within i16"
);

/// Full envelope level. Envelope levels run from 0 to this value.
pub const ENVELOPE_FULL: u32 = 1 << 16;

/// Longest attack, decay or release, in samples:
/// [`MAX_ENVELOPE_MS`](glaux_audio::MAX_ENVELOPE_MS) converted to samples.
pub const MAX_ENVELOPE_SAMPLES: u32 = glaux_audio::MAX_ENVELOPE_MS * SAMPLES_PER_MS;

/// Noise register state after reset. Any nonzero value works. Zero would
/// lock the register.
const LFSR_SEED: u16 = 0x7FFF;

/// The shape a channel plays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Waveform {
    /// 50% duty square: high for the first half of the period.
    #[default]
    Square,
    /// Linear ramp from the low peak at the start of the period to the high
    /// peak at its middle, and back.
    Triangle,
    /// Linear ramp from the low peak to the high peak over the period.
    Saw,
    /// Pseudo-random high/low levels from a 15-bit LFSR, which shifts once
    /// per period, so the channel frequency is the shift rate.
    Noise,
}

/// Envelope parameters for one channel.
///
/// The envelope is linear and constant-rate. `attack`, `decay` and
/// `release` are each the number of samples a ramp takes to cross the full
/// range from 0 to [`ENVELOPE_FULL`]. A ramp that starts or ends partway
/// takes proportionally fewer samples, so decay lasts about
/// `decay * (MAX_SUSTAIN - sustain) / MAX_SUSTAIN` samples and a release from
/// level `L` about `release * L / ENVELOPE_FULL`. A duration of 0 completes
/// the stage at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Adsr {
    /// Samples from 0 to full level, up to [`MAX_ENVELOPE_SAMPLES`].
    pub attack: u32,
    /// Samples from full level to 0, up to [`MAX_ENVELOPE_SAMPLES`].
    pub decay: u32,
    /// Level held while the gate stays on, from 0 to [`MAX_SUSTAIN`]. Sustain
    /// at [`MAX_SUSTAIN`] holds [`ENVELOPE_FULL`].
    pub sustain: u8,
    /// Samples from full level to 0, up to [`MAX_ENVELOPE_SAMPLES`].
    pub release: u32,
}

impl Default for Adsr {
    /// An instant envelope: full level as soon as the gate opens and silence
    /// as soon as it closes.
    fn default() -> Self {
        Self {
            attack: 0,
            decay: 0,
            sustain: MAX_SUSTAIN,
            release: 0,
        }
    }
}

impl Adsr {
    fn clamped(self) -> Self {
        Self {
            attack: self.attack.min(MAX_ENVELOPE_SAMPLES),
            decay: self.decay.min(MAX_ENVELOPE_SAMPLES),
            sustain: self.sustain.min(MAX_SUSTAIN),
            release: self.release.min(MAX_ENVELOPE_SAMPLES),
        }
    }

    /// The envelope level `sustain` stands for, truncated.
    fn sustain_level(&self) -> u32 {
        ENVELOPE_FULL * u32::from(self.sustain) / u32::from(MAX_SUSTAIN)
    }
}

/// Where a channel's envelope is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EnvelopeStage {
    /// Gate off and level 0.
    #[default]
    Idle,
    /// Gate on, rising to [`ENVELOPE_FULL`].
    Attack,
    /// Gate on, falling from [`ENVELOPE_FULL`] to the sustain level.
    Decay,
    /// Gate on, holding the sustain level.
    Sustain,
    /// Gate off, falling to 0.
    Release,
}

/// A channel's amplitude envelope.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Envelope {
    adsr: Adsr,
    stage: EnvelopeStage,
    /// 0 to [`ENVELOPE_FULL`].
    level: u32,
    /// Remainder carried between ramp steps, below the current stage's
    /// duration. Each step moves the level by `ENVELOPE_FULL / duration`
    /// plus one whenever the accumulated `ENVELOPE_FULL % duration` reaches
    /// the duration, so a full-range ramp takes exactly `duration` steps.
    fraction: u32,
}

impl Envelope {
    /// The gate is on exactly while the envelope is in Attack, Decay or
    /// Sustain.
    fn gate(&self) -> bool {
        matches!(
            self.stage,
            EnvelopeStage::Attack | EnvelopeStage::Decay | EnvelopeStage::Sustain
        )
    }

    fn gate_on(&mut self) {
        self.enter(EnvelopeStage::Attack);
        self.settle();
    }

    fn gate_off(&mut self) {
        if self.gate() {
            self.enter(EnvelopeStage::Release);
            self.settle();
        }
    }

    fn set_adsr(&mut self, adsr: Adsr) {
        let adsr = adsr.clamped();
        // Rewriting the same parameters must not disturb a ramp in progress.
        if adsr != self.adsr {
            self.adsr = adsr;
            self.fraction = 0;
            self.settle();
        }
    }

    fn enter(&mut self, stage: EnvelopeStage) {
        self.stage = stage;
        self.fraction = 0;
    }

    /// Ends every stage whose duration is 0 or whose target the level has
    /// already reached, so an active ramp always has a nonzero duration and
    /// somewhere left to go.
    fn settle(&mut self) {
        loop {
            match self.stage {
                EnvelopeStage::Idle => return,
                EnvelopeStage::Attack => {
                    if self.adsr.attack != 0 && self.level < ENVELOPE_FULL {
                        return;
                    }
                    self.level = ENVELOPE_FULL;
                    self.enter(EnvelopeStage::Decay);
                }
                EnvelopeStage::Decay => {
                    let sustain = self.adsr.sustain_level();
                    if self.adsr.decay != 0 && self.level > sustain {
                        return;
                    }
                    self.level = sustain;
                    self.enter(EnvelopeStage::Sustain);
                }
                EnvelopeStage::Sustain => {
                    self.level = self.adsr.sustain_level();
                    return;
                }
                EnvelopeStage::Release => {
                    if self.adsr.release != 0 && self.level > 0 {
                        return;
                    }
                    self.level = 0;
                    self.enter(EnvelopeStage::Idle);
                    return;
                }
            }
        }
    }

    /// Moves the envelope one sample along its current ramp.
    fn advance(&mut self) {
        let (duration, rising) = match self.stage {
            EnvelopeStage::Attack => (self.adsr.attack, true),
            EnvelopeStage::Decay => (self.adsr.decay, false),
            EnvelopeStage::Release => (self.adsr.release, false),
            EnvelopeStage::Idle | EnvelopeStage::Sustain => return,
        };
        // `settle` leaves no active ramp with a zero duration, and
        // `fraction < duration <= MAX_ENVELOPE_SAMPLES`, so the sum cannot
        // overflow.
        let mut delta = ENVELOPE_FULL / duration;
        self.fraction += ENVELOPE_FULL % duration;
        if self.fraction >= duration {
            self.fraction -= duration;
            delta += 1;
        }
        self.level = if rising {
            (self.level + delta).min(ENVELOPE_FULL)
        } else {
            self.level.saturating_sub(delta)
        };
        self.settle();
    }
}

/// One PSG voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channel {
    waveform: Waveform,
    frequency: u16,
    volume: u8,
    /// Position within the current period. One full period spans the whole
    /// `u32` range.
    phase: u32,
    /// 15-bit noise register. It shifts each time the phase wraps while the
    /// waveform is [`Waveform::Noise`], and is otherwise left alone.
    lfsr: u16,
    /// Scales the volume. It advances every sample regardless of the
    /// frequency or volume, and never touches the oscillator.
    envelope: Envelope,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            waveform: Waveform::default(),
            frequency: 0,
            volume: 0,
            phase: 0,
            lfsr: LFSR_SEED,
            envelope: Envelope::default(),
        }
    }
}

impl Channel {
    pub fn waveform(&self) -> Waveform {
        self.waveform
    }

    /// Selects the waveform. The phase and noise register are kept, so a
    /// change of shape does not restart the period.
    pub fn set_waveform(&mut self, waveform: Waveform) {
        self.waveform = waveform;
    }

    /// Frequency in Hz. This is waveform periods per second, or
    /// noise-register shifts per second for [`Waveform::Noise`]. 0 silences
    /// the channel and holds its phase.
    pub fn frequency(&self) -> u16 {
        self.frequency
    }

    /// Sets the frequency in Hz, clamped to [`MAX_FREQUENCY`].
    ///
    /// The phase is kept, so a pitch change does not restart the period.
    pub fn set_frequency(&mut self, frequency: u16) {
        self.frequency = frequency.min(MAX_FREQUENCY);
    }

    /// Volume from 0 (silent) to [`MAX_VOLUME`].
    pub fn volume(&self) -> u8 {
        self.volume
    }

    /// Sets the volume, clamped to [`MAX_VOLUME`].
    pub fn set_volume(&mut self, volume: u8) {
        self.volume = volume.min(MAX_VOLUME);
    }

    pub fn adsr(&self) -> Adsr {
        self.envelope.adsr
    }

    /// Sets the envelope parameters, clamping each to its range.
    ///
    /// The envelope keeps its stage and level, but a stage the new
    /// parameters have already completed ends at once. For example, lowering
    /// the sustain below the level a decay has reached moves to Sustain at
    /// the new sustain level, and a release of 0 ends a release. Writing the
    /// parameters already set changes nothing.
    pub fn set_adsr(&mut self, adsr: Adsr) {
        self.envelope.set_adsr(adsr);
    }

    /// Opens the gate. The envelope attacks from its current level, even if
    /// the gate was already open. The oscillator phase is kept.
    pub fn gate_on(&mut self) {
        self.envelope.gate_on();
    }

    /// Closes the gate. The envelope releases from its current level. Does
    /// nothing while the gate is already closed. The oscillator phase is
    /// kept.
    pub fn gate_off(&mut self) {
        self.envelope.gate_off();
    }

    /// Whether the gate is open, which is the case while the envelope is in
    /// Attack, Decay or Sustain.
    pub fn gate(&self) -> bool {
        self.envelope.gate()
    }

    pub fn envelope_stage(&self) -> EnvelopeStage {
        self.envelope.stage
    }

    /// The envelope level the next sample plays at, from 0 to
    /// [`ENVELOPE_FULL`].
    pub fn envelope_level(&self) -> u32 {
        self.envelope.level
    }

    /// Phase advance per output sample.
    fn step(&self) -> u32 {
        // `frequency <= MAX_FREQUENCY < SAMPLE_RATE`, so the quotient is
        // below 2^32.
        ((u64::from(self.frequency) << 32) / u64::from(SAMPLE_RATE)) as u32
    }

    /// The channel's sample at its current phase.
    fn level(&self) -> i16 {
        if self.volume == 0 || self.frequency == 0 {
            return 0;
        }
        // At ENVELOPE_FULL this is exactly `CHANNEL_PEAK * volume /
        // MAX_VOLUME`, and it never exceeds CHANNEL_PEAK.
        let amplitude =
            (i64::from(CHANNEL_PEAK) * i64::from(self.volume) * i64::from(self.envelope.level)
                / (i64::from(MAX_VOLUME) * i64::from(ENVELOPE_FULL))) as i32;
        // `shape` spans -32768..=32768 and `amplitude <= CHANNEL_PEAK`, so
        // the product fits in i32 and the result's magnitude is at most
        // `amplitude`.
        let shape = match self.waveform {
            Waveform::Square => {
                if self.phase < 1 << 31 {
                    32768
                } else {
                    -32768
                }
            }
            Waveform::Triangle => {
                // Distance from the middle of the period, in 2^15 phase
                // units. Folding on its absolute value makes the wave exactly
                // even about the middle and, with division truncating toward
                // zero, exactly odd about the quarter points.
                let from_middle = (self.phase >> 15) as i32 - 65536;
                32768 - from_middle.abs()
            }
            Waveform::Saw => (self.phase >> 16) as i32 - 32768,
            Waveform::Noise => {
                if self.lfsr & 1 == 1 {
                    32768
                } else {
                    -32768
                }
            }
        };
        (shape * amplitude / 32768) as i16
    }

    /// Moves to the next sample, shifting the noise register when a noise
    /// period ends and advancing the envelope.
    fn advance(&mut self) {
        let (phase, wrapped) = self.phase.overflowing_add(self.step());
        self.phase = phase;
        // `step() <= 2^31`, so a sample ends at most one period.
        if wrapped && self.waveform == Waveform::Noise {
            self.lfsr = shift_lfsr(self.lfsr);
        }
        self.envelope.advance();
    }
}

/// One shift of the 15-bit noise register, with feedback from bits 0 and 1
/// into bit 14. From a nonzero state it cycles through all 32767 nonzero
/// states.
fn shift_lfsr(lfsr: u16) -> u16 {
    let feedback = (lfsr ^ (lfsr >> 1)) & 1;
    (lfsr >> 1) | (feedback << 14)
}

/// The PSG: its channels and their mix.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Psg {
    channels: [Channel; CHANNELS],
}

impl Psg {
    /// A PSG with every channel silent.
    pub fn new() -> Self {
        Self::default()
    }

    /// Channel `index`, or `None` past the last channel.
    pub fn channel(&self, index: usize) -> Option<&Channel> {
        self.channels.get(index)
    }

    /// Channel `index`, or `None` past the last channel.
    pub fn channel_mut(&mut self, index: usize) -> Option<&mut Channel> {
        self.channels.get_mut(index)
    }

    /// Fills `out` with the next `out.len()` samples of the mix, advancing
    /// every channel.
    pub fn render(&mut self, out: &mut [i16]) {
        for sample in out {
            *sample = self.channels.iter_mut().fold(0i16, |mix, channel| {
                let level = channel.level();
                channel.advance();
                mix.saturating_add(level)
            });
        }
    }
}

#[cfg(test)]
mod tests;
