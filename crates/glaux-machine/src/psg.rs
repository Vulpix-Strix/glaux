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
pub const CHANNELS: usize = 1;

/// Loudest channel volume. Volume 0 is silent.
pub const MAX_VOLUME: u8 = 15;

/// Highest frequency a channel plays, in Hz: the Nyquist limit.
pub const MAX_FREQUENCY: u16 = (SAMPLE_RATE / 2) as u16;

/// Sample magnitude of one channel at [`MAX_VOLUME`].
///
/// Four channels at full volume sum to 32764, so mixing up to four never
/// leaves the `i16` range.
pub const CHANNEL_PEAK: i16 = 8191;

/// A square-wave channel with a 50% duty cycle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SquareChannel {
    frequency: u16,
    volume: u8,
    /// Position within the current period; one full period spans the whole
    /// `u32` range. The first half of the period is the high level.
    phase: u32,
}

impl SquareChannel {
    /// Frequency in Hz. 0 holds the channel at its current level.
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
        let amplitude =
            (i32::from(CHANNEL_PEAK) * i32::from(self.volume) / i32::from(MAX_VOLUME)) as i16;
        if self.phase < 1 << 31 {
            amplitude
        } else {
            -amplitude
        }
    }
}

/// The PSG: its channels and their mix.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Psg {
    channels: [SquareChannel; CHANNELS],
}

impl Psg {
    /// A PSG with every channel silent.
    pub fn new() -> Self {
        Self::default()
    }

    /// Channel `index`, or `None` past the last channel.
    pub fn channel(&self, index: usize) -> Option<&SquareChannel> {
        self.channels.get(index)
    }

    /// Channel `index`, or `None` past the last channel.
    pub fn channel_mut(&mut self, index: usize) -> Option<&mut SquareChannel> {
        self.channels.get_mut(index)
    }

    /// Fills `out` with the next `out.len()` samples of the mix, advancing
    /// every channel's phase.
    pub fn render(&mut self, out: &mut [i16]) {
        for sample in out {
            *sample = self.channels.iter_mut().fold(0i16, |mix, channel| {
                let level = channel.level();
                channel.phase = channel.phase.wrapping_add(channel.step());
                mix.saturating_add(level)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn tone(frequency: u16, volume: u8) -> Psg {
        let mut psg = Psg::new();
        let channel = psg.channel_mut(0).unwrap();
        channel.set_frequency(frequency);
        channel.set_volume(volume);
        psg
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
    fn amplitude_follows_volume() {
        let peak = |volume| render(&mut tone(1000, volume), 1)[0];
        assert_eq!(peak(MAX_VOLUME), CHANNEL_PEAK);
        // 8191 * 5 / 15, truncated.
        assert_eq!(peak(5), 2730);
        assert_eq!(peak(0), 0);
        assert_eq!(render(&mut tone(1000, 0), 64), vec![0; 64]);
    }

    #[test]
    fn zero_frequency_is_silent() {
        assert_eq!(render(&mut tone(0, MAX_VOLUME), 64), vec![0; 64]);
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
        let whole = render(&mut tone(441, 9), SAMPLES_PER_TICK * 2);

        let mut split = tone(441, 9);
        let mut parts = render(&mut split, 333);
        parts.extend(render(&mut split, SAMPLES_PER_TICK * 2 - 333));

        assert_eq!(parts, whole);
    }

    #[test]
    fn same_writes_produce_identical_samples() {
        let run = || {
            let mut psg = tone(440, 12);
            let mut out = render(&mut psg, SAMPLES_PER_TICK);
            psg.channel_mut(0).unwrap().set_frequency(523);
            out.extend(render(&mut psg, SAMPLES_PER_TICK));
            out
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn channels_past_the_last_are_absent() {
        let mut psg = Psg::new();
        assert!(psg.channel(CHANNELS).is_none());
        assert!(psg.channel_mut(CHANNELS).is_none());
    }
}
