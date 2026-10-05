//! Raylib-based host for Glaux.

mod crt;
mod keyboard;

use std::time::Duration;

use crt::CrtPass;
use glaux_input::InputEvent;
use glaux_video::{HEIGHT, IndexedFrame, WIDTH};
use keyboard::{EditKeys, KeyState, TextKeys};
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
    rl: RaylibHandle,
    thread: RaylibThread,
}

impl RaylibHost {
    /// Opens a resizable window whose initial size is the logical resolution
    /// multiplied by `scale` (clamped to at least 1), placed according to
    /// `placement` and presenting frames as `presentation`.
    pub fn new(
        title: &str,
        scale: i32,
        placement: WindowPlacement,
        presentation: Presentation,
    ) -> Result<Self, HostError> {
        let scale = scale.max(1);
        let (width, height) = (WIDTH as i32, HEIGHT as i32);
        let (window_width, window_height) = (width * scale, height * scale);

        let (mut rl, thread) = init()
            .size(window_width, window_height)
            .title(title)
            .resizable()
            .vsync()
            .build();
        rl.set_window_min_size(width, height);

        tracing::info!(
            width = window_width,
            height = window_height,
            "window created"
        );
        tracing::debug!(
            logical_width = width,
            logical_height = height,
            scale,
            raylib_version = %format_args!(
                "{}.{}.{}",
                ffi::RAYLIB_VERSION_MAJOR,
                ffi::RAYLIB_VERSION_MINOR,
                ffi::RAYLIB_VERSION_PATCH
            ),
            "host configuration"
        );
        place_window(&mut rl, placement, window_width, window_height);

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
            rl,
            thread,
        })
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

    /// Returns the input received since the previous call.
    ///
    /// Raylib polls the platform when a frame ends, so calling this right
    /// after [`present`](Self::present) yields the input for that frame.
    /// Typed characters and editing keys come from separate raylib queues,
    /// so their relative order within one frame is lost; text is reported
    /// before editing keys. Held keys auto-repeat, except Enter.
    pub fn poll_input(&mut self) -> Vec<InputEvent> {
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

        let shift = rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT)
            || rl.is_key_down(KeyboardKey::KEY_RIGHT_SHIFT);
        self.edit_keys.events(
            |key| KeyState {
                pressed: rl.is_key_pressed(key),
                down: rl.is_key_down(key),
            },
            shift,
            elapsed,
            &mut events,
        );
        events
    }
}

/// Applies `placement` to a freshly opened window.
fn place_window(
    rl: &mut RaylibHandle,
    placement: WindowPlacement,
    window_width: i32,
    window_height: i32,
) {
    if placement == WindowPlacement::PlatformDefault {
        tracing::debug!("initial window placement left to the platform");
        return;
    }

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
    fn fit_degenerate_window_is_empty() {
        assert_eq!(rect(fit_4_3(0, 0)), (0.0, 0.0, 0.0, 0.0));
    }
}
