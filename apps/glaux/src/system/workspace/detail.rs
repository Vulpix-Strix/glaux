//! A window that shows text too long for the place it came from, scrolled
//! by the user.

use glaux_input::InputEvent;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{draw_text, fill_rect};

use super::dialog::{INK, PAD, draw_window_with_detail};
use super::{BAR, Rect, thumb};
use crate::system::text::wrap;

/// Rows of text the window shows at once.
const ROWS: usize = 8;
/// Space the scrollbar takes at the window's right edge.
const RAIL: i32 = 3;

/// What the window did with an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailAction {
    /// The window took the event and stays open.
    Handled,
    /// The user asked to close the window.
    Close,
}

/// The whole of a text in a window, with `title` and `place` in its bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    title: String,
    place: String,
    lines: Vec<String>,
    top: usize,
    window: Rect,
    cell_w: i32,
    cell_h: i32,
}

impl Detail {
    /// A window centred in `area` for `body`, wrapped to fit. Explicit
    /// newlines in `body` are kept.
    pub fn new(font: &BitmapFont, area: Rect, title: &str, place: &str, body: &str) -> Self {
        let cell_w = i32::from(font.metrics().cell_width());
        let cell_h = i32::from(font.metrics().cell_height());
        let width = area.width - 2 * PAD;
        let height = Self::bar_height(cell_h) + ROWS as i32 * cell_h + 2 * PAD;
        let window = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + (area.height - height).max(0) / 2,
            width,
            height: height.min(area.height),
        };
        let columns = usize::try_from((width - 2 * PAD - RAIL) / cell_w.max(1)).unwrap_or(1);
        Self {
            title: title.to_owned(),
            place: place.to_owned(),
            lines: wrap(font, body, columns),
            top: 0,
            window,
            cell_w,
            cell_h,
        }
    }

    fn bar_height(cell_h: i32) -> i32 {
        cell_h + 3
    }

    fn furthest(&self) -> usize {
        self.lines.len().saturating_sub(ROWS)
    }

    fn scroll(&mut self, by: isize) {
        self.top = self.top.saturating_add_signed(by).min(self.furthest());
    }

    /// Scrolls or closes. Every other event is taken without effect, since
    /// the window has the whole input while it is open.
    pub fn handle_input(&mut self, event: InputEvent) -> DetailAction {
        match event {
            InputEvent::Up { .. } => self.scroll(-1),
            InputEvent::Down { .. } => self.scroll(1),
            InputEvent::PageUp { .. } => self.scroll(-(ROWS as isize)),
            InputEvent::PageDown { .. } => self.scroll(ROWS as isize),
            InputEvent::Wheel { delta } => self.scroll(-isize::from(delta)),
            InputEvent::Break | InputEvent::Enter | InputEvent::Tab { .. } => {
                return DetailAction::Close;
            }
            _ => {}
        }
        DetailAction::Handled
    }

    /// Draws the window.
    pub fn render(&self, framebuffer: &mut Framebuffer, font: &BitmapFont) {
        let window = self.window;
        let bar = Rect {
            x: window.x + 1,
            y: window.y + 1,
            width: window.width - 2,
            height: Self::bar_height(self.cell_h),
        };
        draw_window_with_detail(
            framebuffer,
            font,
            window,
            bar,
            &self.title,
            &self.place,
            self.cell_w,
        );
        let x = window.x + PAD;
        let y = bar.bottom() + PAD / 2;
        for (n, line) in self.lines.iter().skip(self.top).take(ROWS).enumerate() {
            draw_text(framebuffer, font, line, x, y + n as i32 * self.cell_h, INK);
        }
        let track = ROWS as i32 * self.cell_h;
        if let Some((start, length)) = thumb(self.top, ROWS, self.lines.len(), track) {
            let rail = window.right() - 1 - RAIL;
            fill_rect(framebuffer, rail, y, RAIL - 1, track, BAR);
            fill_rect(framebuffer, rail, y + start, RAIL - 1, length, INK);
        }
    }
}
