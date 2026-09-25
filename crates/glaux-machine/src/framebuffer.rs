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
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const MAX_X: i32 = WIDTH as i32 - 1;
    const MAX_Y: i32 = HEIGHT as i32 - 1;

    #[test]
    fn has_exact_size() {
        let fb = Framebuffer::new();
        assert_eq!(LEN, 76_800);
        assert_eq!(fb.as_bytes().len(), 320 * 240);
    }

    #[test]
    fn starts_cleared_to_zero() {
        assert!(Framebuffer::new().as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn clear_fills_every_pixel() {
        let mut fb = Framebuffer::new();
        fb.set_pixel(10, 10, 3);
        fb.clear(17);
        assert!(fb.as_bytes().iter().all(|&b| b == 17));
    }

    #[test]
    fn reads_back_written_pixel() {
        let mut fb = Framebuffer::new();
        assert!(fb.set_pixel(42, 17, 9));
        assert_eq!(fb.pixel(42, 17), Some(9));
        assert_eq!(fb.as_bytes()[17 * WIDTH + 42], 9);
    }

    #[test]
    fn writes_and_reads_corners() {
        let mut fb = Framebuffer::new();
        assert!(fb.set_pixel(0, 0, 1));
        assert!(fb.set_pixel(MAX_X, MAX_Y, 2));
        assert_eq!(fb.pixel(0, 0), Some(1));
        assert_eq!(fb.pixel(MAX_X, MAX_Y), Some(2));
        assert_eq!(fb.as_bytes()[0], 1);
        assert_eq!(fb.as_bytes()[LEN - 1], 2);
    }

    #[test]
    fn out_of_range_coordinates_are_clipped() {
        let mut fb = Framebuffer::new();
        fb.clear(5);
        let before = fb.clone();

        let outside = [
            (-1, 0),
            (0, -1),
            (WIDTH as i32, 0),
            (0, HEIGHT as i32),
            (WIDTH as i32, HEIGHT as i32),
            (i32::MIN, i32::MIN),
            (i32::MAX, i32::MAX),
            (i32::MIN, 0),
            (0, i32::MAX),
        ];
        for (x, y) in outside {
            assert_eq!(fb.pixel(x, y), None, "read at ({x}, {y})");
            assert!(!fb.set_pixel(x, y, 9), "write at ({x}, {y})");
        }

        assert_eq!(fb, before);
    }

    #[test]
    fn write_does_not_touch_neighbors() {
        let mut fb = Framebuffer::new();
        fb.set_pixel(100, 100, 7);

        for dy in -1..=1 {
            for dx in -1..=1 {
                let expected = if dx == 0 && dy == 0 { 7 } else { 0 };
                assert_eq!(fb.pixel(100 + dx, 100 + dy), Some(expected));
            }
        }
        assert_eq!(fb.as_bytes().iter().filter(|&&b| b != 0).count(), 1);
    }

    #[test]
    fn row_edge_write_does_not_wrap() {
        let mut fb = Framebuffer::new();
        fb.set_pixel(MAX_X, 0, 7);
        assert_eq!(fb.pixel(0, 1), Some(0));
        assert_eq!(fb.pixel(MAX_X - 1, 0), Some(0));
    }
}
