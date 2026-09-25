//! Raylib-based host for Glaux.

use glaux_video::{HEIGHT, IndexedFrame, WIDTH};
use raylib::core::error::{LoadTextureError, UpdateTextureError};
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

/// A desktop window that presents [`IndexedFrame`]s.
///
/// The frame is drawn at the largest 4:3 size that fits the window, centered
/// with black bars, and scaled with nearest-neighbor filtering.
pub struct RaylibHost {
    // Declared before `rl` so the texture is unloaded before the window
    // (and its GL context) is closed.
    texture: Texture2D,
    rgba: Vec<u8>,
    rl: RaylibHandle,
    thread: RaylibThread,
}

impl RaylibHost {
    /// Opens a resizable window whose initial size is the logical resolution
    /// multiplied by `scale` (clamped to at least 1).
    pub fn new(title: &str, scale: i32) -> Result<Self, HostError> {
        let scale = scale.max(1);
        let (width, height) = (WIDTH as i32, HEIGHT as i32);

        let (mut rl, thread) = init()
            .size(width * scale, height * scale)
            .title(title)
            .resizable()
            .vsync()
            .build();
        rl.set_window_min_size(width, height);

        // A generated image defaults to uncompressed R8G8B8A8, which is the
        // layout `resolve_rgba` produces.
        let image = Image::gen_image_color(width, height, Color::BLACK);
        let texture = rl
            .load_texture_from_image(&thread, &image)
            .map_err(HostError::CreateTexture)?;
        texture.set_texture_filter(&thread, TextureFilter::TEXTURE_FILTER_POINT);

        Ok(Self {
            texture,
            rgba: vec![0; RGBA_LEN],
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
        let source = Rectangle::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32);

        let mut d = self.rl.begin_drawing(&self.thread);
        d.clear_background(Color::BLACK);
        d.draw_texture_pro(
            &self.texture,
            source,
            dest,
            Vector2::zero(),
            0.0,
            Color::WHITE,
        );
        Ok(())
    }
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
    fn fit_degenerate_window_is_empty() {
        assert_eq!(rect(fit_4_3(0, 0)), (0.0, 0.0, 0.0, 0.0));
    }
}
