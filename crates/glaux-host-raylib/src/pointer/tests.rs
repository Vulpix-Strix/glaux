use super::*;
use pretty_assertions::assert_eq;

const SHADER: &str = include_str!("../shaders/crt.fs");

fn clean(window: (i32, i32)) -> Mapping {
    Mapping::new(window, false).unwrap()
}

fn crt(window: (i32, i32)) -> Mapping {
    Mapping::new(window, true).unwrap()
}

fn pt(x: u16, y: u16) -> Option<Point> {
    Some(Point::new(x, y))
}

#[test]
fn shader_curvature_matches_the_pointer_mapping() {
    let (x, y) = CRT_CURVATURE;
    let expected = format!("const vec2 CURVATURE = vec2({x:?}, {y:?});");
    assert!(SHADER.contains(&expected), "shader lacks `{expected}`");
}

#[test]
fn clean_mapping_at_integer_scales() {
    for scale in [1, 2, 3, 4] {
        let m = clean((320 * scale, 240 * scale));
        let s = scale as f32;
        assert_eq!(m.locate((0.0, 0.0)), pt(0, 0), "×{scale}");
        assert_eq!(m.locate((10.0 * s + 0.5, 20.0 * s + 0.5)), pt(10, 20));
        assert_eq!(m.locate((320.0 * s - 0.5, 240.0 * s - 0.5)), pt(319, 239));
        assert_eq!(m.locate((320.0 * s, 0.0)), None);
        assert_eq!(m.locate((0.0, 240.0 * s)), None);
    }
}

#[test]
fn pillarbox_bars_are_outside_the_viewport() {
    // 1280×720 shows the frame at ×3 from x = 160 to 1120.
    let m = clean((1280, 720));
    assert_eq!(m.locate((159.5, 360.0)), None);
    assert_eq!(m.locate((160.5, 360.0)), pt(0, 120));
    assert_eq!(m.locate((1119.5, 360.0)), pt(319, 120));
    assert_eq!(m.locate((1120.5, 360.0)), None);
    assert_eq!(m.clamp((5.0, 360.0)), Point::new(0, 120));
    assert_eq!(m.clamp((1275.0, -40.0)), Point::new(319, 0));
}

#[test]
fn letterbox_bars_are_outside_the_viewport() {
    // 640×800 shows the frame at ×2 from y = 160 to 640.
    let m = clean((640, 800));
    assert_eq!(m.locate((320.0, 159.5)), None);
    assert_eq!(m.locate((320.0, 160.5)), pt(160, 0));
    assert_eq!(m.locate((320.0, 639.5)), pt(160, 239));
    assert_eq!(m.locate((320.0, 640.5)), None);
    assert_eq!(m.clamp((320.0, 790.0)), Point::new(160, 239));
}

#[test]
fn fractional_scales_map_proportionally() {
    // 1000×700: ×2.9167, frame from x = 33.33 to 966.67.
    let m = clean((1000, 700));
    assert_eq!(m.locate((500.0, 350.0)), pt(160, 120));
    assert_eq!(m.locate((33.0, 350.0)), None);
    assert_eq!(m.locate((34.0, 350.0)), pt(0, 120));
    assert_eq!(m.locate((966.0, 699.0)), pt(319, 239));
    assert_eq!(m.locate((967.0, 350.0)), None);
    // 1366×768: ×3.2, frame from x = 171 to 1195.
    let m = clean((1366, 768));
    assert_eq!(m.locate((171.0 + 3.2 * 100.5, 3.2 * 50.5)), pt(100, 50));
}

#[test]
fn crt_mapping_keeps_the_centre_and_axes_straight() {
    let m = crt((960, 720));
    assert_eq!(m.locate((480.0, 360.0)), pt(160, 120));
    // The distortion only bends x with y and y with x, so the middle
    // row and column are undistorted.
    assert_eq!(m.locate((1.0, 360.0)), pt(0, 120));
    assert_eq!(m.locate((480.0, 719.0)), pt(160, 239));
}

/// Window position at frame-relative `(u, v)` of `window`'s 4:3 area.
fn at(window: (i32, i32), u: f32, v: f32) -> (f32, f32) {
    let dest = fit_4_3(window.0, window.1);
    (dest.x + u * dest.width, dest.y + v * dest.height)
}

