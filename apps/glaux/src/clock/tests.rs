use super::*;
use pretty_assertions::assert_eq;

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
fn one_second_of_host_time_is_exactly_tick_rate_ticks() {
    // Millisecond frames stay under the catch-up cap. A step rounded up
    // instead of truncated would leave the last tick of the second due
    // only after it ends.
    assert_eq!(ticks_over(1000, 1_000_000), TICK_RATE);
    assert_eq!(ticks_over(999, 1_000_000), TICK_RATE - 1);
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
