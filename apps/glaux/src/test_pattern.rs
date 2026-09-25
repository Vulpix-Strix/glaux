//! Static pattern for checking palette resolution and presentation.

use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};

const BAR_WIDTH: i32 = 10;
const BARS_HEIGHT: i32 = 160;

/// An index the provisional palette leaves unused, so it shows debug magenta.
const UNUSED_INDEX: u8 = 200;

/// Draws:
/// - 32 vertical bars across the top, one per Zughy 32 index `0..32`;
/// - bottom left: a 1-pixel checkerboard of indices 0 and 16, which stays
///   crisp only under nearest-neighbor scaling;
/// - bottom right: a block of index 200, which should appear magenta.
pub fn draw(fb: &mut Framebuffer) {
    let (width, height) = (WIDTH as i32, HEIGHT as i32);

    for y in 0..height {
        for x in 0..width {
            let index = if y < BARS_HEIGHT {
                (x / BAR_WIDTH) as u8
            } else if x < width / 2 {
                if (x + y) % 2 == 0 { 0 } else { 16 }
            } else {
                UNUSED_INDEX
            };
            fb.set_pixel(x, y, index);
        }
    }
}
