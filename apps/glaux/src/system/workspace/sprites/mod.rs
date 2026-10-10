//! The sprite editor: draws on the loaded cartridge's object tiles in
//! memory, undoes and redoes those edits, and asks for them to be saved.
//! Its palettes are shown but not edited.

mod layout;
mod render;
mod sheet;
mod stroke;

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::obj_palette::OBJ_PALETTE_COLORS;
use glaux_machine::palette::Palette;
use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_GRID_ROWS, ObjVram};
use glaux_video::preview::ObjPreviewPalette;

pub use layout::Metrics;
use layout::{Hit, SpriteLayout, ToolSlot};
use sheet::{SheetView, grid_position};

use crate::system::sprite_document::SpriteDocument;

/// The cartridge state the editor shows, borrowed for one frame.
#[derive(Debug, Clone, Copy)]
pub struct SpriteSource<'a> {
    pub obj: &'a ObjVram,
    /// The cartridge's master palette, for the colors Advanced mode names.
    pub master: &'a Palette,
    /// Where the frame palette holds each master color.
    pub preview: &'a ObjPreviewPalette,
    /// Whether the tiles differ from the cartridge's files.
    pub modified: bool,
}

/// What the editor asks of the workspace after an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpriteRequest {
    /// Save the document to the cartridge.
    Save,
    /// Tell the user something briefly, such as that there is nothing to
    /// undo.
    Notice(&'static str),
}

/// A drag of a tool across the canvas, begun by a primary press on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stroke {
    /// The tile pixel the drag last reached, which may lie off the tile.
    last: (i32, i32),
}

/// The sprite editor's UI state. It holds no cartridge data, so it stays
/// valid across tab switches and trips to the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteEditor {
    metrics: Metrics,
    selected: usize,
    sheet: SheetView,
    grid: bool,
    advanced: bool,
    /// The selected local color, 0–15.
    color: u8,
    tool: ToolSlot,
    /// The drag in progress, if any. It only continues while its press is
    /// known to be held, and ends on anything that casts doubt on that.
    stroke: Option<Stroke>,
}

impl SpriteEditor {
    /// An editor on tile 0 with the pixel grid shown, laid out below a
    /// bar ending at `metrics.top`.
    pub fn new(metrics: Metrics) -> Self {
        Self {
            metrics,
            selected: 0,
            sheet: SheetView::default(),
            grid: true,
            advanced: false,
            color: 1,
            tool: ToolSlot::default(),
            stroke: None,
        }
    }

    #[cfg(test)]
    pub fn selected(&self) -> usize {
        self.selected
    }

    #[cfg(test)]
    pub fn is_advanced(&self) -> bool {
        self.advanced
    }

    /// Ends the drag in progress, if any, closing its edit in `document`.
    /// What it drew stays, undoable as one edit.
    pub fn end_stroke(&mut self, document: Option<&mut SpriteDocument>) {
        self.stroke = None;
        if let Some(document) = document {
            document.end_edit();
        }
    }

    /// Where the enlarged tile is drawn.
    #[cfg(test)]
    pub fn canvas_rect(&self) -> crate::system::workspace::Rect {
        self.layout().canvas
    }

    /// Where the Save button is drawn.
    #[cfg(test)]
    pub fn save_button_rect(&self) -> crate::system::workspace::Rect {
        self.layout().save_button
    }

    fn layout(&self) -> SpriteLayout {
        SpriteLayout::new(self.metrics, self.advanced, self.sheet.zoom())
    }

