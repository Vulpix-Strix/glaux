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
const BODY: u8 = TEXT;
/// Body text, borders, the title bar and focused buttons.
const INK: u8 = BACKGROUND;
/// The window's drop shadow. Lighter than the dark workspace behind it so
/// it still reads as an edge.
const SHADOW: u8 = BAR;
/// Offset of the drop shadow, right and down.
const SHADOW_OFFSET: i32 = 2;
/// Space between the window border and its contents.
const PAD: i32 = 6;
/// Space kept between the window and the edges of the area it is centred
/// in.
const AREA_MARGIN: i32 = 8;
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
        let fill = |fb: &mut Framebuffer, rect: Rect, color: u8| {
            fill_rect(fb, rect.x, rect.y, rect.width, rect.height, color);
        };
        let shadow = Rect {
            x: window.x + SHADOW_OFFSET,
            y: window.y + SHADOW_OFFSET,
            ..window
        };
        fill(framebuffer, shadow, SHADOW);
        fill(framebuffer, window, INK);
        fill(framebuffer, window.inset(1), BODY);

        fill(framebuffer, title_bar, INK);
        let title_x = title_bar.x + (title_bar.width - text_cells(&self.title) * cell_w) / 2;
        draw_text(
            framebuffer,
            font,
            &self.title,
            title_x,
            title_bar.y + 2,
            BODY,
        );

        for (line, y) in &layout.lines {
            draw_text(framebuffer, font, line, layout.text_x, *y, INK);
        }

        for (index, ((label, _), &rect)) in self.actions.iter().zip(&layout.buttons).enumerate() {
            let focused = index == self.focus;
            fill(framebuffer, rect, INK);
            let (face, ink) = if focused { (INK, BODY) } else { (BODY, INK) };
            fill(framebuffer, rect.inset(1), face);
            if focused {
                // An inner rule marks the focused button even where the
                // inversion alone is hard to see.
                let ring = rect.inset(2);
                fill(framebuffer, ring, BODY);
                fill(framebuffer, ring.inset(1), INK);
            }
            let x = rect.x + (rect.width - text_cells(label) * cell_w) / 2;
            let y = rect.y + (rect.height - cell_h) / 2 + 1;
            draw_text(framebuffer, font, label, x, y, ink);
        }
    }
}

impl DialogLayout {
    /// The button under `point`, by index.
    pub fn button_at(&self, point: Point) -> Option<usize> {
        self.buttons.iter().position(|rect| rect.contains(point))
    }
}

