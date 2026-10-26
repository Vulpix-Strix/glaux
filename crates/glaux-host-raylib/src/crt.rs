//! Shader-based CRT presentation.
//!
//! The logical frame is drawn through the CRT shader into a render target at
//! a fixed multiple of the logical resolution, so the effect has the same
//! texture at any window size. The window then shows that target scaled with
//! bilinear filtering.

use glaux_video::{HEIGHT, WIDTH};
use raylib::core::error::LoadTextureError;
use raylib::prelude::*;

/// Render target pixels per logical pixel on each axis. Three or more keeps
/// a whole RGB mask triad inside every logical pixel.
pub(crate) const CRT_SCALE: u32 = 4;

const SHADER_SOURCE: &str = include_str!("shaders/crt.fs");

/// Uniform holding the logical frame size in texels.
const SOURCE_SIZE: &str = "sourceSize";

/// Reasons the CRT pass could not be set up.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CrtError {
    #[error("the CRT shader failed to compile or link")]
    InvalidShader,
    #[error("the CRT shader has no active `{0}` uniform")]
    MissingUniform(&'static str),
    #[error("failed to create the CRT render target")]
    RenderTarget(#[source] LoadTextureError),
    #[error("the CRT render target is incomplete")]
    IncompleteRenderTarget,
}

/// GPU resources of the CRT pass.
pub(crate) struct CrtPass {
    shader: Shader,
    target: RenderTexture2D,
}

impl CrtPass {
    /// Compiles the shader and allocates the render target.
    pub(crate) fn load(rl: &mut RaylibHandle, thread: &RaylibThread) -> Result<Self, CrtError> {
        let mut shader = rl.load_shader_from_memory(thread, None, Some(SHADER_SOURCE));
        // A failed compile leaves program id 0, which raylib reports as
        // invalid. It does not silently substitute the default shader.
        if !shader.is_shader_valid() {
            return Err(CrtError::InvalidShader);
        }
        let source_size = shader.get_shader_location(SOURCE_SIZE);
        if source_size < 0 {
            return Err(CrtError::MissingUniform(SOURCE_SIZE));
        }
        shader.set_shader_value(source_size, Vector2::new(WIDTH as f32, HEIGHT as f32));

        let (width, height) = target_size();
        let target = rl
            .load_render_texture(thread, width, height)
            .map_err(CrtError::RenderTarget)?;
        if !target.is_render_texture_valid() {
            return Err(CrtError::IncompleteRenderTarget);
        }
        target
            .texture()
            .set_texture_filter(thread, TextureFilter::TEXTURE_FILTER_BILINEAR);

        Ok(Self { shader, target })
    }

    /// Draws `frame` through the CRT shader into the render target.
    ///
    /// Must run outside `begin_drawing`. The target is shown afterwards with
    /// [`draw`](Self::draw).
    pub(crate) fn render(
        &mut self,
        rl: &mut RaylibHandle,
        thread: &RaylibThread,
        frame: &Texture2D,
    ) {
        let (width, height) = target_size();
        let mut target = rl.begin_texture_mode(thread, &mut self.target);
        target.clear_background(Color::BLACK);
        let mut shaded = target.begin_shader_mode(&mut self.shader);
        shaded.draw_texture_pro(
            frame,
            Rectangle::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32),
            Rectangle::new(0.0, 0.0, width as f32, height as f32),
            Vector2::zero(),
            0.0,
            Color::WHITE,
        );
    }

    /// Draws the render target into `dest` on the window.
    pub(crate) fn draw(&self, d: &mut RaylibDrawHandle<'_>, dest: Rectangle) {
        d.draw_texture_pro(
            self.target.texture(),
            flipped_source(),
            dest,
            Vector2::zero(),
            0.0,
            Color::WHITE,
        );
    }
}

/// Render target size, which is the logical resolution times [`CRT_SCALE`].
fn target_size() -> (u32, u32) {
    (WIDTH as u32 * CRT_SCALE, HEIGHT as u32 * CRT_SCALE)
}

/// The whole render target as a draw source. Render textures are stored
/// bottom-up, so the source height is negative to show them upright.
fn flipped_source() -> Rectangle {
    let (width, height) = target_size();
    Rectangle::new(0.0, 0.0, width as f32, -(height as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn target_source_is_flipped_vertically() {
        let source = flipped_source();
        assert_eq!(
            (source.x, source.y, source.width, source.height),
            (0.0, 0.0, 1280.0, -960.0)
        );
    }

    #[test]
    fn shader_declares_the_uniforms_the_host_sets() {
        assert!(SHADER_SOURCE.contains(&format!("uniform vec2 {SOURCE_SIZE};")));
        assert!(SHADER_SOURCE.starts_with("#version 330"));
    }
}
