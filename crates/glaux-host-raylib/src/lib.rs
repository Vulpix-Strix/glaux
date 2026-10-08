//! Raylib-based host for Glaux.

mod audio;
mod crt;
mod keyboard;
mod pointer;

use std::time::Duration;

pub use audio::{AudioDevice, AudioError, AudioOutput};
use crt::CrtPass;
use glaux_input::{InputEvent, InputMode};
use glaux_video::{HEIGHT, IndexedFrame, WIDTH};
use keyboard::{EditKeys, GameKeys, KeyState, Modifiers, TextKeys, retain_for_mode};
use pointer::{ButtonFrame, PointerFrame, PointerTracker};
use raylib::core::error::{LoadTextureError, UpdateTextureError};
use raylib::core::input::key_from_i32;
use raylib::prelude::*;

/// Bytes in the host-side RGBA copy of a frame.
const RGBA_LEN: usize = WIDTH * HEIGHT * 4;

/// Errors raised while creating or driving the raylib window.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("failed to create the {WIDTH}x{HEIGHT} framebuffer texture")]
    CreateTexture(#[source] LoadTextureError),
    #[error("failed to upload the frame to the framebuffer texture")]
    UploadFrame(#[source] UpdateTextureError),
}

/// Where the window is placed when it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowPlacement {
    /// Center the window on the monitor it opens on.
    CenterOnMonitor,
    /// Leave the position to the platform, e.g. a Wayland compositor, which
    /// does not let clients position their windows.
    PlatformDefault,
    /// Put the window's client area at (`x`, `y`) in the global desktop space.
    At { x: i32, y: i32 },
}

/// A monitor's rectangle in the global desktop space, in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl MonitorArea {
    /// The monitor at (`x`, `y`) with the given size, or `None` when the
    /// platform reports no usable geometry for it.
    fn new(x: i32, y: i32, width: i32, height: i32) -> Option<Self> {
        (width > 0 && height > 0).then_some(Self {
            x,
            y,
            width,
            height,
        })
    }
}

/// How frames are drawn to the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presentation {
    /// Sharp pixels scaled with nearest-neighbor filtering.
    Clean,
    /// A simulated CRT: curvature, scanlines, phosphor softness and an RGB
    /// shadow mask. Falls back to [`Clean`](Self::Clean) if the GPU cannot
    /// set it up.
    Crt,
}

/// A desktop window that presents [`IndexedFrame`]s.
///
/// The frame is drawn at the largest 4:3 size that fits the window, centered
/// with black bars, in the requested [`Presentation`].
pub struct RaylibHost {
    // GPU resources are declared before `rl` so they are unloaded before the
    // window (and its GL context) is closed.
    /// `None` when presenting [`Presentation::Clean`].
    crt: Option<CrtPass>,
    texture: Texture2D,
    rgba: Vec<u8>,
    edit_keys: EditKeys,
    text_keys: TextKeys,
    game_keys: GameKeys,
    pointer: PointerTracker,
    /// Whether the system mouse cursor is shown over the window.
    pointer_visible: bool,
    rl: RaylibHandle,
    thread: RaylibThread,
}

impl RaylibHost {
    /// Opens a resizable `width`×`height` window presenting frames as
    /// `presentation`.
    ///
    /// The size is raised to at least the logical resolution, which is also
    /// the window's minimum size. Where the window appears is up to the
    /// platform until [`place`](Self::place) is called.
    pub fn new(
        title: &str,
        (width, height): (i32, i32),
        presentation: Presentation,
    ) -> Result<Self, HostError> {
        let (window_width, window_height) = (width.max(WIDTH as i32), height.max(HEIGHT as i32));
        let (width, height) = (WIDTH as i32, HEIGHT as i32);

        let (mut rl, thread) = init()
            .size(window_width, window_height)
            .title(title)
            .resizable()
            .vsync()
            .build();
        rl.set_window_min_size(width, height);
        // Esc is Glaux's break key; raylib would otherwise close the window.
        rl.set_exit_key(None);
        // The cursor is shown only where the system asks for it.
        rl.hide_cursor();

        tracing::info!(
            width = window_width,
            height = window_height,
            "window created"
        );
        tracing::debug!(
            logical_width = width,
            logical_height = height,
            raylib_version = %format_args!(
                "{}.{}.{}",
                ffi::RAYLIB_VERSION_MAJOR,
                ffi::RAYLIB_VERSION_MINOR,
                ffi::RAYLIB_VERSION_PATCH
            ),
            "host configuration"
        );
        // A generated image defaults to uncompressed R8G8B8A8, which is the
        // layout `resolve_rgba` produces.
        let image = Image::gen_image_color(width, height, Color::BLACK);
        let texture = rl
            .load_texture_from_image(&thread, &image)
            .map_err(HostError::CreateTexture)?;
        texture.set_texture_filter(&thread, TextureFilter::TEXTURE_FILTER_POINT);

        let crt = match presentation {
            Presentation::Clean => None,
            Presentation::Crt => match CrtPass::load(&mut rl, &thread) {
                Ok(crt) => Some(crt),
                Err(err) => {
                    tracing::warn!(error = %err, "CRT presentation unavailable, presenting clean");
                    None
                }
            },
        };
        tracing::info!(crt = crt.is_some(), "presentation configured");

        Ok(Self {
            crt,
            texture,
            rgba: vec![0; RGBA_LEN],
            edit_keys: EditKeys::default(),
            text_keys: TextKeys::default(),
            game_keys: GameKeys::default(),
            pointer: PointerTracker::default(),
            pointer_visible: false,
            rl,
            thread,
        })
    }

