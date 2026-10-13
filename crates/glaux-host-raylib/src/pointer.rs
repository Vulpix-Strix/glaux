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
    /// Horizontal wheel movement this frame, in notches; positive is
    /// toward the left.
    pub(crate) wheel_x: f32,
    /// Whether Shift is held, which turns the vertical wheel sideways and
    /// makes presses extend the selection.
    pub(crate) shift: bool,
    /// Whether the window has input focus. Losing it cancels captures,
    /// since the release may never reach the window.
    pub(crate) focused: bool,
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
    /// Fractional horizontal wheel movement not reported yet.
    wheel_x: f32,
}

impl PointerTracker {
    /// Appends this frame's pointer events to `out` when `enabled`.
    ///
    /// Enabling or disabling the tracker resets it: captures end without a
    /// release, buttons held at that moment are ignored until released, and
    /// the first enabled frame reports the current position. While
    /// disabled, nothing is reported.
    ///
    /// A frame without focus ends every capture with one
    /// [`InputEvent::PointerCancel`] instead of releases; buttons held then
    /// are ignored until released.
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
        if !frame.focused && self.is_captured() {
            self.captured = [false; 3];
            self.ignored = frame.buttons.map(|b| b.down || b.pressed);
            out.push(InputEvent::PointerCancel);
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
                    out.push(InputEvent::PointerDown {
                        button,
                        at,
                        select: frame.shift,
                    });
                }
                None => self.ignored[i] = true,
            }
        }

        self.report_position(located, clamped, out);
        if located.is_some() || self.is_captured() {
            // Shift turns the wheel sideways, the convention on desktops
            // whose mice have no horizontal wheel.
            let (vertical, horizontal) = if frame.shift {
                (0.0, frame.wheel_x + frame.wheel)
            } else {
                (frame.wheel, frame.wheel_x)
            };
            if let Some(delta) = take_notches(&mut self.wheel, vertical) {
                out.push(InputEvent::Wheel { delta });
            }
            if let Some(delta) = take_notches(&mut self.wheel_x, horizontal) {
                out.push(InputEvent::WheelHorizontal { delta });
            }
        } else {
            self.wheel = 0.0;
            self.wheel_x = 0.0;
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

    /// The logical pixel last reported as the pointer position: clamped to
    /// the frame while a button is captured, and `None` while disabled,
    /// before the first report, or off the frame.
    pub(crate) fn position(&self) -> Option<Point> {
        self.reported.flatten()
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

/// Adds `movement` to the fractional `pending` wheel movement and takes the
/// whole notches out of it, if any.
fn take_notches(pending: &mut f32, movement: f32) -> Option<i8> {
    *pending += movement;
    let notches = pending.trunc();
    if notches == 0.0 {
        return None;
    }
    *pending -= notches;
    Some(notches.clamp(i8::MIN.into(), i8::MAX.into()) as i8)
}

#[cfg(test)]
mod tests;
