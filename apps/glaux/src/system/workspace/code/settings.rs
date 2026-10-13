//! EDITOR SETTINGS: the window choosing how the Code view shows text.
//!
//! Each choice applies the moment it is made, so the view behind shows its
//! effect; closing the window keeps whatever was chosen.

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::draw_text;

use crate::system::editor_settings::EditorSettings;
use crate::system::workspace::Rect;
use crate::system::workspace::dialog::{
    AREA_MARGIN, BODY, INK, PAD, SHADOW_OFFSET, draw_button, draw_window, fill, text_cells,
};

const TITLE: &str = "EDITOR SETTINGS";
const DONE: &str = "Done";
/// Side of a checkbox.
const CHECKBOX: i32 = 7;
/// Cells taken left of a row's label by its checkbox.
const LABEL_INDENT: i32 = 2;
/// Rows above Done.
const SETTING_ROWS: usize = Row::ALL.len() - 1;

/// One line of the window, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    WordWrap,
    TabSize,
    LineNumbers,
    Ligatures,
    Done,
}

impl Row {
    const ALL: [Self; 5] = [
        Self::WordWrap,
        Self::TabSize,
        Self::LineNumbers,
        Self::Ligatures,
        Self::Done,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::WordWrap => "Word wrap",
            Self::TabSize => "Tab size",
            Self::LineNumbers => "Show line numbers",
            Self::Ligatures => "Ligatures",
            Self::Done => DONE,
        }
    }

    /// Position in [`ALL`](Self::ALL), which lists rows in declaration
    /// order.
    fn index(self) -> usize {
        self as usize
    }
}

/// The open window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsDialog {
    focus: Row,
    /// The settings when the window opened, to tell whether closing it
    /// changed anything.
    opened_with: EditorSettings,
    /// Whether a primary press started on Done. Releasing on it closes.
    armed: bool,
}

/// What an input did to the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    /// Stay open.
    None,
    /// Close the window, keeping every change made in it.
    Close,
}

impl SettingsDialog {
    pub fn new(settings: EditorSettings) -> Self {
        Self {
            focus: Row::WordWrap,
            opened_with: settings,
            armed: false,
        }
    }

    #[cfg(test)]
    pub fn focus(&self) -> Row {
        self.focus
    }

    /// The settings the window opened with.
    pub fn opened_with(&self) -> EditorSettings {
        self.opened_with
    }

