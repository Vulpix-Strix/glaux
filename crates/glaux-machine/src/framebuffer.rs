//! Logical indexed framebuffer.

/// Framebuffer width in pixels.
pub const WIDTH: usize = 320;
/// Framebuffer height in pixels.
pub const HEIGHT: usize = 240;
/// Framebuffer size in bytes: one byte per pixel.
pub const LEN: usize = WIDTH * HEIGHT;

/// A 320×240 framebuffer where each byte is an index into the 256-entry
/// palette presented alongside it.
///
/// Pixels are stored row-major: the byte for `(x, y)` is at `y * WIDTH + x`.
///
/// Coordinates are signed. Coordinates outside the framebuffer are clipped:
/// reads return `None` and writes are ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Framebuffer {
    // Boxed to keep 76 800 bytes off the stack; the array type keeps the
    // length fixed.
    pixels: Box<[u8; LEN]>,
}

impl Framebuffer {
    /// Creates a framebuffer filled with index `0`.
    pub fn new() -> Self {
        Self {
            pixels: Box::new([0; LEN]),
        }
    }

    /// Returns the palette index at `(x, y)`, or `None` if the coordinates
    /// fall outside the framebuffer.
    pub fn pixel(&self, x: i32, y: i32) -> Option<u8> {
        Self::offset(x, y).map(|offset| self.pixels[offset])
    }

    /// Writes a palette index at `(x, y)`.
    ///
    /// Returns `false` and leaves the framebuffer untouched if the
    /// coordinates fall outside it.
    pub fn set_pixel(&mut self, x: i32, y: i32, index: u8) -> bool {
        match Self::offset(x, y) {
            Some(offset) => {
                self.pixels[offset] = index;
                true
            }
            None => false,
        }
    }

    /// Fills the whole framebuffer with a palette index.
    pub fn clear(&mut self, index: u8) {
        self.pixels.fill(index);
    }

    /// Returns the raw row-major pixel indices.
    pub fn as_bytes(&self) -> &[u8; LEN] {
        &self.pixels
    }

    /// Returns the raw row-major pixel indices for in-place rewriting.
    pub fn as_bytes_mut(&mut self) -> &mut [u8; LEN] {
        &mut self.pixels
    }

    fn offset(x: i32, y: i32) -> Option<usize> {
        let x = usize::try_from(x).ok().filter(|&x| x < WIDTH)?;
        let y = usize::try_from(y).ok().filter(|&y| y < HEIGHT)?;
        Some(y * WIDTH + x)
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