    /// Moves the window according to `placement`.
    pub fn place(&mut self, placement: WindowPlacement) {
        match placement {
            WindowPlacement::PlatformDefault => {
                tracing::debug!("window placement left to the platform");
            }
            WindowPlacement::At { x, y } => {
                self.rl.set_window_position(x, y);
                tracing::debug!(x, y, "window placed at a saved position");
            }
            WindowPlacement::CenterOnMonitor => {
                let (width, height) = self.window_size();
                center_window(&mut self.rl, width, height);
            }
        }
    }

    /// The connected monitors that report a usable geometry.
    pub fn monitors(&self) -> Vec<MonitorArea> {
        (0..get_monitor_count())
            .filter_map(|monitor| {
                let origin = get_monitor_position(monitor);
                MonitorArea::new(
                    origin.x as i32,
                    origin.y as i32,
                    get_monitor_width(monitor),
                    get_monitor_height(monitor),
                )
            })
            .collect()
    }

    /// The window's current client-area size in screen coordinates.
    pub fn window_size(&self) -> (i32, i32) {
        (self.rl.get_screen_width(), self.rl.get_screen_height())
    }

    /// The window's client-area position in the global desktop space.
    ///
    /// Only meaningful where the platform reports window positions; Wayland
    /// does not, and the value there never changes.
    pub fn window_position(&self) -> (i32, i32) {
        let position = self.rl.get_window_position();
        (position.x as i32, position.y as i32)
    }

    /// Whether the window is maximized.
    pub fn is_maximized(&self) -> bool {
        self.rl.is_window_maximized()
    }

    /// Whether the window is minimized.
    pub fn is_minimized(&self) -> bool {
        self.rl.is_window_minimized()
    }

    /// Maximizes the window.
    pub fn maximize(&mut self) {
        self.rl.maximize_window();
    }

    /// Restores the window if it is minimized and asks the platform to give
    /// it input focus. The platform may refuse.
    pub fn focus(&mut self) {
        if self.rl.is_window_minimized() {
            self.rl.restore_window();
        }
        self.rl.set_window_focused();
    }

    /// Shows or hides the system mouse cursor over the window.
    pub fn set_pointer_visible(&mut self, visible: bool) {
        if visible == self.pointer_visible {
            return;
        }
        self.pointer_visible = visible;
        if visible {
            self.rl.show_cursor();
        } else {
            self.rl.hide_cursor();
        }
        tracing::debug!(visible, "pointer visibility changed");
    }

    /// Returns `true` once the user has asked to close the window.
    pub fn should_close(&self) -> bool {
        self.rl.window_should_close()
    }