#[test]
fn crt_mapping_near_curved_edges() {
    // Expected pixels are computed from the shader's warp by hand:
    // (u, v) → (c·(1 + c'²k) + 1) / 2 per axis, times 320 or 240.
    for window in [(1000, 700), (1366, 768), (960, 720)] {
        let m = crt(window);
        let check = |u, v, expected| {
            assert_eq!(
                m.locate(at(window, u, v)),
                expected,
                "{window:?} at ({u}, {v})"
            );
        };
        // Near the top-left corner the tube bends inwards: a point
        // inside the 4:3 area shows texel (2, 1)…
        check(0.02, 0.02, pt(2, 1));
        // …while one closer to the corner is already outside the tube.
        check(0.005, 0.005, None);
        check(0.995, 0.995, None);
        // Along the top edge, a quarter of the way across.
        check(0.25, 0.003, None);
        check(0.25, 0.006, pt(78, 0));
        // Along the right edge, a quarter of the way down.
        check(0.997, 0.25, None);
        check(0.993, 0.25, pt(318, 57));
        // Next to the middle row and column the edges barely bend.
        check(0.001, 0.502, pt(0, 120));
        check(0.502, 0.999, pt(160, 239));
    }
}

#[test]
fn crt_clamps_outside_the_tube_to_the_edge() {
    let window = (1000, 700);
    let m = crt(window);
    assert_eq!(m.clamp(at(window, -0.2, -0.2)), Point::new(0, 0));
    assert_eq!(m.clamp(at(window, 1.3, 0.5)), Point::new(319, 120));
}

#[test]
fn degenerate_windows_have_no_mapping() {
    assert_eq!(Mapping::new((0, 0), false), None);
    assert_eq!(Mapping::new((0, 480), true), None);
}

// Tracker.

const WINDOW: (i32, i32) = (960, 720);
const INSIDE: (f32, f32) = (31.5, 61.5);
const INSIDE_PIXEL: Point = Point::new(10, 20);
const PRIMARY: PointerButton = PointerButton::Primary;

fn frame(position: (f32, f32), primary: ButtonFrame) -> PointerFrame {
    PointerFrame {
        position,
        on_window: (0.0..960.0).contains(&position.0) && (0.0..720.0).contains(&position.1),
        window: WINDOW,
        curved: false,
        buttons: [primary, ButtonFrame::default(), ButtonFrame::default()],
        wheel: 0.0,
        wheel_x: 0.0,
        shift: false,
        focused: true,
    }
}

const UP: ButtonFrame = ButtonFrame {
    pressed: false,
    released: false,
    down: false,
};
const PRESS: ButtonFrame = ButtonFrame {
    pressed: true,
    released: false,
    down: true,
};
const HOLD: ButtonFrame = ButtonFrame {
    pressed: false,
    released: false,
    down: true,
};
const RELEASE: ButtonFrame = ButtonFrame {
    pressed: false,
    released: true,
    down: false,
};

fn step(tracker: &mut PointerTracker, frame: PointerFrame) -> Vec<InputEvent> {
    let mut out = Vec::new();
    tracker.events(frame, true, &mut out);
    out
}

fn enabled() -> PointerTracker {
    let mut tracker = PointerTracker::default();
    step(&mut tracker, frame(INSIDE, UP));
    tracker
}

#[test]
fn first_enabled_frame_reports_the_position_and_stillness_reports_nothing() {
    let mut tracker = PointerTracker::default();
    assert_eq!(
        step(&mut tracker, frame(INSIDE, UP)),
        [InputEvent::PointerMove(Some(INSIDE_PIXEL))]
    );
    assert_eq!(step(&mut tracker, frame(INSIDE, UP)), []);
    // Moving within the same logical pixel is not a move.
    assert_eq!(step(&mut tracker, frame((32.9, 62.9), UP)), []);
    assert_eq!(
        step(&mut tracker, frame((34.0, 62.0), UP)),
        [InputEvent::PointerMove(pt(11, 20))]
    );
}