fn text_cells(text: &str) -> i32 {
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaux_machine::font::FontMetrics;
    use glaux_machine::framebuffer::{HEIGHT, WIDTH};
    use pretty_assertions::assert_eq;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Choice {
        Yes,
        No,
        Cancel,
    }

    const CELL_W: i32 = FontMetrics::SYSTEM.cell_width() as i32;
    const CELL_H: i32 = FontMetrics::SYSTEM.cell_height() as i32;

    /// The workspace's area below a 16-pixel tab bar on the 320×240 screen.
    const AREA: Rect = Rect {
        x: 0,
        y: 16,
        width: WIDTH as i32,
        height: HEIGHT as i32 - 16,
    };

    fn dialog() -> Dialog<Choice> {
        Dialog::new(
            "QUESTION",
            vec![
                "A first paragraph that is long enough to need wrapping at the window's width."
                    .into(),
                "A second one.".into(),
            ],
            vec![
                ("YES", Choice::Yes),
                ("NO", Choice::No),
                ("CANCEL", Choice::Cancel),
            ],
            2,
            Some(Choice::Cancel),
        )
    }

    fn centre(rect: Rect) -> Point {
        Point::new(
            (rect.x + rect.width / 2) as u16,
            (rect.y + rect.height / 2) as u16,
        )
    }

    fn press(at: Point) -> InputEvent {
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            at,
        }
    }

    fn release(at: Point) -> InputEvent {
        InputEvent::PointerUp {
            button: PointerButton::Primary,
            at,
        }
    }

    #[test]
    fn focus_moves_both_ways_and_wraps() {
        let mut dialog = dialog();
        let layout = dialog.layout(AREA, CELL_W, CELL_H);
        assert_eq!(dialog.focus(), 2);
        for (event, focus) in [
            (InputEvent::Down, 0),
            (InputEvent::Tab, 1),
            (InputEvent::Right { select: false }, 2),
            (InputEvent::Up, 1),
            (InputEvent::Left { select: false }, 0),
            (InputEvent::Up, 2),
        ] {
            assert_eq!(dialog.handle_input(event, &layout), None);
            assert_eq!(dialog.focus(), focus, "{event:?}");
        }
    }

    #[test]
    fn enter_chooses_the_focused_action_and_escape_cancels() {
        let mut dialog = dialog();
        let layout = dialog.layout(AREA, CELL_W, CELL_H);
        assert_eq!(
            dialog.handle_input(InputEvent::Enter, &layout),
            Some(Choice::Cancel)
        );
        dialog.handle_input(InputEvent::Down, &layout);
        assert_eq!(
            dialog.handle_input(InputEvent::Enter, &layout),
            Some(Choice::Yes)
        );
        assert_eq!(
            dialog.handle_input(InputEvent::Break, &layout),
            Some(Choice::Cancel)
        );

        let mut plain = Dialog::new("X", vec![], vec![("OK", Choice::Yes)], 0, None);
        let layout = plain.layout(AREA, CELL_W, CELL_H);
        assert_eq!(plain.handle_input(InputEvent::Break, &layout), None);
    }

    #[test]
    fn a_click_chooses_the_button_released_on() {
        let mut dialog = dialog();
        let layout = dialog.layout(AREA, CELL_W, CELL_H);
        let no = centre(layout.buttons[1]);
        assert_eq!(dialog.handle_input(press(no), &layout), None);
        assert_eq!(dialog.focus(), 1, "a press focuses its button");
        assert_eq!(dialog.handle_input(release(no), &layout), Some(Choice::No));
    }

    #[test]
    fn releasing_elsewhere_or_a_cancelled_press_chooses_nothing() {
        let mut dialog = dialog();
        let layout = dialog.layout(AREA, CELL_W, CELL_H);
        let yes = centre(layout.buttons[0]);
        let no = centre(layout.buttons[1]);
        dialog.handle_input(press(yes), &layout);
        assert_eq!(dialog.handle_input(release(no), &layout), None);

        dialog.handle_input(press(yes), &layout);
        dialog.handle_input(InputEvent::PointerCancel, &layout);
        assert_eq!(dialog.handle_input(release(yes), &layout), None);

        let outside = Point::new(1, 20);
        dialog.handle_input(press(outside), &layout);
        assert_eq!(dialog.handle_input(release(outside), &layout), None);
        assert_eq!(dialog.focus(), 0, "presses outside keep the focus");
    }

    #[test]
    fn other_input_is_swallowed() {
        let mut dialog = dialog();
        let layout = dialog.layout(AREA, CELL_W, CELL_H);
        for event in [
            InputEvent::Text('y'),
            InputEvent::NextView,
            InputEvent::Save,
            InputEvent::Undo,
            InputEvent::Wheel { delta: 1 },
        ] {
            assert_eq!(dialog.handle_input(event, &layout), None);
            assert_eq!(dialog.focus(), 2);
        }
    }

    #[test]
    fn the_window_fits_the_workspace_and_its_text_fits_the_window() {
        let dialog = dialog();
        let layout = dialog.layout(AREA, CELL_W, CELL_H);
        let window = layout.window;
        assert!(window.x >= AREA.x && window.y >= AREA.y, "{window:?}");
        assert!(window.right() + SHADOW_OFFSET <= AREA.right(), "{window:?}");
        assert!(
            window.bottom() + SHADOW_OFFSET <= AREA.bottom(),
            "{window:?}"
        );
        let body = window.inset(1 + PAD);
        assert!(layout.lines.len() > 3, "the long paragraph wraps");
        for (line, y) in &layout.lines {
            assert!(
                layout.text_x + text_cells(line) * CELL_W <= body.right(),
                "{line:?}"
            );
            assert!(*y >= layout.title_bar.bottom() && y + CELL_H <= body.bottom());
        }
        for (button, next) in layout.buttons.iter().zip(&layout.buttons[1..]) {
            assert!(button.bottom() < next.y);
        }
        for button in &layout.buttons {
            assert!(button.right() <= body.right() && button.bottom() <= body.bottom());
        }
        assert!(
            (window.x + window.width / 2 - (AREA.x + AREA.width / 2)).abs() <= SHADOW_OFFSET,
            "centred"
        );
    }

    #[test]
    fn the_focused_button_is_drawn_inverted() {
        let rom = crate::rom::load().unwrap();
        let dialog = dialog();
        let layout = dialog.layout(AREA, CELL_W, CELL_H);
        let mut fb = Framebuffer::new();
        dialog.render(&mut fb, rom.font(), &layout);
        let face = |rect: Rect| fb.pixel(rect.x + 1, rect.y + 1);
        assert_eq!(face(layout.buttons[2]), Some(INK), "focused");
        assert_eq!(face(layout.buttons[0]), Some(BODY));
        assert_eq!(fb.pixel(layout.title_bar.x, layout.title_bar.y), Some(INK));
        let shadow = layout.window;
        assert_eq!(
            fb.pixel(shadow.right(), shadow.bottom()),
            Some(SHADOW),
            "the shadow sits just past the bottom-right corner"
        );
    }
}
