//! Small System ROM images drawn with caller-chosen colors.
//!
//! Each pixel holds an ink slot instead of a palette index: slot 0 is
//! transparent and slots `1..=INK_SLOTS` are resolved to palette indices
//! when the image is drawn, so one image can be shown in several color
//! states.

/// Number of non-transparent ink slots a pixel can name.
pub const INK_SLOTS: usize = 3;

/// A failure to construct an [`InkImage`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InkImageError {
    #[error("a {width}x{height} image needs {expected} pixels, got {actual}")]
    Length {
        width: usize,
        height: usize,
        expected: usize,
        actual: usize,
    },
    #[error("pixel {index} names ink slot {slot}; slots go up to {INK_SLOTS}")]
    Slot { index: usize, slot: u8 },
}

/// A row-major image of ink slots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InkImage {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl InkImage {
    /// An image whose pixel `(x, y)` is `pixels[y * width + x]`.
    pub fn new(width: usize, height: usize, pixels: Vec<u8>) -> Result<Self, InkImageError> {
        let expected = width * height;
        if pixels.len() != expected {
            return Err(InkImageError::Length {
                width,
                height,
                expected,
                actual: pixels.len(),
            });
        }
        if let Some((index, &slot)) = pixels
            .iter()
            .enumerate()
            .find(|&(_, &slot)| usize::from(slot) > INK_SLOTS)
        {
            return Err(InkImageError::Slot { index, slot });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// The ink slot at `(x, y)`, or 0 (transparent) outside the image.
    pub fn pixel(&self, x: usize, y: usize) -> u8 {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x]
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests;
