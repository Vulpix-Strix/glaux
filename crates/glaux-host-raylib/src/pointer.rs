//! Translation of window-space mouse state into logical pointer events.
//!
//! Window coordinates are mapped through the same layout the frame is
//! presented with: the 4:3 rectangle from [`fit_4_3`](super::fit_4_3) and,
//! for the CRT presentation, the shader's barrel distortion. The shader's
//! `warp` maps a window position to the logical texel shown there, so the
//! pointer applies it forward and never needs an inverse.
//!
//! Capture follows the contract documented on
//! [`InputEvent::PointerDown`]: presses outside the viewport are swallowed
//! with their release, and a reported press clamps the pointer to the
//! viewport until its release is reported.

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_video::{HEIGHT, WIDTH};
use raylib::prelude::Rectangle;

use super::fit_4_3;

/// The CRT barrel distortion, as `CURVATURE` in `shaders/crt.fs`.
///
/// Pointer hit-testing under the CRT presentation depends on this matching
/// the shader exactly; change both together.
pub(crate) const CRT_CURVATURE: (f32, f32) = (0.025, 0.035);

/// Maps window positions to logical pixels for one window size and
/// presentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Mapping {
    dest: Rectangle,
    curved: bool,
}

impl Mapping {
    /// The mapping for a `window`-sized client area, or `None` when the
    /// window is too small to show any of the frame, as when minimized.
    pub(crate) fn new(window: (i32, i32), curved: bool) -> Option<Self> {
        let dest = fit_4_3(window.0, window.1);
        (dest.width > 0.0 && dest.height > 0.0).then_some(Self { dest, curved })
    }

    /// Where `pos` lands in the logical frame, in frame-relative units:
    /// `0.0..1.0` on each axis is inside it.
    fn source_uv(self, (x, y): (f32, f32)) -> (f32, f32) {
        let u = (x - self.dest.x) / self.dest.width;
        let v = (y - self.dest.y) / self.dest.height;
        if self.curved { warp(u, v) } else { (u, v) }
    }

    /// The logical pixel shown at window position `pos`, or `None` over the
    /// bars around the frame or the black outside the CRT tube.
    pub(crate) fn locate(self, pos: (f32, f32)) -> Option<Point> {
        let (u, v) = self.source_uv(pos);
        ((0.0..1.0).contains(&u) && (0.0..1.0).contains(&v)).then(|| pixel(u, v))
    }

    /// The logical pixel nearest to window position `pos`, clamping
    /// positions outside the frame to its edge.
    pub(crate) fn clamp(self, pos: (f32, f32)) -> Point {
        let (u, v) = self.source_uv(pos);
        // NaN only arises from non-finite input positions.
        let fix = |t: f32| if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        pixel(fix(u), fix(v))
    }
}

/// The pixel containing frame-relative `(u, v)`, both in `0.0..=1.0`.
fn pixel(u: f32, v: f32) -> Point {
    let axis = |t: f32, size: usize| ((t * size as f32) as u16).min(size as u16 - 1);
    Point::new(axis(u, WIDTH), axis(v, HEIGHT))
}

/// The shader's `warp`: the frame-relative position whose texel the CRT
/// shows at frame-relative window position `(u, v)`. It is odd in each
/// axis, so it commutes with the vertical flip of the render target.
fn warp(u: f32, v: f32) -> (f32, f32) {
    let (cx, cy) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
    let x = cx * (1.0 + cy * cy * CRT_CURVATURE.0);
    let y = cy * (1.0 + cx * cx * CRT_CURVATURE.1);
    (x * 0.5 + 0.5, y * 0.5 + 0.5)
}

/// One pointer button's state for the current host frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ButtonFrame {
    /// The button went down this frame.
    pub(crate) pressed: bool,
    /// The button went up this frame. A click within one frame is both
    /// pressed and released.
    pub(crate) released: bool,
    /// The button is held now.
    pub(crate) down: bool,
}

