//! What the boxes that float over the text have in common.

use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{draw_text, fill_rect};

use crate::system::workspace::{BAR, Rect};
use crate::system::{BACKGROUND, DIM};

/// Space the hard shadow takes right of and below a box.
pub const SHADOW: i32 = 2;

/// What a popup is laid out with and where it may go.
#[derive(Clone, Copy)]
pub struct Place<'a> {
    pub font: &'a BitmapFont,
    /// Everything the popup may cover.
    pub area: Rect,
    pub cell_w: i32,
    pub cell_h: i32,
}

/// A frame being drawn, with the font and the cell width the text is
/// measured by.
pub struct Canvas<'a> {
    pub framebuffer: &'a mut Framebuffer,
    pub font: &'a BitmapFont,
    pub cell_w: i32,
}

impl Canvas<'_> {
    pub fn fill(&mut self, rect: Rect, color: u8) {
        fill_rect(
            self.framebuffer,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            color,
        );
    }

    pub fn text(&mut self, text: &str, x: i32, y: i32, color: u8) {
        draw_text(self.framebuffer, self.font, text, x, y, color);
    }

    /// An empty box with a border and a hard shadow.
    pub fn frame(&mut self, rect: Rect) {
        self.fill(
            Rect {
                x: rect.x + SHADOW,
                y: rect.y + SHADOW,
                ..rect
            },
            BAR,
        );
        self.fill(rect, DIM);
        self.fill(rect.inset(1), BACKGROUND);
    }
}
