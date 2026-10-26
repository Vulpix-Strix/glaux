//! The SYM panel: the declarations of the open document, docked at the right of the
//! text. It follows the caret while the text has the keys, and takes them once opened
//! from the keyboard so a symbol can be picked.

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_syntax::{Symbol, SymbolKind};

use super::layout::CodeLayout;
use crate::system::editing::document::DocumentId;
use crate::system::editing::session::{Destination, Spot};
use crate::system::workspace::Rect;

/// Width of the panel, as drawn in the design for the 320x240 screen.
pub const WIDTH: i32 = 72;
/// Space between the panel's edge and its rows.
pub const PAD: i32 = 3;

/// A declaration with the line it is on, counted from 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub symbol: Symbol,
    pub line: usize,
}

/// The panel while it is open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SymbolPanel {
    /// Whether the keys pick a symbol instead of editing the text.
    pub(super) focused: bool,
    /// Index into `rows` of the highlighted symbol.
    selected: usize,
    /// Index into `rows` of the first row shown.
    scroll: usize,
    rows: Vec<Row>,
    /// The document and text `rows` were read from.
    source: Option<(DocumentId, u64)>,
}

/// What the panel asks of the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelAction {
    /// Move to this declaration.
    Go(usize),
    /// The panel used the event.
    Handled,
    /// The panel has nothing to do with the event.
    Ignored,
}

/// Where the panel's parts sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelLayout {
    pub area: Rect,
    pub title: Rect,
    /// The first row's rectangle. Every row has the same size, one below the other.
    pub first_row: Rect,
    pub visible: usize,
}

impl PanelLayout {
    /// The panel at the right edge of `layout`'s text area.
    pub fn new(layout: &CodeLayout) -> Self {
        let text = layout.text;
        let area = Rect {
            x: text.right() - WIDTH,
            y: text.y,
            width: WIDTH,
            height: text.height,
        };
        let cell_h = layout.metrics.cell_h;
        let title = Rect {
            height: cell_h + 2,
            ..area
        };
        let first_row = Rect {
            y: title.bottom() + 1,
            height: cell_h,
            ..area
        };
        let visible = usize::try_from((area.bottom() - first_row.y) / cell_h.max(1)).unwrap_or(0);
        Self {
            area,
            title,
            first_row,
            visible,
        }
    }

    /// The rectangle of the `n`th visible row.
    pub fn row(&self, n: usize) -> Rect {
        Rect {
            y: self.first_row.y + n as i32 * self.first_row.height,
            ..self.first_row
        }
    }

    /// The visible row `at` is over.
    fn row_at(&self, at: Point) -> Option<usize> {
        let (x, y) = (i32::from(at.x), i32::from(at.y));
        let inside = self.area.contains(at) && y >= self.first_row.y;
        let n = usize::try_from((y - self.first_row.y) / self.first_row.height.max(1)).ok()?;
        (inside && x >= self.area.x && n < self.visible).then_some(n)
    }
}

impl SymbolPanel {
    pub fn new() -> Self {
        Self {
            focused: true,
            ..Self::default()
        }
    }

    pub fn focus(&mut self) {
        self.focused = true;
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    /// What the panel currently lists, to tell when it needs reading again.
    pub fn source(&self) -> Option<(DocumentId, u64)> {
        self.source
    }

    /// Lists `rows`, read from `source`, keeping the selection in range.
    pub fn set_rows(&mut self, source: Option<(DocumentId, u64)>, rows: Vec<Row>) {
        self.rows = rows;
        self.source = source;
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    /// Highlights the declaration the caret at `caret` is inside, which is the last one
    /// that starts at or before it. Does nothing while the panel has the keys, since the
    /// highlight is then the user's.
    pub fn follow(&mut self, caret: usize, visible: usize) {
        if self.focused {
            return;
        }
        let at = self
            .rows
            .partition_point(|row| row.symbol.declared.start <= caret);
        self.selected = at.saturating_sub(1);
        self.reveal(visible);
    }

    fn reveal(&mut self, visible: usize) {
        let visible = visible.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + visible {
            self.scroll = self.selected + 1 - visible;
        }
        self.scroll = self.scroll.min(self.rows.len().saturating_sub(visible));
    }

    fn select(&mut self, index: usize, visible: usize) {
        self.selected = index.min(self.rows.len().saturating_sub(1));
        self.reveal(visible);
    }

    /// The target of the `index`th symbol in the document at `path`.
    pub fn target(
        &self,
        index: usize,
        path: &crate::cartridge::source::SourcePath,
    ) -> Option<Destination> {
        let row = self.rows.get(index)?;
        Some(Destination {
            path: path.clone(),
            at: Spot::Offset(row.symbol.declared.start),
        })
    }

    /// Applies `event`. `pointer` is where the pointer last was, which decides what the
    /// wheel scrolls.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        layout: &PanelLayout,
        pointer: Option<Point>,
    ) -> PanelAction {
        let visible = layout.visible;
        match event {
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                if !layout.area.contains(at) {
                    self.focused = false;
                    return PanelAction::Ignored;
                }
                match layout.row_at(at).map(|row| self.scroll + row) {
                    Some(index) if index < self.rows.len() => {
                        self.select(index, visible);
                        PanelAction::Go(index)
                    }
                    _ => {
                        self.focused = true;
                        PanelAction::Handled
                    }
                }
            }
            InputEvent::Wheel { delta } if pointer.is_some_and(|at| layout.area.contains(at)) => {
                let step = isize::from(-delta) * 2;
                let last = self.rows.len().saturating_sub(visible);
                self.scroll = self.scroll.saturating_add_signed(step).min(last);
                PanelAction::Handled
            }
            _ if !self.focused => PanelAction::Ignored,
            InputEvent::Up { .. } => {
                self.select(self.selected.saturating_sub(1), visible);
                PanelAction::Handled
            }
            InputEvent::Down { .. } => {
                self.select(self.selected + 1, visible);
                PanelAction::Handled
            }
            InputEvent::PageUp { .. } => {
                self.select(self.selected.saturating_sub(visible), visible);
                PanelAction::Handled
            }
            InputEvent::PageDown { .. } => {
                self.select(self.selected + visible, visible);
                PanelAction::Handled
            }
            InputEvent::DocumentStart { .. } | InputEvent::Home { .. } => {
                self.select(0, visible);
                PanelAction::Handled
            }
            InputEvent::DocumentEnd { .. } | InputEvent::End { .. } => {
                self.select(usize::MAX, visible);
                PanelAction::Handled
            }
            InputEvent::Enter if self.selected < self.rows.len() => PanelAction::Go(self.selected),
            InputEvent::Break => {
                self.focused = false;
                PanelAction::Handled
            }
            // The list has the keys, so nothing typed may reach the document.
            InputEvent::Text(_)
            | InputEvent::Enter
            | InputEvent::Backspace
            | InputEvent::Delete
            | InputEvent::Tab { .. }
            | InputEvent::Left { .. }
            | InputEvent::Right { .. } => PanelAction::Handled,
            _ => PanelAction::Ignored,
        }
    }
}

/// The letter that marks a symbol's kind in its row.
pub fn kind_letter(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "f",
        SymbolKind::Variable => "v",
        SymbolKind::Constant => "c",
    }
}

/// `name` cut to `cells` of `font`'s cells, with an ellipsis when it was longer.
pub fn clip_name(font: &BitmapFont, name: &str, cells: usize) -> String {
    super::super::fit_name(font, name, cells)
        .unwrap_or_else(|| super::super::take_cells(font, name, cells).to_owned())
}

#[cfg(test)]
mod tests;
