//! A context menu is a list of actions opened at the pointer by a secondary
//! press. It takes every input until an action is chosen or the menu is
//! dismissed.
//!
//! The menu knows nothing of what its actions do. Each view builds the
//! entries that fit what was pressed on, with what is unavailable
//! disabled, and carries out the action the menu yields.

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{draw_text, fill_rect};

use super::{BAR, HOVER_SHADE, IDLE_SHADE, Rect};
use crate::system::screen::{DIM, PALE, TEXT};
use crate::ui_text;

/// The status bar hint while a context menu is open.
pub fn menu_hint() -> String {
    ui_text::get("workspace-up-down-pick-enter-run-esc-close")
}
/// Height of a separator row.
const SEPARATOR_HEIGHT: i32 = 5;
/// Cells left of a label.
const LABEL_INDENT: i32 = 1;
/// Cells at least between a label and its shortcut.
const SHORTCUT_GAP: i32 = 3;

/// One row of a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry<A> {
    Item {
        label: String,
        /// The keys that do the same, shown at the row's right end.
        shortcut: Option<&'static str>,
        enabled: bool,
        action: A,
    },
    /// A rule between groups of items.
    Separator,
}

impl<A> MenuEntry<A> {
    /// An enabled item.
    pub fn item(label: &str, shortcut: Option<&'static str>, action: A) -> Self {
        Self::Item {
            label: label.to_owned(),
            shortcut,
            enabled: true,
            action,
        }
    }

    /// An item that shows but can't be chosen unless `enabled`.
    pub fn when(enabled: bool, label: &str, shortcut: Option<&'static str>, action: A) -> Self {
        Self::Item {
            label: label.to_owned(),
            shortcut,
            enabled,
            action,
        }
    }

    fn is_enabled(&self) -> bool {
        matches!(self, Self::Item { enabled: true, .. })
    }
}

/// What an input did to a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOutcome<A> {
    /// Stay open.
    Stay,
    /// Close the menu and carry out `A`.
    Run(A),
    /// Close the menu because Escape was pressed.
    Close,
    /// Close the menu because a pointer button was pressed outside it. The
    /// press is the view's to act on or ignore.
    Outside,
}

/// An open menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenu<A> {
    entries: Vec<MenuEntry<A>>,
    /// Where the menu was opened: its window's top-left corner where the
    /// area leaves room.
    anchor: Point,
    /// The highlighted entry, always an enabled item.
    focus: Option<usize>,
}

/// Where a menu's parts sit for one area and font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuLayout {
    pub window: Rect,
    /// Each entry's row, in order.
    pub rows: Vec<Rect>,
    cell_w: i32,
}

impl MenuLayout {
    /// The entry whose row is under `point`.
    pub fn entry_at(&self, point: Point) -> Option<usize> {
        self.rows.iter().position(|row| row.contains(point))
    }
}

impl<A: Copy> ContextMenu<A> {
    /// A menu of `entries` opened at `anchor`, nothing highlighted.
    pub fn new(entries: Vec<MenuEntry<A>>, anchor: Point) -> Self {
        Self {
            entries,
            anchor,
            focus: None,
        }
    }

    #[cfg(test)]
    pub fn entries(&self) -> &[MenuEntry<A>] {
        &self.entries
    }

    #[cfg(test)]
    pub fn focus(&self) -> Option<usize> {
        self.focus
    }

    /// The menu laid out at its anchor, moved left or up as far as it
    /// takes to stay inside `area`.
    pub fn layout(&self, font: &BitmapFont, area: Rect, cell_w: i32, cell_h: i32) -> MenuLayout {
        let cells = |text: &str| i32::try_from(font.text_cells(text)).unwrap_or(i32::MAX);
        let (mut labels, mut shortcuts) = (0, 0);
        for entry in &self.entries {
            if let MenuEntry::Item {
                label, shortcut, ..
            } = entry
            {
                labels = labels.max(cells(label));
                shortcuts = shortcuts.max(shortcut.map_or(0, cells));
            }
        }
        let gap = if shortcuts > 0 { SHORTCUT_GAP } else { 0 };
        let width = (LABEL_INDENT + labels + gap + shortcuts + 1) * cell_w + 2;
        let row_height = cell_h + 2;
        let height = self
            .entries
            .iter()
            .map(|entry| match entry {
                MenuEntry::Item { .. } => row_height,
                MenuEntry::Separator => SEPARATOR_HEIGHT,
            })
            .sum::<i32>()
            + 2;
        let x = i32::from(self.anchor.x)
            .min(area.right() - width)
            .max(area.x);
        let y = i32::from(self.anchor.y)
            .min(area.bottom() - height)
            .max(area.y);
        let window = Rect {
            x,
            y,
            width,
            height,
        };
        let mut top = y + 1;
        let rows = self
            .entries
            .iter()
            .map(|entry| {
                let height = match entry {
                    MenuEntry::Item { .. } => row_height,
                    MenuEntry::Separator => SEPARATOR_HEIGHT,
                };
                let row = Rect {
                    x: x + 1,
                    y: top,
                    width: width - 2,
                    height,
                };
                top += height;
                row
            })
            .collect();
        MenuLayout {
            window,
            rows,
            cell_w,
        }
    }

