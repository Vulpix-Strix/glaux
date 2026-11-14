//! Drawing the hover popup.

use super::layout::HoverLayout;
use crate::system::workspace::BAR;
use crate::system::workspace::RenderContext;
use crate::system::workspace::code::popup::Canvas;

impl HoverLayout {
    /// Draws the popup.
    pub fn render(&self, ctx: &mut RenderContext<'_>, cell_w: i32) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
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