#[test]
fn position_follows_reports_and_clears_off_frame_or_disabled() {
    let mut tracker = PointerTracker::default();
    assert_eq!(tracker.position(), None);
    step(&mut tracker, frame(INSIDE, UP));
    assert_eq!(tracker.position(), Some(INSIDE_PIXEL));
    step(&mut tracker, frame((34.0, 62.0), UP));
    assert_eq!(tracker.position(), pt(11, 20));

    // Captured drags keep a clamped position outside the frame.
    step(&mut tracker, frame(INSIDE, PRESS));
    let mut outside = frame((-50.0, 2000.0), HOLD);
    outside.on_window = false;
    step(&mut tracker, outside);
    assert_eq!(tracker.position(), pt(0, 239));
    outside.buttons[0] = RELEASE;
    step(&mut tracker, outside);
    assert_eq!(tracker.position(), None);

    step(&mut tracker, frame(INSIDE, UP));
    tracker.events(frame(INSIDE, UP), false, &mut Vec::new());
    assert_eq!(tracker.position(), None);
}

#[test]
fn leaving_the_viewport_or_window_reports_no_position() {
    let mut tracker = enabled();
    let mut off = frame((500.0, 300.0), UP);
    off.on_window = false;
    assert_eq!(step(&mut tracker, off), [InputEvent::PointerMove(None)]);
    let mut letterboxed = PointerTracker::default();
    let mut wide = frame((10.0, 300.0), UP);
    wide.window = (1280, 720);
    assert_eq!(
        step(&mut letterboxed, wide),
        [InputEvent::PointerMove(None)]
    );
}

#[test]
fn click_inside_presses_and_releases_at_the_pixel() {
    let mut tracker = enabled();
    assert_eq!(
        step(&mut tracker, frame(INSIDE, PRESS)),
        [InputEvent::PointerDown {
            button: PRIMARY,
            at: INSIDE_PIXEL
        }]
    );
    assert_eq!(
        step(&mut tracker, frame(INSIDE, RELEASE)),
        [InputEvent::PointerUp {
            button: PRIMARY,
            at: INSIDE_PIXEL
        }]
    );
}

#[test]
fn a_click_within_one_frame_is_not_lost() {
    let mut tracker = enabled();
    let tap = ButtonFrame {
        pressed: true,
        released: true,
        down: false,
    };
    assert_eq!(
        step(&mut tracker, frame(INSIDE, tap)),
        [
            InputEvent::PointerDown {
                button: PRIMARY,
                at: INSIDE_PIXEL
            },
            InputEvent::PointerUp {
                button: PRIMARY,
                at: INSIDE_PIXEL
            },
        ]
    );
}

#[test]
fn presses_on_the_bars_are_swallowed_with_their_release() {
    let mut tracker = PointerTracker::default();
    let mut bar = frame((10.0, 300.0), PRESS);
    bar.window = (1280, 720);
    assert_eq!(step(&mut tracker, bar), [InputEvent::PointerMove(None)]);
    // Dragging into the frame neither presses nor captures.
    let mut inside = frame((640.0, 360.0), HOLD);
    inside.window = (1280, 720);
    assert_eq!(
        step(&mut tracker, inside),
        [InputEvent::PointerMove(pt(160, 120))]
    );
    inside.buttons[0] = RELEASE;
    assert_eq!(step(&mut tracker, inside), []);
    // The next press inside is reported.
    inside.buttons[0] = PRESS;
    assert_eq!(
        step(&mut tracker, inside),
        [InputEvent::PointerDown {
            button: PRIMARY,
            at: Point::new(160, 120)
        }]
    );
}

#[test]
fn a_drag_out_of_the_viewport_is_clamped_and_released() {
    let mut tracker = enabled();
    step(&mut tracker, frame(INSIDE, PRESS));
    let mut outside = frame((-50.0, 2000.0), HOLD);
    outside.on_window = false;
    assert_eq!(
        step(&mut tracker, outside),
        [InputEvent::PointerMove(pt(0, 239))]
    );
    outside.buttons[0] = RELEASE;
    assert_eq!(
        step(&mut tracker, outside),
        [
            InputEvent::PointerUp {
                button: PRIMARY,
                at: Point::new(0, 239)
            },
            InputEvent::PointerMove(None),
        ]
    );
}

#[test]
fn disabling_drops_captures_and_reenabling_ignores_held_buttons() {
    let mut tracker = enabled();
    step(&mut tracker, frame(INSIDE, PRESS));
    let mut out = Vec::new();
    tracker.events(frame(INSIDE, HOLD), false, &mut out);
    tracker.events(frame(INSIDE, RELEASE), false, &mut out);
    tracker.events(frame(INSIDE, PRESS), false, &mut out);
    assert_eq!(out, []);

    // Held across the switch: neither its press nor release reaches the
    // workspace, but the position is reported afresh.
    assert_eq!(
        step(&mut tracker, frame(INSIDE, HOLD)),
        [InputEvent::PointerMove(Some(INSIDE_PIXEL))]
    );
    assert_eq!(step(&mut tracker, frame(INSIDE, RELEASE)), []);
    assert_eq!(
        step(&mut tracker, frame(INSIDE, PRESS)),
        [InputEvent::PointerDown {
            button: PRIMARY,
            at: INSIDE_PIXEL
        }]
    );
}

