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
use glaux_machine::system_rom::SystemIcon;
use glaux_video::draw::{draw_ink_image_centered, draw_text, fill_rect};

use super::{BAR, HOVER_SHADE, IDLE_SHADE, OUTLINE, Rect, take_cells};
use crate::system::screen::{ACCENT, DIM, ERROR, PALE, TEXT};
use crate::system::workspace::RenderContext;
use crate::ui_text;

/// The status bar hint while a context menu is open.
pub fn menu_hint() -> String {
    ui_text::get("workspace-up-down-pick-enter-run-esc-close")
}
/// Height of a separator row.
const SEPARATOR_HEIGHT: i32 = 5;
/// Cells left of a label.
const LABEL_INDENT: i32 = 1;
const CHECK_INDENT: i32 = 3;
/// Cells at least between a label and its shortcut.
const SHORTCUT_GAP: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemStyle {
    Plain,
    Current,
    Danger,
}

/// One row of a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry<A> {
    Item {
        label: String,
        /// The keys that do the same, shown at the row's right end.
        shortcut: Option<&'static str>,
        enabled: bool,
        style: ItemStyle,
        checked: Option<bool>,
        action: A,
    },
    Header(String),
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
            style: ItemStyle::Plain,
            checked: None,
            action,
        }
    }

    /// An item that shows but can't be chosen unless `enabled`.
    pub fn when(enabled: bool, label: &str, shortcut: Option<&'static str>, action: A) -> Self {
        Self::Item {
            label: label.to_owned(),
            shortcut,
            enabled,
            style: ItemStyle::Plain,
            checked: None,
            action,
        }
    }

    pub fn header(label: &str) -> Self {
        Self::Header(label.to_owned())
    }

    pub fn checked(mut self, on: bool) -> Self {
        if let Self::Item { checked, .. } = &mut self {
            *checked = Some(on);
        }
        self
    }

    pub fn styled(mut self, new: ItemStyle) -> Self {
        if let Self::Item { style, .. } = &mut self {
            *style = new;
        }
        self
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
    max_height: Option<i32>,
    top: usize,
}

/// Where a menu's parts sit for one area and font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuLayout {
    pub window: Rect,
    /// Each entry's row, in order.
    pub rows: Vec<Rect>,
    pub first: usize,
    cell_w: i32,
    indent: i32,
    heights: Vec<i32>,
    room: Option<i32>,
}

impl MenuLayout {
    /// The entry whose row is under `point`.
    pub fn entry_at(&self, point: Point) -> Option<usize> {
        let shown = self.rows.iter().position(|row| row.contains(point))?;
        Some(self.first + shown)
    }
}

impl<A: Copy> ContextMenu<A> {
    /// A menu of `entries` opened at `anchor`, nothing highlighted.
    pub fn new(entries: Vec<MenuEntry<A>>, anchor: Point) -> Self {
        Self {
            entries,
            anchor,
            focus: None,
            max_height: None,
            top: 0,
        }
    }

    pub fn limited_to(mut self, height: i32) -> Self {
        self.max_height = Some(height);
        self
    }

    pub fn focused_on(mut self, index: usize) -> Self {
        self.focus = self.entries.get(index).filter(|e| e.is_enabled()).map(|_| index);
        self
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
            match entry {
                MenuEntry::Item {
                    label, shortcut, ..
                } => {
                    labels = labels.max(cells(label));
                    shortcuts = shortcuts.max(shortcut.map_or(0, cells));
                }
                MenuEntry::Header(label) => labels = labels.max(cells(label)),
                MenuEntry::Separator => {}
            }
        }
        let indent = if self
            .entries
            .iter()
            .any(|entry| matches!(entry, MenuEntry::Item { checked: Some(_), .. }))
        {
            CHECK_INDENT
        } else {
            LABEL_INDENT
        };
        let gap = if shortcuts > 0 { SHORTCUT_GAP } else { 0 };
        let row_height = cell_h + 2;
        let heights: Vec<i32> = self
            .entries
            .iter()
            .map(|entry| match entry {
                MenuEntry::Item { .. } | MenuEntry::Header(_) => row_height,
                MenuEntry::Separator => SEPARATOR_HEIGHT,
            })
            .collect();
        let width = ((indent + labels + gap + shortcuts + 1) * cell_w + 2).min(area.width);
        let room = self.max_height.map(|height| height - 2);
        let first = self.first_shown(&heights, room);
        let shown = shown_from(first, &heights, room);
        let height = heights[first..first + shown].iter().sum::<i32>() + 2;
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
        let rows = heights[first..first + shown]
            .iter()
            .map(|&height| {
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
            first,
            cell_w,
            indent,
            heights,
            room,
        }
    }

