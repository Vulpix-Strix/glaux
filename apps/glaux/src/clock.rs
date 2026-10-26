//! Fixed-rate logical time, decoupled from host presentation.

#![warn(unreachable_pub)]

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
pub(crate) struct TickClock {
    /// Time not yet consumed by a tick. It is always shorter than one step
    /// after [`advance`](Self::advance) returns.
    accumulated: Duration,
}

impl TickClock {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Adds `elapsed` host time and returns how many ticks are now due, at
    /// most [`MAX_CATCH_UP`].
    ///
    /// Only the sub-tick remainder carries over. Whole ticks beyond the cap
    /// are discarded so the tick phase is kept without a catch-up burst.
    pub(crate) fn advance(&mut self, elapsed: Duration) -> u32 {
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
mod tests;
