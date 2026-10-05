//! Fixed-rate logical time, decoupled from host presentation.

use std::time::Duration;

use glaux_machine::machine::TICK_RATE;

/// Most ticks run for one host frame. After a longer stall (a window drag,
/// a debugger pause) the excess is dropped instead of replayed in a burst.
const MAX_CATCH_UP: u32 = 4;

/// Length of one machine tick.
const STEP: Duration = Duration::from_nanos(1_000_000_000 / TICK_RATE as u64);

/// Converts elapsed host wall-clock time into whole [`TICK_RATE`] machine
/// ticks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TickClock {
    /// Time not yet consumed by a tick; always shorter than one step after
    /// [`advance`](Self::advance) returns.
    accumulated: Duration,
}

impl TickClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `elapsed` host time and returns how many ticks are now due, at
    /// most [`MAX_CATCH_UP`].
    ///
    /// Only the sub-tick remainder carries over; whole ticks beyond the cap
    /// are discarded so the tick phase is kept without a catch-up burst.
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        let total = self.accumulated.saturating_add(elapsed);
        let step = STEP.as_nanos();
        let due = total.as_nanos() / step;
        let remainder = total.as_nanos() % step;
        // The remainder is below one step, so it always fits in u64.
        self.accumulated = Duration::from_nanos(remainder as u64);
        u32::try_from(due).unwrap_or(u32::MAX).min(MAX_CATCH_UP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn step_matches_tick_rate() {
        assert_eq!(STEP, Duration::from_nanos(16_666_666));
    }

    #[test]
    fn short_frames_accumulate_into_one_tick() {
        let mut clock = TickClock::new();
        let frame = Duration::from_millis(7);
        assert_eq!(clock.advance(frame), 0);
        assert_eq!(clock.advance(frame), 0);
        assert_eq!(clock.advance(frame), 1);
        assert_eq!(clock.accumulated, frame * 3 - STEP);
    }

    #[test]
    fn exact_steps_tick_with_no_remainder() {
        let mut clock = TickClock::new();
        assert_eq!(clock.advance(STEP), 1);
        assert_eq!(clock.accumulated, Duration::ZERO);
        assert_eq!(clock.advance(STEP * 3), 3);
        assert_eq!(clock.accumulated, Duration::ZERO);
        assert_eq!(clock.advance(STEP - Duration::from_nanos(1)), 0);
    }

    #[test]
    fn stall_is_capped_and_keeps_only_the_remainder() {
        let mut clock = TickClock::new();
        let stall = Duration::from_secs(1) + Duration::from_millis(5);
        assert_eq!(clock.advance(stall), MAX_CATCH_UP);
        assert_eq!(
            clock.accumulated,
            Duration::from_nanos((stall.as_nanos() % STEP.as_nanos()) as u64)
        );
        assert!(clock.accumulated < STEP);
        // The next ordinary frame does not replay the dropped ticks.
        assert_eq!(clock.advance(Duration::from_millis(1)), 0);
    }

    /// Total ticks after feeding `frames` frames of `frame_nanos` each.
    fn ticks_over(frames: u32, frame_nanos: u64) -> u32 {
        let mut clock = TickClock::new();
        (0..frames)
            .map(|_| clock.advance(Duration::from_nanos(frame_nanos)))
            .sum()
    }

    #[test]
    fn refresh_rate_does_not_change_logical_rate() {
        // One simulated second at each refresh rate. Truncated frame
        // lengths can leave the last tick a few nanoseconds short.
        for hz in [30, 60, 75, 120, 144, 240] {
            let ticks = ticks_over(hz, 1_000_000_000 / u64::from(hz) + 1);
            assert_eq!(ticks, TICK_RATE, "{hz} Hz");
        }
    }
}