    fn first_shown(&self, heights: &[i32], room: Option<i32>) -> usize {
        let Some(room) = room else { return 0 };
        let last = heights.len().saturating_sub(1);
        let mut first = self.top.min(last);
        match self.focus {
            Some(focus) if focus < first => first = focus,
            Some(focus) => {
                while first < focus && heights[first..=focus].iter().sum::<i32>() > room {
                    first += 1;
                }
            }
            None => {}
        }
        first
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
            InputEvent::Wheel { delta } => self.scroll(-isize::from(delta), layout),
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
        if self.max_height.is_some() {
            self.top = self.first_shown(&layout.heights, layout.room);
        }
        MenuOutcome::Stay
    }

    fn scroll(&mut self, by: isize, layout: &MenuLayout) {
        let Some(room) = layout.room else { return };
        let len = layout.heights.len();
        let last_top = (0..len)
            .find(|&first| shown_from(first, &layout.heights, Some(room)) == len - first)
            .unwrap_or(0);
        let top = layout.first.saturating_add_signed(by).min(last_top);
        let shown = shown_from(top, &layout.heights, Some(room));
        if self.focus.is_some_and(|at| at < top || at >= top + shown) {
            self.focus = None;
        }
        self.top = top;
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
    pub fn render(&self, ctx: &mut RenderContext<'_>, layout: &MenuLayout) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let fill = |framebuffer: &mut Framebuffer, rect: Rect, color| {
            fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, color);
        };
        fill(framebuffer, layout.window, DIM);
        fill(framebuffer, layout.window.inset(1), BAR);
        let entries = self.entries.iter().enumerate().skip(layout.first);
        for ((index, entry), &row) in entries.zip(&layout.rows) {
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
                MenuEntry::Header(label) => {
                    let reach = usize::try_from(row.width / layout.cell_w - 2).unwrap_or(0);
                    let text = take_cells(font, label, reach);
                    draw_text(
                        framebuffer,
                        font,
                        text,
                        row.x + layout.indent * layout.cell_w,
                        row.y + 1,
                        DIM,
                    );
                }
                MenuEntry::Item {
                    label,
                    shortcut,
                    enabled,
                    style,
                    checked,
                    ..
                } => {
                    let focused = self.focus == Some(index);
                    if focused {
                        fill(framebuffer, row, HOVER_SHADE);
                    }
                    let (ink, key_ink) = match (enabled, focused, style) {
                        (false, ..) => (IDLE_SHADE, IDLE_SHADE),
                        (true, true, ItemStyle::Current) => (TEXT, PALE),
                        (true, false, ItemStyle::Current) => (ACCENT, DIM),
                        (true, true, ItemStyle::Danger) => (TEXT, PALE),
                        (true, false, ItemStyle::Danger) => (ERROR, DIM),
                        (true, true, ItemStyle::Plain) => (TEXT, PALE),
                        (true, false, ItemStyle::Plain) => (PALE, DIM),
                    };
                    let y = row.y + 1;
                    if *checked == Some(true) {
                        draw_ink_image_centered(
                            framebuffer,
                            ctx.ui.icon(SystemIcon::Check),
                            (row.x, row.y, CHECK_INDENT * layout.cell_w, row.height),
                            &[OUTLINE, ink, ink],
                        );
                    }
                    let reach = (row.width / layout.cell_w - layout.indent - 1).max(0);
                    draw_text(
                        framebuffer,
                        font,
                        take_cells(font, label, usize::try_from(reach).unwrap_or(0)),
                        row.x + layout.indent * layout.cell_w,
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
        let total = self.entries.len();
        if layout.rows.len() < total {
            let inner = layout.window.inset(1);
            let thumb = inner.height * layout.rows.len() as i32 / total as i32;
            let at = inner.y + inner.height * layout.first as i32 / total as i32;
            fill(framebuffer, Rect { x: inner.right() - 2, y: at, width: 2, height: thumb }, DIM);
        }
    }
}

fn shown_from(first: usize, heights: &[i32], room: Option<i32>) -> usize {
    let Some(room) = room else {
        return heights.len() - first;
    };
    let mut used = 0;
    let fitting = heights[first..]
        .iter()
        .take_while(|&&height| {
            used += height;
            used <= room
        })
        .count();
    fitting.max(1).min(heights.len() - first)
}

#[cfg(test)]
mod tests;
