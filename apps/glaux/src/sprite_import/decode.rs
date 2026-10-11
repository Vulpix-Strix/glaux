//! PNG decoding for the sprite importer.

use std::io::BufRead;

use anyhow::{Context as _, bail};
use glaux_machine::palette::Rgb;
use glaux_machine::sprite::{OBJ_SHEET_MAX_HEIGHT, OBJ_SHEET_MAX_WIDTH};
use png::{BitDepth, ColorType, Transformations};

use super::palette_fit::{FitError, Sheet};

/// A decoded image as RGBA8, before any pixel is classified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RgbaImage {
    pub(crate) width: usize,
    pub(crate) height: usize,
    /// `width * height` pixels, four bytes each, row by row.
    pub(crate) rgba: Vec<u8>,
}

impl RgbaImage {
    /// The image as sheet pixels, with opaque pixels of color `key`
    /// transparent too.
    pub(crate) fn sheet(&self, key: Option<Rgb>) -> Result<Sheet, FitError> {
        Sheet::from_rgba(self.width, self.height, &self.rgba, key)
    }

    /// The color of the top-left pixel, when it is fully opaque: the usual
    /// background of sheets drawn without transparency.
    pub(crate) fn top_left(&self) -> Option<Rgb> {
        match self.rgba.get(..4)? {
            &[r, g, b, 255] => Some(Rgb { r, g, b }),
            _ => None,
        }
    }
}

/// Decodes any static PNG to RGBA8. Images larger than the object-tile
/// grid are rejected from their header, before their pixels are decoded.
pub(crate) fn decode_png(reader: impl BufRead + std::io::Seek) -> anyhow::Result<RgbaImage> {
    let mut decoder = png::Decoder::new(reader);
    decoder.set_transformations(
        Transformations::EXPAND | Transformations::STRIP_16 | Transformations::ALPHA,
    );
    let mut reader = decoder.read_info()?;
    if reader.info().animation_control.is_some() {
        bail!("animated PNG is not supported; expected a static PNG");
    }
    let width =
        usize::try_from(reader.info().width).context("PNG width does not fit this platform")?;
    let height =
        usize::try_from(reader.info().height).context("PNG height does not fit this platform")?;
    if width > OBJ_SHEET_MAX_WIDTH || height > OBJ_SHEET_MAX_HEIGHT {
        return Err(FitError::TooLarge { width, height }.into());
    }
    let size = reader
        .output_buffer_size()
        .context("decoded PNG is too large for this platform")?;
    let mut decoded = vec![0; size];
    let info = reader.next_frame(&mut decoded)?;
    let decoded = &decoded[..info.buffer_size()];

    let rgba: Vec<u8> = match (info.color_type, info.bit_depth) {
        (ColorType::Rgba, BitDepth::Eight) => decoded.to_vec(),
        (ColorType::GrayscaleAlpha, BitDepth::Eight) => decoded
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[gray, alpha]| [gray, gray, gray, alpha])
            .collect(),
        (color_type, depth) => {
            bail!("unexpected decoded PNG layout {color_type:?} at {depth:?} bits")
        }
    };
    Ok(RgbaImage {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests;
