//! Drawing the signature popup.

use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_syntax::SyntaxKind;

use super::SignaturePopup;
use super::layout::{Segment, SignatureLayout};
use crate::system::screen::PALE;
use crate::system::syntax_theme;
use crate::system::workspace::code::popup::Canvas;
use crate::system::workspace::{BAR, Rect, take_cells};

impl SignaturePopup {
    /// Draws the popup as `layout` places it.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        cell_w: i32,
        cell_h: i32,
        layout: &SignatureLayout,
    ) {
        let mut canvas = Canvas {
            framebuffer,
            font,
            cell_w,
        };
        canvas.frame(layout.window);
        canvas.segments(
            &layout.segments,
            layout.text_x,
            layout.line_y,
            layout.columns,
            cell_h,
        );
        if let Some(rule) = layout.rule {
            canvas.fill(rule, BAR);
        }
        canvas.segments(
            &layout.head,
            layout.text_x,
            layout.head_y,
            layout.columns,
            cell_h,
        );
        for note in &layout.notes {
            canvas.text(&note.text, note.x, note.y, PALE);
        }
    }
}

impl Canvas<'_> {
    /// Draws `segments` in a row from `x`, cutting what does not fit in
    /// `columns`. A chip segment gets a band behind its text.
    fn segments(&mut self, segments: &[Segment], x: i32, y: i32, columns: usize, cell_h: i32) {
        let mut left = columns;
        let mut at = x;
        for segment in segments {
            let shown = take_cells(self.font, &segment.text, left);
            let used = self.font.text_cells(shown);
            let width = i32::try_from(used).unwrap_or(0) * self.cell_w;
            if segment.chip {
                self.fill(
                    Rect {
                        x: at,
                        y,
                        width,
                        height: cell_h,
                    },
                    BAR,
                );
            }
            self.text(shown, at, y, color(segment.kind));
            left -= used;
            at += width;
        }
    }
}

fn color(kind: SyntaxKind) -> u8 {
    syntax_theme::color(kind)
}