    /// Applies `event`, drawing on the selected tile of `document` when
    /// there is one. Returns what the workspace should do about it, if
    /// anything.
    ///
    /// A press on the canvas uses the selected tool there, and the pencil,
    /// eraser and eyedropper keep acting along a drag until its release;
    /// each drag or fill is one undoable edit. A click elsewhere selects a
    /// sheet tile, a palette color, a tool or an options button, or asks to
    /// save. The wheel scrolls the sheet's rows, and Shift with the wheel
    /// pans its columns. Arrow keys move the selection; `p`, `e`, `i` and
    /// `f` pick a tool, `g` toggles the grid, `a` Advanced mode, and `+`
    /// and `-` zoom the sheet. Undo, redo and save first end any drag, so
    /// it counts as one finished edit.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        mut document: Option<&mut SpriteDocument>,
    ) -> Option<SpriteRequest> {
        let layout = self.layout();
        match event {
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
            } => {
                // A press while a drag is open means its release was lost.
                self.end_stroke(document.as_deref_mut());
                match layout.hit(at, &self.sheet) {
                    Hit::Canvas { .. } => {
                        if let Some(document) = document {
                            self.press(document, layout.canvas_pixel(at));
                        }
                    }
                    // A tile under the pointer is on screen, so selecting
                    // it never scrolls.
                    Hit::Sheet(tile) => self.select(tile, document),
                    Hit::Swatch(local) => self.color = local,
                    Hit::Tool(tool) => self.set_tool(tool, document),
                    Hit::GridButton => self.grid = !self.grid,
                    Hit::AdvancedButton => self.toggle_advanced(),
                    Hit::ZoomOut => self.zoom(-1),
                    Hit::ZoomIn => self.zoom(1),
                    Hit::SaveButton => return self.save(document),
                    Hit::Bank(_) | Hit::None => {}
                }
            }
            InputEvent::PointerMove(Some(at)) => {
                if let Some(document) = document {
                    self.drag(document, at);
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                at,
            } => {
                if let Some(document) = document.as_deref_mut() {
                    self.drag(document, at);
                }
                self.end_stroke(document);
            }
            InputEvent::PointerCancel => self.end_stroke(document),
            InputEvent::Wheel { delta } => {
                let rows = -i32::from(delta) * self.sheet.zoom().wheel_rows();
                self.sheet.scroll_rows(rows, layout.visible);
            }
            InputEvent::WheelHorizontal { delta } => {
                self.sheet.scroll_columns(-i32::from(delta), layout.visible);
            }
            InputEvent::Left { .. } => self.step(-1, 0, document),
            InputEvent::Right { .. } => self.step(1, 0, document),
            InputEvent::Up => self.step(0, -1, document),
            InputEvent::Down => self.step(0, 1, document),
            InputEvent::Text('g' | 'G') => self.grid = !self.grid,
            InputEvent::Text('a' | 'A') => self.toggle_advanced(),
            InputEvent::Text('+' | '=') => self.zoom(1),
            InputEvent::Text('-' | '_') => self.zoom(-1),
            InputEvent::Text(key) => {
                if let Some(tool) = ToolSlot::from_key(key) {
                    self.set_tool(tool, document);
                }
            }
            InputEvent::Save => return self.save(document),
            InputEvent::Undo => {
                self.end_stroke(document.as_deref_mut());
                if !document.is_some_and(SpriteDocument::undo) {
                    return Some(SpriteRequest::Notice("Nothing to undo"));
                }
            }
            InputEvent::Redo => {
                self.end_stroke(document.as_deref_mut());
                if !document.is_some_and(SpriteDocument::redo) {
                    return Some(SpriteRequest::Notice("Nothing to redo"));
                }
            }
            _ => {}
        }
        None
    }

    /// Ends any drag, then asks to save `document` if it has changes.
    fn save(&mut self, mut document: Option<&mut SpriteDocument>) -> Option<SpriteRequest> {
        self.end_stroke(document.as_deref_mut());
        match document {
            Some(document) if document.is_modified() => Some(SpriteRequest::Save),
            Some(_) => Some(SpriteRequest::Notice("Nothing to save")),
            None => None,
        }
    }

    /// Uses the tool at tile pixel `at`, starting a drag if the tool keeps
    /// acting while dragged.
    fn press(&mut self, document: &mut SpriteDocument, at: (i32, i32)) {
        // A fill records itself as one edit; a drag's edit stays open
        // until the stroke ends.
        if matches!(self.tool, ToolSlot::Pencil | ToolSlot::Eraser) {
            document.begin_edit();
        }
        self.apply(document, at);
        if self.tool.drags() {
            self.stroke = Some(Stroke { last: at });
        }
    }

    /// Continues the drag in progress to the pixel under `at`, using the
    /// tool on every pixel between, so fast moves leave no gaps.
    fn drag(&mut self, document: &mut SpriteDocument, at: Point) {
        let Some(stroke) = self.stroke else {
            return;
        };
        let to = self.layout().canvas_pixel(at);
        for pixel in stroke::line(stroke.last, to).skip(1) {
            self.apply(document, pixel);
        }
        self.stroke = Some(Stroke { last: to });
    }

    /// Uses the tool on tile pixel `at` of the selected tile. Pixels off
    /// the tile are ignored.
    fn apply(&mut self, document: &mut SpriteDocument, (x, y): (i32, i32)) {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return;
        };
        let tile = self.selected;
        match self.tool {
            ToolSlot::Pencil => {
                document.set_pixel(tile, x, y, self.color());
            }
            ToolSlot::Eraser => {
                document.set_pixel(tile, x, y, 0);
            }
            ToolSlot::Eyedropper => {
                if let Some(local) = document.pixel(tile, x, y) {
                    self.color = local;
                }
            }
            ToolSlot::Fill => {
                document.fill(tile, x, y, self.color());
            }
        }
    }

    fn set_tool(&mut self, tool: ToolSlot, document: Option<&mut SpriteDocument>) {
        self.end_stroke(document);
        self.tool = tool;
    }

    /// Selects `tile`. A drag never carries over to another tile.
    fn select(&mut self, tile: usize, document: Option<&mut SpriteDocument>) {
        if tile != self.selected {
            self.end_stroke(document);
        }
        self.selected = tile;
    }

    /// Moves the selection by whole grid cells, stopping at the grid's
    /// edges, and scrolls just enough to keep it on screen.
    fn step(&mut self, columns: i32, rows: i32, document: Option<&mut SpriteDocument>) {
        let (column, row) = grid_position(self.selected);
        let column = (column as i32 + columns).clamp(0, OBJ_GRID_COLUMNS as i32 - 1) as usize;
        let row = (row as i32 + rows).clamp(0, OBJ_GRID_ROWS as i32 - 1) as usize;
        self.select(row * OBJ_GRID_COLUMNS + column, document);
        self.sheet.reveal(self.selected, self.layout().visible);
    }

    /// Advanced mode changes the sheet's height; the sheet keeps its
    /// scroll position as far as the new height allows.
    fn toggle_advanced(&mut self) {
        self.advanced = !self.advanced;
        self.sheet.clamp(self.layout().visible);
    }

    fn zoom(&mut self, direction: i32) {
        let current = self.sheet.zoom();
        let next = if direction > 0 {
            current.zoomed_in()
        } else {
            current.zoomed_out()
        };
        if let Some(zoom) = next {
            let visible = SpriteLayout::new(self.metrics, self.advanced, zoom).visible;
            self.sheet.set_zoom(zoom, visible, self.selected);
        }
    }

    /// The selected local color, always below [`OBJ_PALETTE_COLORS`].
    fn color(&self) -> u8 {
        debug_assert!(usize::from(self.color) < OBJ_PALETTE_COLORS);
        self.color
    }
}

#[cfg(test)]
mod tests;