#[test]
fn losing_focus_mid_drag_cancels_the_capture_without_a_release() {
    let mut tracker = enabled();
    step(&mut tracker, frame(INSIDE, PRESS));
    let mut unfocused = frame((-50.0, 2000.0), HOLD);
    unfocused.on_window = false;
    unfocused.focused = false;
    assert_eq!(
        step(&mut tracker, unfocused),
        [InputEvent::PointerCancel, InputEvent::PointerMove(None)],
        "no longer clamped: the pointer is simply off the frame"
    );
    unfocused.buttons[0] = RELEASE;
    assert_eq!(step(&mut tracker, unfocused), []);
    assert_eq!(
        step(&mut tracker, frame(INSIDE, UP)),
        [InputEvent::PointerMove(Some(INSIDE_PIXEL))]
    );
}

#[test]
fn a_button_held_through_focus_loss_is_ignored_until_released() {
    let mut tracker = enabled();
    step(&mut tracker, frame(INSIDE, PRESS));
    let mut unfocused = frame(INSIDE, HOLD);
    unfocused.focused = false;
    assert_eq!(step(&mut tracker, unfocused), [InputEvent::PointerCancel]);
    // Focus returns with the button still down: no press, no drag.
    assert_eq!(
        step(&mut tracker, frame((34.0, 62.0), HOLD)),
        [InputEvent::PointerMove(pt(11, 20))]
    );
    assert_eq!(step(&mut tracker, frame((34.0, 62.0), RELEASE)), []);
    assert_eq!(
        step(&mut tracker, frame((34.0, 62.0), PRESS)),
        [InputEvent::PointerDown {
            button: PRIMARY,
            at: Point::new(11, 20)
        }]
    );
}

#[test]
fn an_unfocused_window_without_captures_reports_nothing_extra() {
    let mut tracker = enabled();
    let mut unfocused = frame(INSIDE, UP);
    unfocused.focused = false;
    assert_eq!(step(&mut tracker, unfocused), []);
}

#[test]
fn wheel_reports_whole_notches_over_the_viewport() {
    let mut tracker = enabled();
    let mut f = frame(INSIDE, UP);
    f.wheel = 0.6;
    assert_eq!(step(&mut tracker, f), []);
    assert_eq!(step(&mut tracker, f), [InputEvent::Wheel { delta: 1 }]);
    f.wheel = -3.0;
    assert_eq!(step(&mut tracker, f), [InputEvent::Wheel { delta: -2 }]);

    let mut bar = frame((10.0, 300.0), UP);
    bar.window = (1280, 720);
    bar.wheel = 5.0;
    assert_eq!(step(&mut tracker, bar), [InputEvent::PointerMove(None)]);
}

#[test]
fn shift_turns_the_wheel_sideways() {
    let mut tracker = enabled();
    let mut f = frame(INSIDE, UP);
    f.shift = true;
    f.wheel = 2.0;
    assert_eq!(
        step(&mut tracker, f),
        [InputEvent::WheelHorizontal { delta: 2 }]
    );
    f.shift = false;
    assert_eq!(step(&mut tracker, f), [InputEvent::Wheel { delta: 2 }]);
}

#[test]
fn horizontal_wheels_report_whole_notches_on_their_own_axis() {
    let mut tracker = enabled();
    let mut f = frame(INSIDE, UP);
    f.wheel_x = -0.6;
    f.wheel = 0.5;
    assert_eq!(step(&mut tracker, f), []);
    assert_eq!(
        step(&mut tracker, f),
        [
            InputEvent::Wheel { delta: 1 },
            InputEvent::WheelHorizontal { delta: -1 }
        ]
    );

    let mut bar = frame((10.0, 300.0), UP);
    bar.window = (1280, 720);
    bar.wheel_x = 4.0;
    assert_eq!(step(&mut tracker, bar), [InputEvent::PointerMove(None)]);
}