    /// Resolves `frame` to RGB and draws it to the window.
    pub fn present(&mut self, frame: &IndexedFrame) -> Result<(), HostError> {
        resolve_rgba(frame, &mut self.rgba);
        self.texture
            .update_texture(&self.rgba)
            .map_err(HostError::UploadFrame)?;

        let dest = fit_4_3(self.rl.get_screen_width(), self.rl.get_screen_height());
        if let Some(crt) = &mut self.crt {
            crt.render(&mut self.rl, &self.thread, &self.texture);
        }

        let mut d = self.rl.begin_drawing(&self.thread);
        d.clear_background(Color::BLACK);
        match &self.crt {
            Some(crt) => crt.draw(&mut d, dest),
            None => d.draw_texture_pro(
                &self.texture,
                Rectangle::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32),
                dest,
                Vector2::zero(),
                0.0,
                Color::WHITE,
            ),
        }
        Ok(())
    }

    /// Returns the input received since the previous call that `mode`
    /// consumes, plus any [`InputEvent::Break`].
    ///
    /// Raylib polls the platform when a frame ends, so calling this right
    /// after [`present`](Self::present) yields the input for that frame.
    /// Typed characters and editing keys come from separate raylib queues,
    /// so their relative order within one frame is lost; text is reported
    /// before editing keys. Held editing keys auto-repeat, except Enter.
    ///
    /// Every translator runs whatever the mode, so their held-key state
    /// stays current: a key already down when the mode changes does not
    /// report a fresh press in the new mode. Pointer events are produced
    /// only in [`InputMode::Workspace`], mapped to logical pixels through
    /// the current window size and presentation.
    pub fn poll_input(&mut self, mode: InputMode) -> Vec<InputEvent> {
        let chars: Vec<char> = std::iter::from_fn(|| self.rl.get_char_pressed()).collect();
        // Codes raylib-rs has no `KeyboardKey` for are skipped rather than
        // ending the drain, so they don't hide keys queued after them.
        let pressed: Vec<KeyboardKey> = std::iter::from_fn(|| self.rl.get_key_pressed_number())
            .filter_map(|code| i32::try_from(code).ok().and_then(key_from_i32))
            .collect();

        let rl = &self.rl;
        let elapsed = Duration::try_from_secs_f32(rl.get_frame_time()).unwrap_or_default();
        let mut events = Vec::new();
        self.text_keys.events(
            &chars,
            &pressed,
            |key| rl.is_key_down(key),
            elapsed,
            &mut events,
        );

        let modifiers = Modifiers {
            shift: rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT)
                || rl.is_key_down(KeyboardKey::KEY_RIGHT_SHIFT),
            ctrl: rl.is_key_down(KeyboardKey::KEY_LEFT_CONTROL)
                || rl.is_key_down(KeyboardKey::KEY_RIGHT_CONTROL),
        };
        self.edit_keys.events(
            |key| KeyState {
                pressed: rl.is_key_pressed(key),
                down: rl.is_key_down(key),
            },
            modifiers,
            elapsed,
            &mut events,
        );
        self.game_keys
            .events(&pressed, |key| rl.is_key_down(key), &mut events);
        retain_for_mode(mode, &mut events);
        self.pointer.events(
            self.pointer_frame(),
            mode == InputMode::Workspace,
            &mut events,
        );
        events
    }

    /// The mouse state raylib collected for the frame just presented.
    fn pointer_frame(&self) -> PointerFrame {
        let rl = &self.rl;
        let position = rl.get_mouse_position();
        let button = |button| ButtonFrame {
            pressed: rl.is_mouse_button_pressed(button),
            released: rl.is_mouse_button_released(button),
            down: rl.is_mouse_button_down(button),
        };
        PointerFrame {
            position: (position.x, position.y),
            on_window: rl.is_cursor_on_screen(),
            window: (rl.get_screen_width(), rl.get_screen_height()),
            curved: self.crt.is_some(),
            // In the order of `PointerButton::ALL`.
            buttons: [
                button(MouseButton::MOUSE_BUTTON_LEFT),
                button(MouseButton::MOUSE_BUTTON_RIGHT),
                button(MouseButton::MOUSE_BUTTON_MIDDLE),
            ],
            wheel: rl.get_mouse_wheel_move(),
        }
    }
}

/// Centers a `window_width`×`window_height` window on its current monitor.
fn center_window(rl: &mut RaylibHandle, window_width: i32, window_height: i32) {
    let monitor = get_current_monitor();
    let origin = get_monitor_position(monitor);
    let (monitor_width, monitor_height) = (get_monitor_width(monitor), get_monitor_height(monitor));
    if monitor_width <= 0 || monitor_height <= 0 {
        tracing::debug!(
            monitor,
            "monitor geometry unavailable, keeping platform placement"
        );
        return;
    }

    let (x, y) = center_on_monitor(
        origin.x as i32,
        origin.y as i32,
        monitor_width,
        monitor_height,
        window_width,
        window_height,
    );
    rl.set_window_position(x, y);
    tracing::debug!(
        monitor,
        monitor_x = origin.x as i32,
        monitor_y = origin.y as i32,
        monitor_width,
        monitor_height,
        x,
        y,
        "window centered on monitor"
    );
}