    /// Applies `event` to `settings`, the ones the view shows. Every input
    /// is taken, so nothing reaches what lies under the window.
    ///
    /// Up and Down move between rows. Enter or Space toggles a checkbox,
    /// Left and Right change the tab size, and Escape or Done closes. A
    /// primary press toggles a checkbox or steps the tab size with its
    /// arrows; releasing on Done after pressing it closes.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        settings: &mut EditorSettings,
        layout: &SettingsLayout,
    ) -> SettingsAction {
        let last = Row::ALL.len() - 1;
        match event {
            InputEvent::Break => return SettingsAction::Close,
            InputEvent::Up => self.focus = Row::ALL[self.focus.index().saturating_sub(1)],
            InputEvent::Down | InputEvent::Tab => {
                self.focus = Row::ALL[(self.focus.index() + 1).min(last)];
            }
            InputEvent::Left { .. } if self.focus == Row::TabSize => settings.step_tab_size(-1),
            InputEvent::Right { .. } if self.focus == Row::TabSize => settings.step_tab_size(1),
            InputEvent::Enter | InputEvent::Text(' ') => {
                if self.focus == Row::Done {
                    return SettingsAction::Close;
                }
                toggle(self.focus, settings);
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
            } => {
                self.armed = false;
                if layout.minus.contains(at) {
                    self.focus = Row::TabSize;
                    settings.step_tab_size(-1);
                } else if layout.plus.contains(at) {
                    self.focus = Row::TabSize;
                    settings.step_tab_size(1);
                } else if let Some(row) = layout.row_at(at) {
                    self.focus = row;
                    self.armed = row == Row::Done;
                    toggle(row, settings);
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                at,
            } => {
                if std::mem::take(&mut self.armed) && layout.done.contains(at) {
                    return SettingsAction::Close;
                }
            }
            InputEvent::PointerCancel => self.armed = false,
            _ => {}
        }
        SettingsAction::None
    }

    /// Lays the window out centred in `area`.
    pub fn layout(area: Rect, cell_w: i32, cell_h: i32) -> SettingsLayout {
        let cell_w = cell_w.max(1);
        let label_cells = Row::ALL
            .iter()
            .map(|row| LABEL_INDENT + text_cells(row.label()))
            .max()
            .unwrap_or(0);
        // "Tab size" followed by "< 8 >" two cells after it.
        let stepper_cells = 5;
        let inner_cells = (label_cells + 1)
            .max(LABEL_INDENT + text_cells(Row::TabSize.label()) + 2 + stepper_cells)
            .max(text_cells(TITLE));
        let inner_width = (inner_cells * cell_w).min(area.width - 2 * AREA_MARGIN - 2 * PAD);
        let row_height = cell_h + 2;
        let button_height = cell_h + 4;
        let title_height = cell_h + 2;
        let width = inner_width + 2 * PAD + 2;
        let height = 1
            + title_height
            + PAD
            + SETTING_ROWS as i32 * row_height
            + cell_h
            + button_height
            + PAD
            + 1;
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
        let x = window.x + 1 + PAD;
        let top = title_bar.bottom() + PAD;
        let rows = [0, 1, 2, 3].map(|n| Rect {
            x,
            y: top + n * row_height,
            width: inner_width,
            height: row_height,
        });
        let tab = rows[1];
        let arrow = |cells_from_right: i32| Rect {
            x: tab.right() - cells_from_right * cell_w - 2,
            y: tab.y,
            width: cell_w + 2,
            height: tab.height,
        };
        let done_width = (text_cells(DONE) + 4) * cell_w;
        SettingsLayout {
            window,
            title_bar,
            rows,
            minus: arrow(stepper_cells),
            plus: arrow(1),
            done: Rect {
                x: window.x + (width - done_width) / 2,
                y: rows[SETTING_ROWS - 1].bottom() + cell_h,
                width: done_width,
                height: button_height,
            },
            cell_w,
            cell_h,
        }
    }

    /// Draws the window showing `settings`.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        layout: &SettingsLayout,
        settings: &EditorSettings,
    ) {
        let SettingsLayout { cell_w, cell_h, .. } = *layout;
        draw_window(
            framebuffer,
            font,
            layout.window,
            layout.title_bar,
            TITLE,
            cell_w,
        );
        for (&rect, row) in layout.rows.iter().zip(Row::ALL) {
            let focused = row == self.focus;
            let (ground, ink) = if focused { (INK, BODY) } else { (BODY, INK) };
            fill(framebuffer, rect, ground);
            let text_y = rect.y + 2;
            let label_x = rect.x + 2 + LABEL_INDENT * cell_w;
            draw_text(framebuffer, font, row.label(), label_x, text_y, ink);
            match row {
                Row::WordWrap | Row::LineNumbers | Row::Ligatures => {
                    let checked = match row {
                        Row::WordWrap => settings.word_wrap,
                        Row::LineNumbers => settings.line_numbers,
                        _ => settings.ligatures,
                    };
                    let rect = Rect {
                        x: rect.x + 2,
                        y: rect.y + (rect.height - CHECKBOX) / 2,
                        width: CHECKBOX,
                        height: CHECKBOX,
                    };
                    fill(framebuffer, rect, ink);
                    fill(framebuffer, rect.inset(1), ground);
                    if checked {
                        fill(framebuffer, rect.inset(2), ink);
                    }
                }
                Row::TabSize => {
                    let value = settings.tab_size.to_string();
                    let value = value.as_str();
                    draw_text(framebuffer, font, "<", layout.minus.x + 1, text_y, ink);
                    draw_text(framebuffer, font, ">", layout.plus.x + 1, text_y, ink);
                    let between = layout.plus.x - layout.minus.right();
                    let x = layout.minus.right() + (between - text_cells(value) * cell_w) / 2;
                    draw_text(framebuffer, font, value, x, text_y, ink);
                }
                Row::Done => {}
            }
        }
        draw_button(
            framebuffer,
            font,
            layout.done,
            DONE,
            self.focus == Row::Done,
            cell_w,
            cell_h,
        );
    }
}

/// Flips the checkbox on `row`, if it has one.
fn toggle(row: Row, settings: &mut EditorSettings) {
    match row {
        Row::WordWrap => settings.word_wrap = !settings.word_wrap,
        Row::LineNumbers => settings.line_numbers = !settings.line_numbers,
        Row::Ligatures => settings.ligatures = !settings.ligatures,
        Row::TabSize | Row::Done => {}
    }
}

/// Where the window's parts sit. Rendering and hit-testing both read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsLayout {
    pub window: Rect,
    pub title_bar: Rect,
    /// Word wrap, tab size, line numbers and ligatures, top to bottom.
    pub rows: [Rect; SETTING_ROWS],
    /// The arrows stepping the tab size down and up.
    pub minus: Rect,
    pub plus: Rect,
    pub done: Rect,
    cell_w: i32,
    cell_h: i32,
}

impl SettingsLayout {
    /// The row under `point`, Done included.
    pub fn row_at(&self, point: Point) -> Option<Row> {
        if self.done.contains(point) {
            return Some(Row::Done);
        }
        self.rows
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| Row::ALL[index])
    }
}

#[cfg(test)]
mod tests;
