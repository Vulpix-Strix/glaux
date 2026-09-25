//! Static screen for checking text rendering with the System ROM font.

use glaux_machine::font::{BitmapFont, FIRST_CHAR, LAST_CHAR};
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{clear, draw_text};

// Zughy 32 indices.
const BACKGROUND: u8 = 11;
const WHITE: u8 = 16;
const CYAN: u8 = 18;
const PINK: u8 = 25;
const GRAY: u8 = 14;

/// Printable ASCII characters per line in the glyph table.
const TABLE_COLUMNS: usize = 48;

/// Draws a few lines of text plus every printable ASCII glyph.
pub fn draw(fb: &mut Framebuffer, font: &BitmapFont) {
    let metrics = font.metrics();
    let (cell_w, cell_h) = (
        i32::from(metrics.cell_width()),
        i32::from(metrics.cell_height()),
    );
    let row = |n: i32| cell_h + n * cell_h;

    clear(fb, BACKGROUND);
    draw_text(fb, font, "GLAUX", cell_w, row(0), WHITE);
    draw_text(fb, font, "SYSTEM ROM", cell_w, row(1), CYAN);
    draw_text(fb, font, "HELLO, WORLD!", cell_w, row(2), PINK);

    let ascii: Vec<char> = (FIRST_CHAR..=LAST_CHAR).collect();
    let table = ascii
        .chunks(TABLE_COLUMNS)
        .map(|line| line.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    draw_text(fb, font, &table, cell_w, row(4), GRAY);
}
