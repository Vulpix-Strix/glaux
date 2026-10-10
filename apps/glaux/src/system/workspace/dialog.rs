//! Modal dialogs: a framed window drawn over the workspace that takes every
//! input until one of its actions is chosen.
//!
//! Windows are drawn the way the system's fictional hardware would: square,
//! pixel-aligned rectangles with a solid title bar, a thin border and a
//! small offset shadow, with the focused button inverted.

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{draw_text, fill_rect};

use super::{BAR, Rect, wrap};
use crate::system::{BACKGROUND, TEXT};

/// Window body.
pub const BODY: u8 = TEXT;
/// Body text, borders, the title bar and focused buttons.
pub const INK: u8 = BACKGROUND;
/// The window's drop shadow. Lighter than the dark workspace behind it so
/// it still reads as an edge.
const SHADOW: u8 = BAR;
/// Offset of the drop shadow, right and down.
pub const SHADOW_OFFSET: i32 = 2;
/// Space between the window border and its contents.
pub const PAD: i32 = 6;
/// Space kept between the window and the edges of the area it is centred
/// in.
pub const AREA_MARGIN: i32 = 8;
/// Most text columns a message line takes.
const MAX_COLUMNS: i32 = 40;
/// Buttons are indented from the body text by this many cells.
const BUTTON_INDENT: i32 = 2;
/// Space between stacked buttons.
const BUTTON_GAP: i32 = 3;

/// A modal dialog: a title, a message, and the actions that close it. `A`
/// is what choosing an action yields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog<A> {
    title: String,
    /// Paragraphs, wrapped to the window when laid out.
    message: Vec<String>,
    actions: Vec<(String, A)>,
    focus: usize,
    /// What Escape yields, if the dialog can be dismissed that way.
    cancel: Option<A>,
    /// The button a primary press started on. Releasing on the same button
    /// chooses it.
    armed: Option<usize>,
}

/// Where a dialog's parts sit for one area and font. Rendering and
/// hit-testing both read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogLayout {
    pub window: Rect,
    pub title_bar: Rect,
    /// Message lines and the top of each.
    pub lines: Vec<(String, i32)>,
    pub text_x: i32,
    pub buttons: Vec<Rect>,
    cell_w: i32,
    cell_h: i32,
}

impl<A: Copy> Dialog<A> {
    /// A dialog with `actions`, top to bottom, focusing `focus`. Escape
    /// yields `cancel` when given.
    ///
    /// # Panics
    ///
    /// If `actions` is empty or `focus` is not one of them: a dialog that
    /// cannot be closed is a programming error.
    pub fn new(
        title: impl Into<String>,
        message: Vec<String>,
        actions: Vec<(&str, A)>,
        focus: usize,
        cancel: Option<A>,
    ) -> Self {
        assert!(focus < actions.len(), "a dialog needs a focused action");
        Self {
            title: title.into(),
            message,
            actions: actions
                .into_iter()
                .map(|(label, action)| (label.to_owned(), action))
                .collect(),
            focus,
            cancel,
            armed: None,
        }
    }

    #[cfg(test)]
    pub fn focus(&self) -> usize {
        self.focus
    }

    #[cfg(test)]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[cfg(test)]
    pub fn labels(&self) -> Vec<&str> {
        self.actions
            .iter()
            .map(|(label, _)| label.as_str())
            .collect()
    }

    /// Applies `event`, returning the chosen action, if any. Every event is
    /// taken, so nothing reaches what lies under the dialog.
    ///
    /// Up and Left focus the previous button, Down, Right and Tab the next,
    /// wrapping around. Enter chooses the focused button and Escape the
    /// cancel action. A primary press on a button focuses it and releasing
    /// on the same button chooses it.
    pub fn handle_input(&mut self, event: InputEvent, layout: &DialogLayout) -> Option<A> {
        let count = self.actions.len();
        match event {
            InputEvent::Up | InputEvent::Left { .. } => {
                self.focus = (self.focus + count - 1) % count;
            }
            InputEvent::Down | InputEvent::Right { .. } | InputEvent::Tab => {
                self.focus = (self.focus + 1) % count;
            }
            InputEvent::Enter => return Some(self.actions[self.focus].1),
            InputEvent::Break => return self.cancel,
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
            } => {
                self.armed = layout.button_at(at);
                if let Some(button) = self.armed {
                    self.focus = button;
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                at,
            } => {
                let armed = self.armed.take();
                if armed.is_some() && armed == layout.button_at(at) {
                    return armed.map(|button| self.actions[button].1);
                }
            }
            InputEvent::PointerCancel => self.armed = None,
            _ => {}
        }
        None
    }