    /// Applies `event`. Every input is taken while the menu is open.
    ///
    /// Up and Down highlight the previous or next item that can be chosen,
    /// wrapping around, and Enter chooses the highlighted one. The pointer
    /// highlights the item under it, and a primary press on an item that
    /// can be chosen chooses it. Escape closes the menu, and so does a
    /// press outside it.
    pub fn handle_input(&mut self, event: InputEvent, layout: &MenuLayout) -> MenuOutcome<A> {
        match event {
            InputEvent::Break => return MenuOutcome::Close,
            InputEvent::Up { .. } | InputEvent::Tab { reverse: true } => self.step(-1),
            InputEvent::Down { .. } | InputEvent::Tab { reverse: false } => self.step(1),
            InputEvent::Enter => {
                if let Some(action) = self.focus.and_then(|index| self.action(index)) {
                    return MenuOutcome::Run(action);
                }
            }
            InputEvent::PointerMove(Some(at)) => {
                if let Some(index) = layout.entry_at(at) {
                    self.focus = self.entries[index].is_enabled().then_some(index);
                }
            }
            InputEvent::PointerDown { button, at, .. } => {
                if !layout.window.contains(at) {
                    return MenuOutcome::Outside;
                }
                if button == PointerButton::Primary
                    && let Some(action) = layout.entry_at(at).and_then(|index| self.action(index))
                {
                    return MenuOutcome::Run(action);
                }
            }
            _ => {}
        }
        MenuOutcome::Stay
    }

    /// Whether a press at `point` would choose an item.
    pub fn chooses_at(&self, point: Point, layout: &MenuLayout) -> bool {
        layout
            .entry_at(point)
            .is_some_and(|index| self.entries[index].is_enabled())
    }

    /// The action of entry `index`, if it can be chosen.
    fn action(&self, index: usize) -> Option<A> {
        match self.entries.get(index)? {
            MenuEntry::Item {
                enabled: true,
                action,
                ..
            } => Some(*action),
            _ => None,
        }
    }

    /// Highlights the next item that can be chosen, `direction` 1 for down
    /// and -1 for up, wrapping around.
    fn step(&mut self, direction: isize) {
        let len = self.entries.len() as isize;
        let start = match self.focus {
            Some(index) => index as isize,
            // From nothing highlighted, Down finds the first item and Up
            // the last.
            None if direction > 0 => -1,
            None => len,
        };
        self.focus = (1..=len)
            .map(|offset| (start + direction * offset).rem_euclid(len) as usize)
            .find(|&index| self.entries[index].is_enabled());
    }

    /// Draws the menu.
    pub fn render(&self, framebuffer: &mut Framebuffer, font: &BitmapFont, layout: &MenuLayout) {
        let fill = |framebuffer: &mut Framebuffer, rect: Rect, color| {
            fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, color);
        };
        fill(framebuffer, layout.window, DIM);
        fill(framebuffer, layout.window.inset(1), BAR);
        for (index, (entry, &row)) in self.entries.iter().zip(&layout.rows).enumerate() {
            match entry {
                MenuEntry::Separator => {
                    let rule = Rect {
                        x: row.x + 2,
                        y: row.y + row.height / 2,
                        width: row.width - 4,
                        height: 1,
                    };
                    fill(framebuffer, rule, DIM);
                }
                MenuEntry::Item {
                    label,
                    shortcut,
                    enabled,
                    ..
                } => {
                    let focused = self.focus == Some(index);
                    if focused {
                        fill(framebuffer, row, HOVER_SHADE);
                    }
                    let (ink, key_ink) = match (enabled, focused) {
                        (false, _) => (IDLE_SHADE, IDLE_SHADE),
                        (true, true) => (TEXT, PALE),
                        (true, false) => (PALE, DIM),
                    };
                    let y = row.y + 1;
                    draw_text(
                        framebuffer,
                        font,
                        label,
                        row.x + LABEL_INDENT * layout.cell_w,
                        y,
                        ink,
                    );
                    if let Some(shortcut) = shortcut {
                        let cells = i32::try_from(font.text_cells(shortcut)).unwrap_or(0);
                        let x = row.right() - (cells + 1) * layout.cell_w;
                        draw_text(framebuffer, font, shortcut, x, y, key_ink);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
