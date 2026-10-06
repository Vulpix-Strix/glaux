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

/// Number of PSG channels.
pub const CHANNELS: usize = 4;

/// Loudest channel volume. Volume 0 is silent.
pub const MAX_VOLUME: u8 = 15;

/// Highest frequency a channel plays, in Hz: the Nyquist limit.
pub const MAX_FREQUENCY: u16 = (SAMPLE_RATE / 2) as u16;

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

/// Highest sustain value: sustain at this value holds [`ENVELOPE_FULL`].
pub const MAX_SUSTAIN: u8 = 15;

/// Longest attack, decay or release, in samples: 10 seconds.
pub const MAX_ENVELOPE_SAMPLES: u32 = 10 * SAMPLE_RATE;

/// Noise register state after reset. Any nonzero value works; zero would
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
    /// per period: the channel frequency is the shift rate.
    Noise,
}

/// Envelope parameters for one channel.
///
/// The envelope is linear and constant-rate: `attack`, `decay` and
/// `release` are each the number of samples a ramp takes to cross the full
/// range from 0 to [`ENVELOPE_FULL`]. A ramp that starts or ends partway takes
/// proportionally fewer samples, so decay lasts about
/// `decay * (MAX_SUSTAIN - sustain) / MAX_SUSTAIN` samples and a release from
/// level `L` about `release * L / ENVELOPE_FULL`. A duration of 0 completes
/// the stage at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Adsr {
    /// Samples from 0 to full level, up to [`MAX_ENVELOPE_SAMPLES`].
    pub attack: u32,
    /// Samples from full level to 0, up to [`MAX_ENVELOPE_SAMPLES`].
    pub decay: u32,
    /// Level held while the gate stays on, from 0 to [`MAX_SUSTAIN`].
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
    /// Position within the current period; one full period spans the whole
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

    /// Frequency in Hz: waveform periods per second, or for
    /// [`Waveform::Noise`] noise-register shifts per second. 0 silences the
    /// channel and holds its phase.
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
    /// parameters have already completed ends at once: for example, lowering
    /// the sustain below the level a decay has reached moves to Sustain at
    /// the new sustain level, and a release of 0 ends a release. Writing the
    /// parameters already set changes nothing.
    pub fn set_adsr(&mut self, adsr: Adsr) {
        self.envelope.set_adsr(adsr);
    }

    /// Opens the gate: the envelope attacks from its current level, even if
    /// the gate was already open. The oscillator phase is kept.
    pub fn gate_on(&mut self) {
        self.envelope.gate_on();
    }

    /// Closes the gate: the envelope releases from its current level. Does
    /// nothing while the gate is already closed. The oscillator phase is
    /// kept.
    pub fn gate_off(&mut self) {
        self.envelope.gate_off();
    }

    /// Whether the gate is open: the envelope is in Attack, Decay or
    /// Sustain.
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
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const WAVEFORMS: [Waveform; 4] = [
        Waveform::Square,
        Waveform::Triangle,
        Waveform::Saw,
        Waveform::Noise,
    ];

    fn set(psg: &mut Psg, index: usize, waveform: Waveform, frequency: u16, volume: u8) {
        let channel = psg.channel_mut(index).unwrap();
        channel.set_waveform(waveform);
        channel.set_frequency(frequency);
        channel.set_volume(volume);
        channel.gate_on();
    }

    fn voice(waveform: Waveform, frequency: u16, volume: u8) -> Psg {
        let mut psg = Psg::new();
        set(&mut psg, 0, waveform, frequency, volume);
        psg
    }

    fn tone(frequency: u16, volume: u8) -> Psg {
        voice(Waveform::Square, frequency, volume)
    }

    fn render(psg: &mut Psg, len: usize) -> Vec<i16> {
        let mut out = vec![0; len];
        psg.render(&mut out);
        out
    }

    #[test]
    fn a_tick_spans_the_sample_rate() {
        assert_eq!(SAMPLES_PER_TICK * TICK_RATE as usize, SAMPLE_RATE as usize);
    }

    #[test]
    fn starts_silent() {
        let mut psg = Psg::new();
        assert_eq!(
            render(&mut psg, SAMPLES_PER_TICK),
            vec![0; SAMPLES_PER_TICK]
        );
    }

    #[test]
    fn has_four_channels() {
        let mut psg = Psg::new();
        assert_eq!(CHANNELS, 4);
        assert!(psg.channel(CHANNELS - 1).is_some());
        assert!(psg.channel(CHANNELS).is_none());
        assert!(psg.channel_mut(CHANNELS).is_none());
    }

    #[test]
    fn channels_default_to_square() {
        let psg = Psg::new();
        for index in 0..CHANNELS {
            assert_eq!(psg.channel(index).unwrap().waveform(), Waveform::Square);
        }
    }

    #[test]
    fn square_has_the_requested_period_and_duty() {
        // 48 kHz / 750 Hz = 64 samples per period, with an exact 2^26 step.
        let mut psg = tone(750, MAX_VOLUME);
        let samples = render(&mut psg, 128);
        let period: Vec<i16> = [vec![CHANNEL_PEAK; 32], vec![-CHANNEL_PEAK; 32]].concat();
        assert_eq!(samples[..64], period[..]);
        assert_eq!(samples[64..], period[..]);
    }

    #[test]
    fn inexact_steps_keep_the_average_period() {
        // The 1 kHz step is truncated, which can delay an edge by a sample
        // but lowers the pitch by less than one part in 10^8. The first
        // period starts high, so one second holds 999 rising edges.
        let samples = render(&mut tone(1000, MAX_VOLUME), SAMPLE_RATE as usize);
        let rising = samples
            .windows(2)
            .filter(|pair| pair[0] < 0 && pair[1] > 0)
            .count();
        assert_eq!(rising, 999);
    }

    #[test]
    fn saw_rises_across_the_period() {
        let samples = render(&mut voice(Waveform::Saw, 750, MAX_VOLUME), 128);
        assert_eq!(samples[0], -CHANNEL_PEAK);
        // Phase 2^30: -16384 * 8191 / 32768, truncated toward zero.
        assert_eq!(samples[16], -4095);
        assert_eq!(samples[32], 0);
        assert!(samples[..64].windows(2).all(|pair| pair[0] < pair[1]));
        assert!(samples[..64].iter().all(|s| s.abs() <= CHANNEL_PEAK));
        assert_eq!(samples[..64], samples[64..]);
    }

    #[test]
    fn triangle_is_symmetric() {
        // Sample k sits 2048k - 65536 phase units from the middle.
        let samples = render(&mut voice(Waveform::Triangle, 750, MAX_VOLUME), 128);
        let s = |k: usize| samples[k];
        assert_eq!(
            [s(0), s(16), s(32), s(48), s(64)],
            [-CHANNEL_PEAK, 0, CHANNEL_PEAK, 0, -CHANNEL_PEAK]
        );
        for j in 0..=32 {
            assert_eq!(s(32 - j), s(32 + j), "even about the middle, j = {j}");
        }
        for j in 0..=16 {
            assert_eq!(
                s(16 - j),
                -s(16 + j),
                "odd about the first quarter, j = {j}"
            );
            assert_eq!(
                s(48 - j),
                -s(48 + j),
                "odd about the third quarter, j = {j}"
            );
        }
        assert!(samples[..32].windows(2).all(|pair| pair[0] < pair[1]));
        assert!(samples[32..64].windows(2).all(|pair| pair[0] > pair[1]));
        assert_eq!(samples[..64], samples[64..]);
    }

    #[test]
    fn noise_follows_the_register_from_reset() {
        // At the Nyquist limit the register shifts every two samples. From
        // the all-ones seed the feedback stays 0 while two low bits are set:
        // 15 high levels while the ones shift out, then 14 low levels while
        // the single fed-back one travels down from bit 14.
        let samples = render(&mut voice(Waveform::Noise, MAX_FREQUENCY, MAX_VOLUME), 60);
        let levels: Vec<i16> = samples.chunks(2).map(|pair| pair[0]).collect();
        let expected: Vec<i16> = [
            vec![CHANNEL_PEAK; 15],
            vec![-CHANNEL_PEAK; 14],
            vec![CHANNEL_PEAK],
        ]
        .concat();
        assert_eq!(levels, expected);
        assert!(samples.chunks(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn noise_holds_its_level_for_a_period() {
        let samples = render(&mut voice(Waveform::Noise, 750, MAX_VOLUME), 64 * 40);
        for period in samples.chunks(64) {
            assert!(period.iter().all(|&s| s == period[0]));
        }
    }

    #[test]
    fn noise_is_deterministic() {
        let run = || render(&mut voice(Waveform::Noise, 1200, 9), SAMPLES_PER_TICK * 4);
        let samples = run();
        assert_eq!(samples, run());
        // 8191 * 9 / 15, truncated.
        assert!(samples.contains(&4914));
        assert!(samples.contains(&-4914));
    }

    #[test]
    fn noise_register_has_the_maximal_period() {
        let mut lfsr = LFSR_SEED;
        for shifts in 1..=32767 {
            lfsr = shift_lfsr(lfsr);
            assert_ne!(lfsr, 0);
            if lfsr == LFSR_SEED {
                assert_eq!(shifts, 32767);
                return;
            }
        }
        panic!("the register did not return to its seed");
    }

    #[test]
    fn noise_does_not_advance_without_a_frequency() {
        let mut psg = voice(Waveform::Noise, 0, MAX_VOLUME);
        assert_eq!(render(&mut psg, 64), vec![0; 64]);
        let channel = psg.channel(0).unwrap();
        assert_eq!((channel.phase, channel.lfsr), (0, LFSR_SEED));
    }

    #[test]
    fn waveform_selects_the_shape() {
        let outputs: Vec<Vec<i16>> = WAVEFORMS
            .iter()
            .map(|&waveform| {
                let mut psg = voice(waveform, 750, MAX_VOLUME);
                assert_eq!(psg.channel(0).unwrap().waveform(), waveform);
                render(&mut psg, 64 * 20)
            })
            .collect();
        for (i, a) in outputs.iter().enumerate() {
            for b in &outputs[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn amplitude_follows_volume() {
        let peak = |volume| render(&mut tone(1000, volume), 1)[0];
        assert_eq!(peak(MAX_VOLUME), CHANNEL_PEAK);
        // 8191 * 5 / 15, truncated.
        assert_eq!(peak(5), 2730);
        assert_eq!(peak(0), 0);
    }

    #[test]
    fn zero_volume_or_frequency_is_silent_for_every_waveform() {
        for waveform in WAVEFORMS {
            assert_eq!(
                render(&mut voice(waveform, 1000, 0), 256),
                vec![0; 256],
                "{waveform:?} at volume 0"
            );
            assert_eq!(
                render(&mut voice(waveform, 0, MAX_VOLUME), 256),
                vec![0; 256],
                "{waveform:?} at 0 Hz"
            );
        }
    }

    #[test]
    fn setters_clamp() {
        let psg = tone(u16::MAX, u8::MAX);
        let channel = psg.channel(0).unwrap();
        assert_eq!(channel.frequency(), MAX_FREQUENCY);
        assert_eq!(channel.volume(), MAX_VOLUME);
    }

    #[test]
    fn highest_frequency_alternates_every_sample() {
        let samples = render(&mut tone(MAX_FREQUENCY, MAX_VOLUME), 4);
        assert_eq!(
            samples,
            vec![CHANNEL_PEAK, -CHANNEL_PEAK, CHANNEL_PEAK, -CHANNEL_PEAK]
        );
    }

    #[test]
    fn phase_carries_across_render_calls() {
        for waveform in WAVEFORMS {
            let whole = render(&mut voice(waveform, 441, 9), SAMPLES_PER_TICK * 2);

            let mut split = voice(waveform, 441, 9);
            let mut parts = render(&mut split, 333);
            parts.extend(render(&mut split, SAMPLES_PER_TICK * 2 - 333));

            assert_eq!(parts, whole, "{waveform:?}");
        }
    }

    #[test]
    fn phase_survives_a_frequency_change() {
        let mut psg = voice(Waveform::Saw, 750, MAX_VOLUME);
        render(&mut psg, 16);
        psg.channel_mut(0).unwrap().set_frequency(1500);
        // Phase 2^30 from the 750 Hz samples, now stepping 2^27.
        assert_eq!(render(&mut psg, 3), vec![-4095, -3583, -3071]);
    }

    #[test]
    fn phase_survives_a_waveform_change() {
        let mut psg = voice(Waveform::Saw, 750, MAX_VOLUME);
        render(&mut psg, 16);
        psg.channel_mut(0).unwrap().set_waveform(Waveform::Triangle);
        // Phase 2^30 is the triangle's first zero crossing.
        assert_eq!(render(&mut psg, 1), vec![0]);
    }

    #[test]
    fn channels_are_independent() {
        let mut psg = Psg::new();
        set(&mut psg, 0, Waveform::Square, 440, 8);
        set(&mut psg, 1, Waveform::Triangle, 660, 6);
        set(&mut psg, 2, Waveform::Saw, 220, 5);
        set(&mut psg, 3, Waveform::Noise, 1200, 4);

        let expected = [
            (Waveform::Square, 440, 8),
            (Waveform::Triangle, 660, 6),
            (Waveform::Saw, 220, 5),
            (Waveform::Noise, 1200, 4),
        ];
        for (index, (waveform, frequency, volume)) in expected.into_iter().enumerate() {
            let channel = psg.channel(index).unwrap();
            assert_eq!(
                (channel.waveform(), channel.frequency(), channel.volume()),
                (waveform, frequency, volume)
            );
        }

        // Silencing channel 1 mid-stream leaves the others' output as if
        // they played alone.
        let mut mixed = render(&mut psg, 500);
        psg.channel_mut(1).unwrap().set_volume(0);
        mixed.extend(render(&mut psg, 500));

        let mut others = Psg::new();
        set(&mut others, 0, Waveform::Square, 440, 8);
        set(&mut others, 2, Waveform::Saw, 220, 5);
        set(&mut others, 3, Waveform::Noise, 1200, 4);
        let others = render(&mut others, 1000);
        assert_eq!(mixed[500..], others[500..]);
    }

    #[test]
    fn mix_is_the_sum_of_the_channels() {
        let voices = [
            (Waveform::Square, 440, 8),
            (Waveform::Triangle, 660, 6),
            (Waveform::Saw, 220, 5),
            (Waveform::Noise, 1200, 4),
        ];
        let mut psg = Psg::new();
        for (index, &(waveform, frequency, volume)) in voices.iter().enumerate() {
            set(&mut psg, index, waveform, frequency, volume);
        }
        let mix = render(&mut psg, SAMPLES_PER_TICK);

        let mut sum = vec![0i16; SAMPLES_PER_TICK];
        for (index, &(waveform, frequency, volume)) in voices.iter().enumerate() {
            let mut alone = Psg::new();
            set(&mut alone, index, waveform, frequency, volume);
            for (total, sample) in sum.iter_mut().zip(render(&mut alone, SAMPLES_PER_TICK)) {
                *total += sample;
            }
        }
        assert_eq!(mix, sum);
    }

    #[test]
    fn four_full_volume_channels_stay_in_range() {
        let mut psg = Psg::new();
        for index in 0..CHANNELS {
            set(&mut psg, index, Waveform::Square, 750, MAX_VOLUME);
        }
        let samples = render(&mut psg, 64);
        assert_eq!(samples[0], 4 * CHANNEL_PEAK);
        assert_eq!(samples[32], -4 * CHANNEL_PEAK);
    }

    #[test]
    fn same_writes_produce_identical_samples() {
        let run = || {
            let mut psg = Psg::new();
            for (index, waveform) in WAVEFORMS.into_iter().enumerate() {
                set(&mut psg, index, waveform, 440 + 110 * index as u16, 12);
            }
            let mut out = render(&mut psg, SAMPLES_PER_TICK);
            psg.channel_mut(0).unwrap().set_frequency(523);
            psg.channel_mut(3).unwrap().set_waveform(Waveform::Saw);
            psg.channel_mut(1).unwrap().set_waveform(Waveform::Noise);
            out.extend(render(&mut psg, SAMPLES_PER_TICK));
            out
        };
        assert_eq!(run(), run());
    }

    // Envelopes. Each sample is emitted at the envelope level read before
    // it, and the envelope advances afterwards: an attack of N samples from
    // 0 emits samples 0..N on the rising ramp, its Nth advance reaches
    // ENVELOPE_FULL, and sample N is the first one emitted at full level.

    /// A square voice on channel 0 at 750 Hz and full volume with `adsr`,
    /// gate still closed.
    fn enveloped(adsr: Adsr) -> Psg {
        let mut psg = Psg::new();
        let channel = psg.channel_mut(0).unwrap();
        channel.set_frequency(750);
        channel.set_volume(MAX_VOLUME);
        channel.set_adsr(adsr);
        psg
    }

    fn adsr(attack: u32, decay: u32, sustain: u8, release: u32) -> Adsr {
        Adsr {
            attack,
            decay,
            sustain,
            release,
        }
    }

    fn ch0(psg: &mut Psg) -> &mut Channel {
        psg.channel_mut(0).unwrap()
    }

    fn level(psg: &Psg) -> u32 {
        psg.channel(0).unwrap().envelope_level()
    }

    fn stage(psg: &Psg) -> EnvelopeStage {
        psg.channel(0).unwrap().envelope_stage()
    }

    /// Channel 0's envelope level for each of the next `len` samples: the
    /// level each sample is emitted at.
    fn levels(psg: &mut Psg, len: usize) -> Vec<u32> {
        (0..len)
            .map(|_| {
                let before = level(psg);
                render(psg, 1);
                before
            })
            .collect()
    }

    /// Envelope advances until channel 0 reaches `target`, failing after
    /// `limit`.
    fn advances_until(psg: &mut Psg, target: EnvelopeStage, limit: usize) -> usize {
        for n in 0..=limit {
            if stage(psg) == target {
                return n;
            }
            render(psg, 1);
        }
        panic!("{target:?} not reached in {limit} samples");
    }

    /// The level after `k` advances of a full-range ramp of `n` samples.
    fn ramp(k: u32, n: u32) -> u32 {
        (u64::from(k) * u64::from(ENVELOPE_FULL) / u64::from(n)) as u32
    }

    #[test]
    fn envelopes_start_idle_with_the_gate_closed() {
        let mut psg = Psg::new();
        for index in 0..CHANNELS {
            let channel = psg.channel(index).unwrap();
            assert_eq!(
                (
                    channel.gate(),
                    channel.envelope_stage(),
                    channel.envelope_level(),
                    channel.adsr()
                ),
                (false, EnvelopeStage::Idle, 0, Adsr::default())
            );
        }
        for index in 0..CHANNELS {
            let channel = psg.channel_mut(index).unwrap();
            channel.set_frequency(750);
            channel.set_volume(MAX_VOLUME);
        }
        assert_eq!(
            render(&mut psg, SAMPLES_PER_TICK),
            vec![0; SAMPLES_PER_TICK]
        );
    }

    #[test]
    fn attack_reaches_full_level_after_exactly_its_length() {
        let mut psg = enveloped(adsr(480, 0, MAX_SUSTAIN, 0));
        ch0(&mut psg).gate_on();
        assert_eq!((stage(&psg), level(&psg)), (EnvelopeStage::Attack, 0));

        let mut probe = psg.clone();
        let rising = levels(&mut probe, 481);
        let expected: Vec<u32> = (0..=480).map(|k| ramp(k, 480)).collect();
        assert_eq!(rising, expected);
        assert!(rising[479] < ENVELOPE_FULL);
        assert_eq!(rising[480], ENVELOPE_FULL);

        let samples = render(&mut psg, 481);
        assert_eq!(samples[0], 0);
        assert!(samples[479].abs() < CHANNEL_PEAK);
        // Sample 480 sits in the low half of the 64-sample square period.
        assert_eq!(samples[480], -CHANNEL_PEAK);
        assert_eq!(stage(&psg), EnvelopeStage::Sustain);
    }

    #[test]
    fn decay_reaches_the_sustain_level_and_sustain_holds_it() {
        let mut psg = enveloped(adsr(0, 4800, 8, 0));
        ch0(&mut psg).gate_on();
        // A zero attack completes at once.
        assert_eq!(
            (stage(&psg), level(&psg)),
            (EnvelopeStage::Decay, ENVELOPE_FULL)
        );

        let mut probe = psg.clone();
        let falling = levels(&mut probe, 2241);
        assert!(falling.windows(2).all(|pair| pair[0] > pair[1]));

        // Sustain 8 is 65536 * 8 / 15 = 34952, reached after
        // ceil((65536 - 34952) * 4800 / 65536) = 2241 advances.
        assert_eq!(advances_until(&mut psg, EnvelopeStage::Sustain, 4800), 2241);
        assert_eq!(level(&psg), 34952);

        let held = render(&mut psg, SAMPLES_PER_TICK * 10);
        assert_eq!((stage(&psg), level(&psg)), (EnvelopeStage::Sustain, 34952));
        // 8191 * 15 * 34952 / (15 * 65536), truncated.
        assert!(held.iter().all(|s| s.abs() == 4368));
    }

    #[test]
    fn release_reaches_zero_and_goes_idle() {
        let mut psg = enveloped(adsr(0, 0, MAX_SUSTAIN, 960));
        ch0(&mut psg).gate_on();
        render(&mut psg, 100);
        ch0(&mut psg).gate_off();
        assert_eq!(
            (stage(&psg), level(&psg), psg.channel(0).unwrap().gate()),
            (EnvelopeStage::Release, ENVELOPE_FULL, false)
        );

        let mut probe = psg.clone();
        let expected: Vec<u32> = (0..=960).map(|k| ENVELOPE_FULL - ramp(k, 960)).collect();
        assert_eq!(levels(&mut probe, 961), expected);

        render(&mut psg, 959);
        assert_eq!(stage(&psg), EnvelopeStage::Release);
        assert!(level(&psg) > 0);
        render(&mut psg, 1);
        assert_eq!((stage(&psg), level(&psg)), (EnvelopeStage::Idle, 0));
        assert_eq!(render(&mut psg, 256), vec![0; 256]);
    }

    #[test]
    fn gate_off_during_attack_releases_from_the_current_level() {
        let mut psg = enveloped(adsr(1000, 0, MAX_SUSTAIN, 1000));
        ch0(&mut psg).gate_on();
        render(&mut psg, 300);
        let reached = ramp(300, 1000);
        assert_eq!(level(&psg), reached);

        ch0(&mut psg).gate_off();
        assert_eq!(
            (stage(&psg), level(&psg)),
            (EnvelopeStage::Release, reached)
        );
        // Equal attack and release rates retrace the 300-sample climb.
        assert_eq!(advances_until(&mut psg, EnvelopeStage::Idle, 1000), 300);
    }

    #[test]
    fn gate_off_during_decay_releases_from_the_current_level() {
        let mut psg = enveloped(adsr(0, 1000, 0, 2000));
        ch0(&mut psg).gate_on();
        render(&mut psg, 250);
        let reached = ENVELOPE_FULL - ramp(250, 1000);
        assert_eq!((stage(&psg), level(&psg)), (EnvelopeStage::Decay, reached));

        ch0(&mut psg).gate_off();
        assert_eq!(
            (stage(&psg), level(&psg)),
            (EnvelopeStage::Release, reached)
        );
        let falling = levels(&mut psg, 100);
        assert_eq!(falling[0], reached);
        assert!(falling.windows(2).all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn gate_on_during_release_attacks_from_the_current_level() {
        let mut psg = enveloped(adsr(1000, 0, MAX_SUSTAIN, 1000));
        ch0(&mut psg).gate_on();
        render(&mut psg, 1000);
        ch0(&mut psg).gate_off();
        render(&mut psg, 400);
        let reached = ENVELOPE_FULL - ramp(400, 1000);
        assert_eq!(level(&psg), reached);

        ch0(&mut psg).gate_on();
        assert_eq!((stage(&psg), level(&psg)), (EnvelopeStage::Attack, reached));
        let mut probe = psg.clone();
        render(&mut probe, 399);
        assert!(level(&probe) < ENVELOPE_FULL);
        assert_eq!(advances_until(&mut psg, EnvelopeStage::Sustain, 1000), 400);
    }

    #[test]
    fn gate_on_during_sustain_retriggers_the_attack() {
        let mut psg = enveloped(adsr(1000, 1000, 8, 0));
        ch0(&mut psg).gate_on();
        advances_until(&mut psg, EnvelopeStage::Sustain, 2000);
        let sustain = level(&psg);

        ch0(&mut psg).gate_on();
        assert_eq!((stage(&psg), level(&psg)), (EnvelopeStage::Attack, sustain));
        advances_until(&mut psg, EnvelopeStage::Decay, 1000);
        assert_eq!(level(&psg), ENVELOPE_FULL);
        advances_until(&mut psg, EnvelopeStage::Sustain, 1000);
        assert_eq!(level(&psg), sustain);
    }

    #[test]
    fn set_adsr_ends_stages_the_new_parameters_have_completed() {
        let mut psg = enveloped(adsr(0, 1000, 0, 1000));
        ch0(&mut psg).gate_on();
        render(&mut psg, 250);
        assert_eq!(level(&psg), 49152);

        // 65536 * 12 / 15 = 52428 is above the decay's current level.
        ch0(&mut psg).set_adsr(adsr(0, 1000, 12, 1000));
        assert_eq!((stage(&psg), level(&psg)), (EnvelopeStage::Sustain, 52428));

        // 65536 * 4 / 15 = 17476.
        ch0(&mut psg).set_adsr(adsr(0, 1000, 4, 1000));
        assert_eq!((stage(&psg), level(&psg)), (EnvelopeStage::Sustain, 17476));

        ch0(&mut psg).gate_off();
        render(&mut psg, 10);
        assert_eq!(stage(&psg), EnvelopeStage::Release);
        ch0(&mut psg).set_adsr(adsr(0, 1000, 4, 0));
        assert_eq!((stage(&psg), level(&psg)), (EnvelopeStage::Idle, 0));
    }

    #[test]
    fn set_adsr_clamps_and_ignores_repeated_writes() {
        let psg = enveloped(adsr(u32::MAX, u32::MAX, u8::MAX, u32::MAX));
        assert_eq!(
            psg.channel(0).unwrap().adsr(),
            adsr(
                MAX_ENVELOPE_SAMPLES,
                MAX_ENVELOPE_SAMPLES,
                MAX_SUSTAIN,
                MAX_ENVELOPE_SAMPLES
            )
        );

        // 7 does not divide ENVELOPE_FULL, so a ramp in progress carries a
        // remainder that a rewrite must leave alone.
        let mut psg = enveloped(adsr(7, 0, MAX_SUSTAIN, 0));
        ch0(&mut psg).gate_on();
        render(&mut psg, 3);
        let mut rewritten = psg.clone();
        ch0(&mut rewritten).set_adsr(adsr(7, 0, MAX_SUSTAIN, 0));
        assert_eq!(rewritten, psg);
        assert_eq!(render(&mut rewritten, 8), render(&mut psg, 8));
    }

    #[test]
    fn envelope_does_not_depend_on_the_oscillator() {
        let shape = adsr(700, 900, 6, 1100);
        let run = |waveform: Waveform, frequency: u16, volume: u8| {
            let mut psg = Psg::new();
            let channel = ch0(&mut psg);
            channel.set_waveform(waveform);
            channel.set_frequency(frequency);
            channel.set_volume(volume);
            channel.set_adsr(shape);
            channel.gate_on();
            let mut seen = levels(&mut psg, 1500);
            ch0(&mut psg).gate_off();
            seen.extend(levels(&mut psg, 1500));
            seen
        };
        let reference = run(Waveform::Square, 750, MAX_VOLUME);
        for waveform in WAVEFORMS {
            for frequency in [0, 750, 1000, MAX_FREQUENCY] {
                for volume in [0, 9, MAX_VOLUME] {
                    assert_eq!(
                        run(waveform, frequency, volume),
                        reference,
                        "{waveform:?} {frequency} Hz volume {volume}"
                    );
                }
            }
        }
    }

    #[test]
    fn gate_changes_keep_the_oscillator_running() {
        for waveform in WAVEFORMS {
            let mut reference = voice(waveform, 441, MAX_VOLUME);
            let reference = render(&mut reference, 3000);

            // With the instant envelope, closing the gate silences the
            // channel while its oscillator keeps going.
            let mut gated = voice(waveform, 441, MAX_VOLUME);
            let mut samples = render(&mut gated, 1000);
            let before = *gated.channel(0).unwrap();
            ch0(&mut gated).gate_off();
            let after = *gated.channel(0).unwrap();
            assert_eq!((after.phase, after.lfsr), (before.phase, before.lfsr));

            samples.extend(render(&mut gated, 1000));
            ch0(&mut gated).gate_on();
            samples.extend(render(&mut gated, 1000));

            assert_eq!(samples[..1000], reference[..1000], "{waveform:?}");
            assert_eq!(samples[1000..2000], vec![0; 1000], "{waveform:?}");
            assert_eq!(samples[2000..], reference[2000..], "{waveform:?}");
        }
    }

    #[test]
    fn envelope_progresses_while_frequency_or_volume_is_zero() {
        for (frequency, volume) in [(0, MAX_VOLUME), (750, 0)] {
            let mut psg = enveloped(adsr(480, 0, MAX_SUSTAIN, 0));
            ch0(&mut psg).set_frequency(frequency);
            ch0(&mut psg).set_volume(volume);
            ch0(&mut psg).gate_on();
            assert_eq!(render(&mut psg, 480), vec![0; 480]);
            assert_eq!(
                (stage(&psg), level(&psg)),
                (EnvelopeStage::Sustain, ENVELOPE_FULL)
            );

            ch0(&mut psg).set_frequency(750);
            ch0(&mut psg).set_volume(MAX_VOLUME);
            assert_eq!(render(&mut psg, 1)[0].abs(), CHANNEL_PEAK);
        }
    }

    /// Writes four channels with distinct envelopes and opens their gates at
    /// different times, closing two of them later.
    fn envelope_script(psg: &mut Psg, only: Option<usize>) {
        let voices = [
            (Waveform::Square, 440, 8, adsr(480, 960, 10, 4800)),
            (Waveform::Triangle, 660, 6, adsr(0, 2400, 4, 480)),
            (Waveform::Saw, 220, 5, adsr(2000, 0, 15, 3000)),
            (Waveform::Noise, 1200, 4, adsr(10, 10, 0, 10)),
        ];
        let active = |index: usize| only.is_none_or(|only| only == index);
        for (index, &(waveform, frequency, volume, shape)) in voices.iter().enumerate() {
            if active(index) {
                let channel = psg.channel_mut(index).unwrap();
                channel.set_waveform(waveform);
                channel.set_frequency(frequency);
                channel.set_volume(volume);
                channel.set_adsr(shape);
            }
        }
        for index in 0..CHANNELS {
            if active(index) {
                psg.channel_mut(index).unwrap().gate_on();
            }
            render(psg, 300);
        }
        render(psg, 1500);
        for index in [0, 2] {
            if active(index) {
                psg.channel_mut(index).unwrap().gate_off();
            }
            render(psg, 700);
        }
    }

    #[test]
    fn channels_keep_independent_envelopes() {
        let mut psg = Psg::new();
        envelope_script(&mut psg, None);
        let stages: Vec<EnvelopeStage> = (0..CHANNELS)
            .map(|index| psg.channel(index).unwrap().envelope_stage())
            .collect();
        assert_eq!(
            stages,
            [
                EnvelopeStage::Release,
                EnvelopeStage::Sustain,
                EnvelopeStage::Release,
                EnvelopeStage::Sustain,
            ]
        );
        for index in 0..CHANNELS {
            let mut alone = Psg::new();
            envelope_script(&mut alone, Some(index));
            assert_eq!(psg.channel(index), alone.channel(index), "channel {index}");
        }
    }

    /// Renders a fixed sequence of writes and gate changes, rendering the
    /// stretches between them in pieces of at most `chunk` samples.
    fn chunked_run(chunk: usize) -> (Vec<i16>, Psg) {
        let mut psg = Psg::new();
        for (index, waveform) in WAVEFORMS.into_iter().enumerate() {
            let channel = psg.channel_mut(index).unwrap();
            channel.set_waveform(waveform);
            channel.set_frequency(330 + 110 * index as u16);
            channel.set_volume(10);
            channel.set_adsr(adsr(333, 777, 7, 1234));
        }
        type Event = fn(&mut Psg);
        let events: [(usize, Event); 6] = [
            (0, |psg| psg.channel_mut(0).unwrap().gate_on()),
            (101, |psg| psg.channel_mut(1).unwrap().gate_on()),
            (900, |psg| psg.channel_mut(0).unwrap().gate_off()),
            (1300, |psg| psg.channel_mut(2).unwrap().gate_on()),
            (1450, |psg| psg.channel_mut(0).unwrap().gate_on()),
            (2100, |psg| psg.channel_mut(3).unwrap().gate_on()),
        ];
        let end = 4000;
        let mut out = Vec::new();
        let mut at = 0;
        for (index, &(offset, event)) in events.iter().enumerate() {
            event(&mut psg);
            let until = events.get(index + 1).map_or(end, |&(next, _)| next);
            debug_assert!(at == offset);
            while at < until {
                let len = chunk.min(until - at);
                out.extend(render(&mut psg, len));
                at += len;
            }
        }
        (out, psg)
    }

    #[test]
    fn chunked_rendering_matches_one_continuous_render() {
        let whole = chunked_run(usize::MAX);
        for chunk in [1, 7, 333, SAMPLES_PER_TICK] {
            assert_eq!(chunked_run(chunk), whole, "chunks of {chunk}");
        }
    }

    #[test]
    fn same_writes_produce_identical_envelopes() {
        let run = || {
            let mut psg = Psg::new();
            envelope_script(&mut psg, None);
            let out = render(&mut psg, SAMPLES_PER_TICK);
            (out, psg)
        };
        assert_eq!(run(), run());
    }
}
