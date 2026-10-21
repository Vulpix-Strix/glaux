//! Drawing the hover popup.

use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;

use super::layout::HoverLayout;
use crate::system::workspace::BAR;
use crate::system::workspace::code::popup::Canvas;

impl HoverLayout {
    /// Draws the popup.
    pub fn render(&self, framebuffer: &mut Framebuffer, font: &BitmapFont, cell_w: i32) {
        let mut canvas = Canvas {
            framebuffer,
            font,
            cell_w,
        };
        canvas.frame(self.window);
        for rule in &self.rules {
            canvas.fill(*rule, BAR);
        }
        for text in &self.texts {
            canvas.text(&text.text, text.x, text.y, text.color);
        }
    }
}