    /// Lays the dialog out centred in `area` for a font whose character
    /// cells are `cell_w` by `cell_h` pixels.
    pub fn layout(&self, area: Rect, cell_w: i32, cell_h: i32) -> DialogLayout {
        let cell_w = cell_w.max(1);
        let room = (area.width - 2 * AREA_MARGIN - 2 * PAD - 2 - SHADOW_OFFSET) / cell_w;
        let columns = room.clamp(1, MAX_COLUMNS);

        let mut wrapped: Vec<String> = Vec::new();
        for (index, paragraph) in self.message.iter().enumerate() {
            if index > 0 {
                wrapped.push(String::new());
            }
            wrapped.extend(wrap(paragraph, columns as usize));
        }
        let title_cells = text_cells(&self.title);
        let label_cells = self
            .actions
            .iter()
            .map(|(label, _)| text_cells(label))
            .max()
            .unwrap_or(0);
        let button_width = (label_cells + 4) * cell_w;
        let button_height = cell_h + 4;
        let widest = wrapped
            .iter()
            .map(|line| text_cells(line) * cell_w)
            .chain([title_cells * cell_w, BUTTON_INDENT * cell_w + button_width])
            .max()
            .unwrap_or(0);
        let inner_width = widest.min(columns * cell_w);

        let title_height = cell_h + 2;
        let buttons_height = self.actions.len() as i32 * (button_height + BUTTON_GAP) - BUTTON_GAP;
        let body_height = PAD + wrapped.len() as i32 * cell_h + cell_h + buttons_height + PAD;
        let width = inner_width + 2 * PAD + 2;
        let height = 1 + title_height + body_height + 1;
        let window = Rect {
            x: area.x + (area.width - width - SHADOW_OFFSET) / 2,
            y: area.y + (area.height - height - SHADOW_OFFSET) / 2,
            width,
            height,
        };
        let title_bar = Rect {
            x: window.x + 1,
            y: window.y + 1,
            width: width - 2,
            height: title_height,
        };
        let text_x = window.x + 1 + PAD;
        let mut y = title_bar.bottom() + PAD;
        let lines = wrapped
            .into_iter()
            .map(|line| {
                let top = y;
                y += cell_h;
                (line, top)
            })
            .collect();
        y += cell_h;
        let buttons = self
            .actions
            .iter()
            .map(|_| {
                let rect = Rect {
                    x: text_x + BUTTON_INDENT * cell_w,
                    y,
                    width: button_width,
                    height: button_height,
                };
                y += button_height + BUTTON_GAP;
                rect
            })
            .collect();
        DialogLayout {
            window,
            title_bar,
            lines,
            text_x,
            buttons,
            cell_w,
            cell_h,
        }
    }

    /// Draws the dialog as laid out in `layout`, over whatever is already
    /// in `framebuffer`.
    pub fn render(&self, framebuffer: &mut Framebuffer, font: &BitmapFont, layout: &DialogLayout) {
        let DialogLayout {
            window,
            title_bar,
            cell_w,
            cell_h,
            ..
        } = *layout;
        draw_window(framebuffer, font, window, title_bar, &self.title, cell_w);
        for (line, y) in &layout.lines {
            draw_text(framebuffer, font, line, layout.text_x, *y, INK);
        }
        for (index, ((label, _), &rect)) in self.actions.iter().zip(&layout.buttons).enumerate() {
            draw_button(
                framebuffer,
                font,
                rect,
                label,
                index == self.focus,
                cell_w,
                cell_h,
            );
        }
    }
}

/// Draws a window's drop shadow, border, body and title bar, with `title`
/// centred in the bar.
pub fn draw_window(
    framebuffer: &mut Framebuffer,
    font: &BitmapFont,
    window: Rect,
    title_bar: Rect,
    title: &str,
    cell_w: i32,
) {
    let shadow = Rect {
        x: window.x + SHADOW_OFFSET,
        y: window.y + SHADOW_OFFSET,
        ..window
    };
    fill(framebuffer, shadow, SHADOW);
    fill(framebuffer, window, INK);
    fill(framebuffer, window.inset(1), BODY);
    fill(framebuffer, title_bar, INK);
    let title_x = title_bar.x + (title_bar.width - text_cells(title) * cell_w) / 2;
    draw_text(framebuffer, font, title, title_x, title_bar.y + 2, BODY);
}

/// Draws a push button labelled `label`, inverted with an inner rule when
/// `focused`.
pub fn draw_button(
    framebuffer: &mut Framebuffer,
    font: &BitmapFont,
    rect: Rect,
    label: &str,
    focused: bool,
    cell_w: i32,
    cell_h: i32,
) {
    fill(framebuffer, rect, INK);
    let (face, ink) = if focused { (INK, BODY) } else { (BODY, INK) };
    fill(framebuffer, rect.inset(1), face);
    if focused {
        // An inner rule marks the focused button even where the inversion
        // alone is hard to see.
        let ring = rect.inset(2);
        fill(framebuffer, ring, BODY);
        fill(framebuffer, ring.inset(1), INK);
    }
    let x = rect.x + (rect.width - text_cells(label) * cell_w) / 2;
    let y = rect.y + (rect.height - cell_h) / 2 + 1;
    draw_text(framebuffer, font, label, x, y, ink);
}

/// Fills `rect` with `color`.
pub fn fill(framebuffer: &mut Framebuffer, rect: Rect, color: u8) {
    fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, color);
}

impl DialogLayout {
    /// The button under `point`, by index.
    pub fn button_at(&self, point: Point) -> Option<usize> {
        self.buttons.iter().position(|rect| rect.contains(point))
    }
}

/// Character cells `text` takes.
pub fn text_cells(text: &str) -> i32 {
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests;
