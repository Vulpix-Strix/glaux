//! The Map editor's state and input handling.
//!
//! Every change to a map goes through [`MapEditor::write`], the one place
//! that refuses a locked layer.

use glaux_i18n::Arg;
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::tilemap::{BG_LAYERS, BgBank, BgLayer, TileEntry};

use super::attributes::AttrPanel;
use super::frame::{DrawerSize, Frame, Hit, Shape};
use super::pattern::{Clip, stamp};
use super::picker::Picker;
use super::tools::{Selection, Tool, connected, line, would_change};
use super::view::Viewport;
use super::{MapEditor, MapOut, Overlay};
use crate::system::map_document::MapDocument;
use crate::system::workspace::dialog::text_cells;
use crate::ui_text;

const STRIP_ROWS_TOTAL: usize = 64;
const WHEEL_TILES: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LayerAction {
    Edit,
    Rename,
    ToggleVisible,
    ToggleLock,
    ShowOnly,
    SelectAll,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TileAction {
    UseAsBrush,
    EditInSprites,
    SelectOnLayer,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stroke {
    Paint { last: (i32, i32) },
    Select { anchor: (i32, i32) },
    Pan { last: (i32, i32) },
}

/// Editor-only state: hiding or locking a layer never reaches the cartridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Edit {
    pub(super) view: Viewport,
    pub(super) tool: Tool,
    pub(super) layer: BgLayer,
    pub(super) visible: [bool; BG_LAYERS],
    pub(super) locked: [bool; BG_LAYERS],
    pub(super) brush: TileEntry,
    pub(super) pattern: (usize, usize),
    pub(super) picker: Option<Picker>,
    pub(super) attrs: Option<AttrPanel>,
    pub(super) clipboard: Option<Clip>,
    pub(super) drawer: DrawerSize,
    pub(super) strip_top: usize,
    pub(super) grid: bool,
    pub(super) rulers: bool,
    pub(super) stroke: Option<Stroke>,
    pub(super) selection: Option<Selection>,
    /// The wheel carries no position, so the last known one is kept.
    pub(super) pointer: Option<Point>,
}

impl Default for Edit {
    fn default() -> Self {
        Self {
            view: Viewport::default(),
            tool: Tool::default(),
            layer: BgLayer::Bg1,
            visible: [true; BG_LAYERS],
            locked: [false; BG_LAYERS],
            // Tile 1 is the first tile that draws.
            brush: TileEntry::new(1, false, false, false, BgBank::ZERO).unwrap_or(TileEntry::EMPTY),
            pattern: (1, 1),
            picker: None,
            attrs: None,
            clipboard: None,
            drawer: DrawerSize::default(),
            strip_top: 0,
            grid: true,
            rulers: false,
            stroke: None,
            selection: None,
            pointer: None,
        }
    }
}

impl Edit {
    /// Keeps the tool, the brush and the layer states.
    pub(super) fn reset_for_new_map(&mut self) {
        self.view = Viewport::default();
        self.stroke = None;
        self.selection = None;
        self.pointer = None;
    }
}

/// Whether `event` changes the layer, the zoom or what part of the map the
/// canvas shows. A stroke in progress ends first: it would otherwise go on
/// writing on another layer, or join cells the pointer never crossed.
fn changes_what_a_stroke_draws_on(event: &InputEvent) -> bool {
    match event {
        InputEvent::Wheel { .. }
        | InputEvent::WheelHorizontal { .. }
        | InputEvent::Left { .. }
        | InputEvent::Right { .. }
        | InputEvent::Up { .. }
        | InputEvent::Down { .. }
        | InputEvent::PageUp { .. }
        | InputEvent::PageDown { .. }
        | InputEvent::Tab { .. }
        | InputEvent::Function(2) => true,
        InputEvent::Text(key) => matches!(key, 'x' | 'X' | '+' | '=' | '-' | '_' | '0'..='2'),
        _ => false,
    }
}

pub(super) fn map_size(document: &MapDocument) -> (usize, usize) {
    let meta = document.meta();
    (meta.width, meta.height)
}

impl MapEditor {
    pub(super) fn frame(&self) -> Frame {
        Frame::new(
            self.metrics,
            Shape {
                drawer: self.edit.drawer,
                rulers: self.edit.rulers,
                tile: self.edit.view.zoom().tile_px(),
                tool_cells: text_cells(&ui_text::get(self.edit.tool.message_id())),
                locked: self.edit.locked,
            },
        )
    }

    pub(super) fn viewport(&self, frame: &Frame, document: &MapDocument) -> Viewport {
        let mut view = self.edit.view;
        view.clamp(
            map_size(document),
            (frame.canvas.width, frame.canvas.height),
        );
        view
    }

    fn fit(&mut self, document: &MapDocument) {
        let frame = self.frame();
        self.edit.view = self.viewport(&frame, document);
    }

    fn cell_at(&self, frame: &Frame, document: &MapDocument, at: Point) -> (i32, i32) {
        self.viewport(frame, document)
            .cell_at(frame.canvas, (i32::from(at.x), i32::from(at.y)))
    }

    pub(super) fn end_stroke(&mut self, document: Option<&mut MapDocument>) {
        if matches!(self.edit.stroke.take(), Some(Stroke::Paint { .. }))
            && let Some(document) = document
        {
            document.end_edit();
        }
    }

    /// Locked layers, cells off the map and unchanged entries return `false`.
    pub(super) fn write(&self, document: &mut MapDocument, cell: (i32, i32), entry: TileEntry) -> bool {
        let layer = self.edit.layer;
        if self.edit.locked[layer.index()] {
            return false;
        }
        let (Ok(x), Ok(y)) = (usize::try_from(cell.0), usize::try_from(cell.1)) else {
            return false;
        };
        document.set_cell(layer, x, y, entry)
    }

    pub(super) fn locked_notice(&self) -> MapOut {
        MapOut::Notice(ui_text::get_with(
            "maps-layer-locked",
            &[("layer", Arg::Text(&self.edit.layer.to_string()))],
        ))
    }

    /// `None` leaves the event to the rest of the view. `Some(None)` means it was
    /// used and asks nothing of the workspace.
    pub(super) fn editor_input(
        &mut self,
        event: InputEvent,
        document: &mut MapDocument,
    ) -> Option<Option<MapOut>> {
        self.fit(document);
        if changes_what_a_stroke_draws_on(&event) {
            self.end_stroke(Some(&mut *document));
        }
        let frame = self.frame();
        if let Some(handled) = self.picker_input(event, &frame, document) {
            self.fit(document);
            return Some(handled);
        }
        if let Some(handled) = self.attr_input(event, &frame, document) {
            return Some(handled);
        }
        let out = match event {
            InputEvent::PointerMove(at) => {
                self.edit.pointer = at;
                if let Some(at) = at {
                    self.drag(&frame, document, at);
                }
                None
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                // A press while a drag is open means its release was lost.
                self.end_stroke(Some(&mut *document));
                if frame.hit(at, self.edit.strip_top) == Hit::Nothing {
                    return None;
                }
                self.press(&frame, document, at)
            }
            InputEvent::PointerDown {
                button: PointerButton::Secondary,
                at,
                ..
            } => {
                self.end_stroke(Some(&mut *document));
                match frame.hit(at, self.edit.strip_top) {
                    Hit::Chip(layer)
                    | Hit::LayerName(layer)
                    | Hit::Eye(layer)
                    | Hit::Lock(layer) => {
                        self.open_layer_menu(layer, at, document);
                        None
                    }
                    Hit::Tile(tile) => {
                        self.open_tile_menu(tile, at, document);
                        None
                    }
                    Hit::Nothing => return None,
                    _ => None,
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                at,
            } => {
                self.drag(&frame, document, at);
                self.end_stroke(Some(&mut *document));
                None
            }
            InputEvent::PointerCancel => {
                self.end_stroke(Some(&mut *document));
                None
            }
            InputEvent::Wheel { delta } => {
                self.wheel(&frame, document, 0, -i32::from(delta));
                None
            }
            InputEvent::WheelHorizontal { delta } => {
                self.wheel(&frame, document, -i32::from(delta), 0);
                None
            }
            InputEvent::Left { .. } => self.scroll(&frame, document, -1, 0),
            InputEvent::Right { .. } => self.scroll(&frame, document, 1, 0),
            InputEvent::Up { .. } => self.scroll(&frame, document, 0, -1),
            InputEvent::Down { .. } => self.scroll(&frame, document, 0, 1),
            InputEvent::PageUp { .. } => self.scroll_page(&frame, document, -1),
            InputEvent::PageDown { .. } => self.scroll_page(&frame, document, 1),
            InputEvent::Tab { reverse } => {
                self.edit.drawer = self.edit.drawer.cycled(reverse);
                self.fit(document);
                None
            }
            InputEvent::Function(2) => {
                let over_tile = self.edit.pointer.and_then(|at| {
                    match frame.hit(at, self.edit.strip_top) {
                        Hit::Tile(tile) => Some(tile),
                        _ => None,
                    }
                });
                match over_tile {
                    Some(tile) => self.edit_tile_out(tile),
                    None => {
                        self.open_layer_rename(document);
                        None
                    }
                }
            }
            InputEvent::Copy => self.copy(document, false),
            InputEvent::Cut => self.copy(document, true),
            InputEvent::Paste => self.paste_clip(&frame, document),
            InputEvent::SelectAll => {
                self.select_all(document);
                None
            }
            InputEvent::Break if self.edit.selection.is_some() => {
                self.edit.selection = None;
                None
            }
            InputEvent::Text(key) => self.key(&frame, document, key)?,
            // A drag in progress must end first so that it counts as a finished edit.
            InputEvent::Undo | InputEvent::Redo | InputEvent::Save => {
                self.end_stroke(Some(&mut *document));
                return None;
            }
            _ => return None,
        };
        self.fit(document);
        Some(out)
    }

    fn key(
        &mut self,
        frame: &Frame,
        document: &mut MapDocument,
        key: char,
    ) -> Option<Option<MapOut>> {
        if let Some(tool) = Tool::from_key(key) {
            self.set_tool(tool, document);
            return Some(None);
        }
        match key {
            'g' | 'G' => self.edit.grid = !self.edit.grid,
            'a' | 'A' => self.toggle_attr(),
            't' | 'T' => self.toggle_picker(),
            'v' => self.toggle_visible(self.edit.layer),
            'V' => self.show_only(self.edit.layer),
            'l' | 'L' => self.toggle_lock(self.edit.layer),
            'x' | 'X' => {
                self.edit.rulers = !self.edit.rulers;
                self.fit(document);
            }
            '+' | '=' => self.zoom(frame, document, true),
            '-' | '_' => self.zoom(frame, document, false),
            '0'..='2' => {
                let index = usize::from(key as u8 - b'0');
                self.edit.layer = BgLayer::from_index(index)?;
            }
            _ => return None,
        }
        Some(None)
    }

    fn set_tool(&mut self, tool: Tool, document: &mut MapDocument) {
        self.end_stroke(Some(document));
        self.edit.tool = tool;
    }

    fn press(&mut self, frame: &Frame, document: &mut MapDocument, at: Point) -> Option<MapOut> {
        match frame.hit(at, self.edit.strip_top) {
            Hit::Canvas => return self.press_canvas(frame, document, at),
            Hit::Tool(tool) => self.set_tool(tool, document),
            Hit::Grid => self.edit.grid = !self.edit.grid,
            Hit::Attr => self.toggle_attr(),
            Hit::Picker => self.toggle_picker(),
            Hit::EditTile => return self.edit_tile_out(usize::from(self.edit.brush.tile())),
            Hit::Rulers => {
                self.edit.rulers = !self.edit.rulers;
                self.fit(document);
            }
            Hit::ZoomOut => self.zoom(frame, document, false),
            Hit::ZoomIn => self.zoom(frame, document, true),
            Hit::DrawerSize => {
                self.edit.drawer = self.edit.drawer.cycled(false);
                self.fit(document);
            }
            Hit::Chip(layer) | Hit::LayerName(layer) => self.edit.layer = layer,
            Hit::Eye(layer) => self.toggle_visible(layer),
            Hit::Lock(layer) => self.toggle_lock(layer),
            Hit::Tile(tile) => return self.pick_tile(tile),
            Hit::Selector => self.open_list(Some(&*document)),
            Hit::Undo => {
                self.end_stroke(Some(&mut *document));
                document.undo();
            }
            Hit::Redo => {
                self.end_stroke(Some(&mut *document));
                document.redo();
            }
            Hit::Brush | Hit::Nothing => {}
        }
        None
    }

    /// Makes the strip's `tile` the brush's tile, keeping its flips,
    /// priority and bank. Tile 0 draws nothing, so it is not offered.
    pub(super) fn pick_tile(&mut self, tile: usize) -> Option<MapOut> {
        let brush = self.edit.brush;
        let picked = u16::try_from(tile)
            .ok()
            .filter(|&tile| tile != 0)
            .and_then(|tile| {
                TileEntry::new(
                    tile,
                    brush.flip_x(),
                    brush.flip_y(),
                    brush.priority(),
                    brush.bank(),
                )
            });
        match picked {
            Some(entry) => {
                self.edit.brush = entry;
                self.edit.pattern = (1, 1);
                None
            }
            None => Some(MapOut::Notice(ui_text::get("maps-tile-0-transparent"))),
        }
    }

    fn press_canvas(
        &mut self,
        frame: &Frame,
        document: &mut MapDocument,
        at: Point,
    ) -> Option<MapOut> {
        let cell = self.cell_at(frame, document, at);
        let size = map_size(document);
        match self.edit.tool {
            Tool::Pencil | Tool::Eraser | Tool::Fill
                if self.edit.locked[self.edit.layer.index()] =>
            {
                return Some(self.locked_notice());
            }
            Tool::Pencil | Tool::Eraser => {
                document.begin_edit();
                self.paint(document, cell);
                self.edit.stroke = Some(Stroke::Paint { last: cell });
            }
            Tool::Fill => self.fill(document, cell),
            Tool::Select => {
                self.edit.selection = Selection::between(cell, cell, size);
                self.edit.stroke = Some(Stroke::Select { anchor: cell });
            }
            Tool::Eyedropper => {
                let (Ok(x), Ok(y)) = (usize::try_from(cell.0), usize::try_from(cell.1)) else {
                    return None;
                };
                if let Some(entry) = document.layer(self.edit.layer).get(x, y) {
                    self.edit.brush = entry;
                    self.edit.pattern = (1, 1);
                }
            }
            Tool::Hand => {
                self.edit.stroke = Some(Stroke::Pan {
                    last: (i32::from(at.x), i32::from(at.y)),
                });
            }
        }
        None
    }

    fn paint(&self, document: &mut MapDocument, cell: (i32, i32)) {
        if self.edit.tool == Tool::Eraser {
            self.write(document, cell, TileEntry::EMPTY);
            return;
        }
        for ((dx, dy), entry) in stamp(self.edit.brush, self.edit.pattern) {
            self.write(document, (cell.0 + dx as i32, cell.1 + dy as i32), entry);
        }
    }

    /// Entries are the same only if every part is, flips, priority and bank included.
    fn fill(&self, document: &mut MapDocument, cell: (i32, i32)) {
        let (Ok(x), Ok(y)) = (usize::try_from(cell.0), usize::try_from(cell.1)) else {
            return;
        };
        let layer = document.layer(self.edit.layer);
        if !would_change(layer, x, y, self.edit.brush) {
            return;
        }
        let cells = connected(layer, x, y);
        document.begin_edit();
        for (x, y) in cells {
            self.write(document, (x as i32, y as i32), self.edit.brush);
        }
        document.end_edit();
    }

    fn drag(&mut self, frame: &Frame, document: &mut MapDocument, at: Point) {
        let Some(stroke) = self.edit.stroke else {
            return;
        };
        let size = map_size(document);
        match stroke {
            Stroke::Paint { last } => {
                let cell = self.cell_at(frame, document, at);
                let view = self.viewport(frame, document);
                let (first, shown) =
                    view.visible_cells(size, (frame.canvas.width, frame.canvas.height));
                let is_shown = |step: &(i32, i32)| {
                    (first.0 as i32..(first.0 + shown.0) as i32).contains(&step.0)
                        && (first.1 as i32..(first.1 + shown.1) as i32).contains(&step.1)
                };
                // Joining the cells between the moves leaves no gaps when
                // the pointer jumps. A pointer that leaves the canvas keeps
                // the stroke, but the cells under the controls around it
                // are not on screen and stay as they are.
                for step in line(last, cell).skip(1).filter(is_shown) {
                    self.paint(document, step);
                }
                self.edit.stroke = Some(Stroke::Paint { last: cell });
            }
            Stroke::Select { anchor } => {
                let cell = self.cell_at(frame, document, at);
                self.edit.selection = Selection::between(anchor, cell, size);
            }
            Stroke::Pan { last } => {
                let now = (i32::from(at.x), i32::from(at.y));
                let area = (frame.canvas.width, frame.canvas.height);
                self.edit
                    .view
                    .scroll_by(last.0 - now.0, last.1 - now.1, size, area);
                self.edit.stroke = Some(Stroke::Pan { last: now });
            }
        }
    }

    fn zoom(&mut self, frame: &Frame, document: &MapDocument, closer: bool) {
        let view = self.viewport(frame, document);
        let next = if closer {
            view.zoom().zoomed_in()
        } else {
            view.zoom().zoomed_out()
        };
        if let Some(zoom) = next {
            self.edit.view = view;
            let area = (frame.canvas.width, frame.canvas.height);
            self.edit.view.set_zoom(zoom, map_size(document), area);
        }
    }

    fn scroll(
        &mut self,
        frame: &Frame,
        document: &MapDocument,
        columns: i32,
        rows: i32,
    ) -> Option<MapOut> {
        let area = (frame.canvas.width, frame.canvas.height);
        self.edit.view = self.viewport(frame, document);
        self.edit
            .view
            .scroll_tiles(columns, rows, map_size(document), area);
        None
    }

    fn scroll_page(
        &mut self,
        frame: &Frame,
        document: &MapDocument,
        direction: i32,
    ) -> Option<MapOut> {
        let area = (frame.canvas.width, frame.canvas.height);
        let tile = self.edit.view.zoom().tile_px();
        let page = (area.1 - tile).max(tile);
        self.edit.view = self.viewport(frame, document);
        self.edit
            .view
            .scroll_by(0, direction * page, map_size(document), area);
        None
    }

    /// The wheel scrolls the strip while the pointer is over it, and the canvas otherwise.
    fn wheel(&mut self, frame: &Frame, document: &MapDocument, columns: i32, rows: i32) {
        let over_strip = self
            .edit
            .pointer
            .is_some_and(|at| frame.strip_frame().is_some_and(|rect| rect.contains(at)));
        if over_strip {
            let shown = frame.strip_rows;
            let last = STRIP_ROWS_TOTAL.saturating_sub(shown);
            let top = self.edit.strip_top as i32 + rows * 2;
            self.edit.strip_top = top.clamp(0, last as i32) as usize;
        } else {
            self.scroll(frame, document, columns * WHEEL_TILES, rows * WHEEL_TILES);
        }
    }

    #[cfg(test)]
    pub(super) fn zoom_level(&self) -> super::view::Zoom {
        self.edit.view.zoom()
    }

    pub(super) fn open_layer_rename(&mut self, document: &MapDocument) {
        let layer = self.edit.layer;
        let label = ui_text::get_with(
            "maps-layer-rename-label",
            &[("layer", Arg::Text(&layer.to_string()))],
        );
        let mut field = super::TextField::any(super::MAX_NAME);
        if let Some(name) = &document.meta().layer_names[layer.index()] {
            field.set_text(name);
        }
        self.overlay = Some(Overlay::RenameLayer(
            layer,
            super::Prompt::new(&ui_text::get("maps-layer-rename-title"), label, "", field),
        ));
    }

    pub(super) fn has_selection(&self) -> bool {
        self.edit.selection.is_some()
    }
}

#[cfg(test)]
mod tests;