/// Top-left position that centers a `window_width`×`window_height` window
/// on a monitor at (`monitor_x`, `monitor_y`) in the global desktop space.
fn center_on_monitor(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: i32,
    monitor_height: i32,
    window_width: i32,
    window_height: i32,
) -> (i32, i32) {
    (
        monitor_x + (monitor_width - window_width) / 2,
        monitor_y + (monitor_height - window_height) / 2,
    )
}

/// Writes each pixel of `frame` as opaque RGBA bytes into `out`.
///
/// `out` must be exactly `WIDTH * HEIGHT * 4` bytes-long.
fn resolve_rgba(frame: &IndexedFrame, out: &mut [u8]) {
    assert_eq!(out.len(), RGBA_LEN, "RGBA buffer has the wrong length");
    let palette = frame.palette();
    let (texels, _) = out.as_chunks_mut::<4>();
    for (&index, texel) in frame.pixels().iter().zip(texels) {
        let color = palette[usize::from(index)];
        *texel = [color.r, color.g, color.b, 255];
    }
}

/// Returns the largest 4:3 rectangle that fits in a `width`×`height` window,
/// centered on both axes.
fn fit_4_3(width: i32, height: i32) -> Rectangle {
    let (width, height) = (width.max(0) as f32, height.max(0) as f32);
    let scale = (width / WIDTH as f32).min(height / HEIGHT as f32);
    let (w, h) = (WIDTH as f32 * scale, HEIGHT as f32 * scale);
    Rectangle::new((width - w) / 2.0, (height - h) / 2.0, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaux_machine::framebuffer::Framebuffer;
    use glaux_machine::palette::{DEBUG_MAGENTA, Palette, ZUGHY_32};
    use pretty_assertions::assert_eq;

    fn rect(r: Rectangle) -> (f32, f32, f32, f32) {
        (r.x, r.y, r.width, r.height)
    }

    #[test]
    fn resolves_indices_to_opaque_rgba() {
        let mut fb = Framebuffer::new();
        fb.clear(3);
        fb.set_pixel(0, 0, 31);
        fb.set_pixel(1, 0, 200);
        let palette = Palette::provisional();
        let mut out = vec![0; RGBA_LEN];

        resolve_rgba(&IndexedFrame::new(&fb, &palette), &mut out);

        let c31 = ZUGHY_32[31];
        let c3 = ZUGHY_32[3];
        let m = DEBUG_MAGENTA;
        assert_eq!(out[0..4], [c31.r, c31.g, c31.b, 255]);
        assert_eq!(out[4..8], [m.r, m.g, m.b, 255]);
        assert_eq!(out[8..12], [c3.r, c3.g, c3.b, 255]);
        assert_eq!(out[RGBA_LEN - 4..], [c3.r, c3.g, c3.b, 255]);
    }

    #[test]
    fn fit_exact_4_3_window_fills_it() {
        assert_eq!(rect(fit_4_3(960, 720)), (0.0, 0.0, 960.0, 720.0));
    }

    #[test]
    fn fit_wide_window_is_pillarboxed_and_centered() {
        assert_eq!(rect(fit_4_3(1280, 720)), (160.0, 0.0, 960.0, 720.0));
    }

    #[test]
    fn fit_tall_window_is_letterboxed_and_centered() {
        assert_eq!(rect(fit_4_3(640, 800)), (0.0, 160.0, 640.0, 480.0));
    }

    #[test]
    fn centers_on_primary_monitor() {
        assert_eq!(center_on_monitor(0, 0, 1920, 1080, 960, 720), (480, 180));
    }

    #[test]
    fn centers_on_monitor_to_the_right() {
        assert_eq!(
            center_on_monitor(1920, 0, 2560, 1440, 960, 720),
            (2720, 360)
        );
    }

    #[test]
    fn centers_on_monitor_at_negative_offset() {
        assert_eq!(
            center_on_monitor(-1280, -200, 1280, 1024, 960, 720),
            (-1120, -48)
        );
    }

    #[test]
    fn window_larger_than_monitor_overhangs_evenly() {
        assert_eq!(center_on_monitor(0, 0, 800, 600, 960, 720), (-80, -60));
    }

    #[test]
    fn monitors_without_geometry_are_skipped() {
        assert_eq!(
            MonitorArea::new(-1920, 0, 1920, 1080),
            Some(MonitorArea {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080
            })
        );
        assert_eq!(MonitorArea::new(0, 0, 0, 1080), None);
        assert_eq!(MonitorArea::new(0, 0, 1920, -1), None);
    }

    #[test]
    fn fit_degenerate_window_is_empty() {
        assert_eq!(rect(fit_4_3(0, 0)), (0.0, 0.0, 0.0, 0.0));
    }
}