/// The mouse as the host sees it in one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PointerFrame {
    /// Pointer position in window coordinates. Outside the client area it
    /// is only meaningful while the platform keeps tracking a held button.
    pub(crate) position: (f32, f32),
    /// Whether the pointer is over the window's client area.
    pub(crate) on_window: bool,
    /// The window's client-area size.
    pub(crate) window: (i32, i32),
    /// Whether frames are presented through the CRT shader.
    pub(crate) curved: bool,
    /// Each button of [`PointerButton::ALL`], in that order.
    pub(crate) buttons: [ButtonFrame; 3],
    /// Wheel movement this frame, in notches; positive is away from the
    /// user.
    pub(crate) wheel: f32,
}

/// Turns per-frame mouse state into pointer events.
#[derive(Debug, Default)]
pub(crate) struct PointerTracker {
    enabled: bool,
    /// The last position reported, or `None` before anything was reported
    /// since the tracker was enabled.
    reported: Option<Option<Point>>,
    /// Buttons whose press was reported and whose release is still due.
    captured: [bool; 3],
    /// Held buttons whose press was not reported: pressed outside the
    /// viewport or held across a reset. They are ignored until released.
    ignored: [bool; 3],
    /// The last captured position, where a release lands when the window
    /// shows no frame.
    grip: Point,
    /// Fractional wheel movement not reported yet.
    wheel: f32,
}

impl PointerTracker {
    /// Appends this frame's pointer events to `out` when `enabled`.
    ///
    /// Enabling or disabling the tracker resets it: captures end without a
    /// release, buttons held at that moment are ignored until released, and
    /// the first enabled frame reports the current position. While
    /// disabled, nothing is reported.
    pub(crate) fn events(&mut self, frame: PointerFrame, enabled: bool, out: &mut Vec<InputEvent>) {
        if enabled != self.enabled {
            self.reset(frame, enabled);
        }
        if !enabled {
            for (ignored, button) in self.ignored.iter_mut().zip(frame.buttons) {
                *ignored = button.down && !button.released;
            }
            return;
        }

        let mapping = Mapping::new(frame.window, frame.curved);
        let located = mapping
            .filter(|_| frame.on_window)
            .and_then(|m| m.locate(frame.position));
        let clamped = mapping.map(|m| m.clamp(frame.position));

        for (i, button) in PointerButton::ALL.into_iter().enumerate() {
            let state = frame.buttons[i];
            if !state.pressed || self.captured[i] || self.ignored[i] {
                continue;
            }
            match located {
                Some(at) => {
                    self.captured[i] = true;
                    self.grip = at;
                    out.push(InputEvent::PointerDown { button, at });
                }
                None => self.ignored[i] = true,
            }
        }

        self.report_position(located, clamped, out);
        if located.is_some() || self.is_captured() {
            self.wheel += frame.wheel;
            let notches = self.wheel.trunc();
            if notches != 0.0 {
                self.wheel -= notches;
                let delta = notches.clamp(i8::MIN.into(), i8::MAX.into()) as i8;
                out.push(InputEvent::Wheel { delta });
            }
        } else {
            self.wheel = 0.0;
        }

        let mut ended = false;
        for (i, button) in PointerButton::ALL.into_iter().enumerate() {
            let state = frame.buttons[i];
            if !(state.released || !state.down) {
                continue;
            }
            self.ignored[i] = false;
            if std::mem::take(&mut self.captured[i]) {
                ended = true;
                let at = clamped.unwrap_or(self.grip);
                out.push(InputEvent::PointerUp { button, at });
            }
        }
        // A capture released outside the viewport ends with the pointer
        // outside it.
        if ended {
            self.report_position(located, clamped, out);
        }
    }

    fn is_captured(&self) -> bool {
        self.captured.contains(&true)
    }

    /// Reports the pointer position if it changed: clamped while a button
    /// is captured, the located pixel otherwise.
    fn report_position(
        &mut self,
        located: Option<Point>,
        clamped: Option<Point>,
        out: &mut Vec<InputEvent>,
    ) {
        let position = if self.is_captured() {
            let at = clamped.unwrap_or(self.grip);
            self.grip = at;
            Some(at)
        } else {
            located
        };
        if self.reported != Some(position) {
            self.reported = Some(position);
            out.push(InputEvent::PointerMove(position));
        }
    }

    fn reset(&mut self, frame: PointerFrame, enabled: bool) {
        *self = Self {
            enabled,
            ignored: frame.buttons.map(|b| b.down || b.pressed),
            ..Self::default()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const SHADER: &str = include_str!("shaders/crt.fs");

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
}
