//! Drawing the suggestion list and its panel.

use glaux_input::Point;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{draw_text, fill_rect};

use super::layout::{
    CompletionLayout, MARKER_GAP, PANEL_PAD_X, PanelLayout, PanelLine, ROW_PAD_LEFT, ROW_PAD_RIGHT,
    SHADOW,
};
use super::panel::DocPanel;
use super::{CompletionPopup, marker};
use crate::system::syntax_theme;
use crate::system::workspace::{BAR, HOVER_SHADE, Rect, take_cells};
use crate::system::{BACKGROUND, DIM, PALE, TEXT};

/// Space between the two keys named in the panel's foot.
const FOOT_GAP: i32 = 8;

impl CompletionPopup {
    /// Draws the list and its panel as `layout` places them, with the row
    /// under `pointer` highlighted.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        cell_w: i32,
        layout: &CompletionLayout,
        pointer: Option<Point>,
    ) {
        let mut canvas = Canvas {
            framebuffer,
            font,
            cell_w,
        };
        canvas.frame(layout.list);
        for &(index, row) in &layout.rows {
            let Some(item) = self.completions.items.get(index) else {
                continue;
            };
            let selected = index == self.selected;
            let hovered = pointer.is_some_and(|at| row.contains(at));
            let (ground, name, detail) = if selected {
                (Some(PALE), BACKGROUND, BACKGROUND)
            } else if hovered {
                (Some(HOVER_SHADE), TEXT, DIM)
            } else {
                (None, TEXT, DIM)
            };
            if let Some(ground) = ground {
                canvas.fill(row, ground);
            }
            let (letter, kind) = marker(item.kind);
            let marker_ink = if selected {
                BACKGROUND
            } else {
                syntax_theme::color(kind)
            };
            let mut letter_text = [0; 4];
            let letter_text: &str = letter.encode_utf8(&mut letter_text);
            canvas.text(letter_text, row.x + ROW_PAD_LEFT, row.y, marker_ink);
            let name_x = row.x + ROW_PAD_LEFT + cell_w + MARKER_GAP;
            canvas.text(&item.label, name_x, row.y, name);
            let detail_cells = i32::try_from(font.text_cells(&item.detail)).unwrap_or(0);
            let detail_x = row.right() - ROW_PAD_RIGHT - detail_cells * cell_w;
            canvas.text(&item.detail, detail_x, row.y, detail);
        }
        if let Some(track) = layout.track {
            canvas.fill(track, BAR);
        }
        if let Some(thumb) = layout.thumb {
            canvas.fill(thumb, DIM);
        }
        if let (Some(panel), Some(layout)) = (self.panel(), &layout.panel) {
            canvas.panel(panel, layout);
        }
    }
}

/// A frame being drawn, with the font and the cell width the text is
/// measured by.
struct Canvas<'a> {
    framebuffer: &'a mut Framebuffer,
    font: &'a BitmapFont,
    cell_w: i32,
}

impl Canvas<'_> {
    fn fill(&mut self, rect: Rect, color: u8) {
        fill_rect(
            self.framebuffer,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            color,
        );
    }

    fn text(&mut self, text: &str, x: i32, y: i32, color: u8) {
        draw_text(self.framebuffer, self.font, text, x, y, color);
    }

    /// An empty box with a border and a hard shadow.
    fn frame(&mut self, rect: Rect) {
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

    fn panel(&mut self, panel: &DocPanel, layout: &PanelLayout) {
        let window = layout.window;
        self.frame(window);
        let x = window.x + 1 + PANEL_PAD_X;
        let columns =
            usize::try_from((window.width - 2 - 2 * PANEL_PAD_X) / self.cell_w.max(1)).unwrap_or(0);
        for line in &layout.lines {
            match line {
                PanelLine::Signature { y } => {
                    let mut left = columns;
                    let mut at = x;
                    for (piece, kind) in &panel.signature {
                        let shown = take_cells(self.font, piece, left);
                        self.text(shown, at, y + 3, syntax_theme::color(*kind));
                        let used = self.font.text_cells(shown);
                        left -= used;
                        at += i32::try_from(used).unwrap_or(0) * self.cell_w;
                    }
                }
                PanelLine::Rule { y } => self.fill(
                    Rect {
                        x: window.x + 1,
                        y: *y,
                        width: window.width - 2,
                        height: 1,
                    },
                    BAR,
                ),
                PanelLine::Summary { y, text } => self.text(text, x, *y, PALE),
                PanelLine::Params { y, text } => self.text(text, x, *y, DIM),
            }
        }
        self.foot(layout.foot);
    }

    /// The bar naming the keys the list answers to.
    fn foot(&mut self, foot: Rect) {
        self.fill(foot, BAR);
        let mut x = foot.x + PANEL_PAD_X;
        let y = foot.y + 2;
        for (key, action) in [("Tab", ": pick"), ("Esc", ": close")] {
            self.text(key, x, y, PALE);
            x += i32::try_from(self.font.text_cells(key)).unwrap_or(0) * self.cell_w;
            self.text(action, x, y, DIM);
            x += i32::try_from(self.font.text_cells(action)).unwrap_or(0) * self.cell_w + FOOT_GAP;
        }
    }
}
