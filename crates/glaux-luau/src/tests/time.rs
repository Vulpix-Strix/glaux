//! `frame` and `t`.

use super::*;
use glaux_machine::machine::TICK_RATE;
use pretty_assertions::assert_eq;

#[test]
fn time_is_zero_during_entry_and_boot() {
    assert_eq!(
        start_output(r#"log.info(frame(), t()) function boot() log.info(frame(), t()) end"#),
        ["0\t0", "0\t0"]
    );
}

#[test]
fn tick_and_draw_see_the_same_frame() {
    let mut runtime = started(
        r#"
        function tick() log.info(frame()) end
        function draw() log.info(frame()) end
        "#,
    );
    for _ in 0..3 {
        runtime.tick(NO_INPUT).unwrap();
    }
    assert_eq!(texts(runtime.take_output()), ["0", "0", "1", "1", "2", "2"]);
}

#[test]
fn t_is_frame_over_the_tick_rate() {
    let mut runtime = started(&format!(
        r#"
        function tick()
            log.info(t() == frame() / {TICK_RATE}, frame() == {TICK_RATE} and t() == 1)
        end
        "#
    ));
    // A callback sees the ticks completed before it, so the first tick
    // to see one second is the one after `TICK_RATE` completed ticks.
    for _ in 0..=TICK_RATE {
        runtime.tick(NO_INPUT).unwrap();
    }
    let mut expected = vec!["true\tfalse"; TICK_RATE as usize];
    expected.push("true\ttrue");
    assert_eq!(texts(runtime.take_output()), expected);
}

#[test]
fn failed_tick_does_not_advance_time() {
    let mut runtime = started(
        r#"
        local failed = false
        function tick()
            log.info(frame())
            if frame() == 1 and not failed then failed = true error("x") end
        end
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.tick(NO_INPUT).unwrap_err().phase, Phase::Tick);
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["0", "1", "1"]);
}

#[test]
fn failed_draw_does_not_advance_time() {
    let mut runtime = started(
        r#"
        local failed = false
        function tick() log.info(frame()) end
        function draw()
            if frame() == 1 and not failed then failed = true error("x") end
        end
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.tick(NO_INPUT).unwrap_err().phase, Phase::Draw);
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["0", "1", "1"]);
}
