//! Programmable sound generator.
//!
//! The PSG turns its channel state into mono signed 16-bit samples at
//! [`SAMPLE_RATE`]. Synthesis uses integer math only, so the same sequence of
//! register writes and render calls always yields the same samples.

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
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            waveform: Waveform::default(),
            frequency: 0,
            volume: 0,
            phase: 0,
            lfsr: LFSR_SEED,
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
        let amplitude = i32::from(CHANNEL_PEAK) * i32::from(self.volume) / i32::from(MAX_VOLUME);
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
    /// period ends.
    fn advance(&mut self) {
        let (phase, wrapped) = self.phase.overflowing_add(self.step());
        self.phase = phase;
        // `step() <= 2^31`, so a sample ends at most one period.
        if wrapped && self.waveform == Waveform::Noise {
            self.lfsr = shift_lfsr(self.lfsr);
        }
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
}
