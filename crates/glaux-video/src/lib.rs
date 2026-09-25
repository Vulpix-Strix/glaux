//! Video subsystem for Glaux.

use glaux_machine::framebuffer::{Framebuffer, LEN};
use glaux_machine::palette::{PALETTE_LEN, Palette};

pub use glaux_machine::framebuffer::{HEIGHT, WIDTH};
pub use glaux_machine::palette::Rgb;

/// Row-major palette indices plus the 256-entry
/// palette they index into.
///
/// Every byte in [`pixels`](Self::pixels) is a valid index into
/// [`palette`](Self::palette), so a host can resolve colors without knowing
/// anything else about the machine.
#[derive(Clone, Copy, Debug)]
pub struct IndexedFrame<'a> {
    pixels: &'a [u8; LEN],
    palette: &'a [Rgb; PALETTE_LEN],
}

impl<'a> IndexedFrame<'a> {
    /// Borrows a framebuffer and the palette it should be presented with.
    pub fn new(framebuffer: &'a Framebuffer, palette: &'a Palette) -> Self {
        Self {
            pixels: framebuffer.as_bytes(),
            palette: palette.entries(),
        }
    }

    /// Frame width in pixels.
    pub const fn width(&self) -> usize {
        WIDTH
    }

    /// Frame height in pixels.
    pub const fn height(&self) -> usize {
        HEIGHT
    }

    /// Row-major palette indices, `width() * height()` bytes-long.
    pub fn pixels(&self) -> &'a [u8; LEN] {
        self.pixels
    }

    /// The 256 colors that pixel indices resolve to.
    pub fn palette(&self) -> &'a [Rgb; PALETTE_LEN] {
        self.palette
    }
}
