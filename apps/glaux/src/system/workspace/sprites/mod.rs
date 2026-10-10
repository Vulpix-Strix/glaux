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
mod tests {
    use super::*;
    use glaux_input::Point;
    use pretty_assertions::assert_eq;
    use sheet::Zoom;

    const METRICS: Metrics = Metrics {
        cell_w: 6,
        cell_h: 10,
        top: 16,
    };

    fn editor() -> SpriteEditor {
        SpriteEditor::new(METRICS)
    }

    fn click(editor: &mut SpriteEditor, rect: crate::system::workspace::Rect) {
        editor.handle_input(
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at: Point::new(
                    (rect.x + rect.width / 2) as u16,
                    (rect.y + rect.height / 2) as u16,
                ),
            },
            None,
        );
    }

    #[test]
    fn clicking_a_visible_tile_selects_it_without_scrolling() {
        let mut editor = editor();
        editor.handle_input(InputEvent::Wheel { delta: -5 }, None);
        let before = editor.sheet;
        let layout = editor.layout();
        click(&mut editor, layout.sheet_cell(4, 21, Zoom::X1));
        assert_eq!(editor.selected(), (10 + 21) * 16 + 4);
        assert_eq!(editor.sheet, before);
    }

    #[test]
    fn the_wheel_scrolls_rows_and_shift_wheel_pans_at_two_x() {
        let mut editor = editor();
        editor.handle_input(InputEvent::Wheel { delta: -3 }, None);
        assert_eq!(editor.sheet.top_row(), 6);
        editor.handle_input(InputEvent::Wheel { delta: 1 }, None);
        assert_eq!(editor.sheet.top_row(), 4);
        editor.handle_input(InputEvent::WheelHorizontal { delta: -2 }, None);
        assert_eq!(editor.sheet.left_column(), 0, "1x shows every column");

        editor.handle_input(InputEvent::Text('+'), None);
        assert_eq!(editor.sheet.zoom(), Zoom::X2);
        editor.handle_input(InputEvent::WheelHorizontal { delta: -2 }, None);
        assert_eq!(editor.sheet.left_column(), 2);
        editor.handle_input(InputEvent::WheelHorizontal { delta: -100 }, None);
        assert_eq!(editor.sheet.left_column(), 5);
        editor.handle_input(InputEvent::Wheel { delta: i8::MIN }, None);
        assert_eq!(editor.sheet.top_row(), 64 - 11);
    }

    #[test]
    fn arrows_move_the_selection_and_reveal_it_minimally() {
        let mut editor = editor();
        editor.handle_input(InputEvent::Left { select: false }, None);
        editor.handle_input(InputEvent::Up, None);
        assert_eq!(
            editor.selected(),
            0,
            "the grid's corner stops the selection"
        );

        for _ in 0..22 {
            editor.handle_input(InputEvent::Down, None);
        }
        assert_eq!(editor.selected(), 22 * 16);
        assert_eq!(editor.sheet.top_row(), 1);

        for _ in 0..20 {
            editor.handle_input(InputEvent::Right { select: false }, None);
        }
        assert_eq!(editor.selected(), 22 * 16 + 15);
        assert_eq!(editor.sheet.top_row(), 1);
    }

    #[test]
    fn zooming_keeps_the_selection_on_screen() {
        let mut editor = editor();
        let layout = editor.layout();
        click(&mut editor, layout.sheet_cell(15, 20, Zoom::X1));
        editor.handle_input(InputEvent::Text('+'), None);
        let visible = editor.layout().visible;
        assert!(editor.sheet.is_visible(editor.selected(), visible));
        editor.handle_input(InputEvent::Text('+'), None);
        assert_eq!(editor.sheet.zoom(), Zoom::X2, "2x is the largest zoom");
        editor.handle_input(InputEvent::Text('-'), None);
        editor.handle_input(InputEvent::Text('-'), None);
        assert_eq!(editor.sheet.zoom(), Zoom::X1);
    }

    #[test]
    fn advanced_mode_keeps_zoom_and_scroll() {
        let mut editor = editor();
        editor.handle_input(InputEvent::Text('+'), None);
        editor.handle_input(InputEvent::Wheel { delta: -10 }, None);
        editor.handle_input(InputEvent::WheelHorizontal { delta: -3 }, None);
        let before = editor.sheet;
        editor.handle_input(InputEvent::Text('a'), None);
        assert!(editor.is_advanced());
        assert_eq!(editor.sheet, before);
        editor.handle_input(InputEvent::Text('a'), None);
        assert_eq!(editor.sheet, before);
    }

    #[test]
    fn options_buttons_and_swatches_respond_to_clicks() {
        let mut editor = editor();
        let layout = editor.layout();
        click(&mut editor, layout.grid_button);
        assert!(!editor.grid);
        click(&mut editor, layout.zoom_in);
        assert_eq!(editor.sheet.zoom(), Zoom::X2);
        click(&mut editor, layout.swatches[9]);
        assert_eq!(editor.color(), 9);
        click(&mut editor, layout.advanced_button);
        assert!(editor.is_advanced());
    }

    // Drawing.

    use crate::system::video::{CartridgeVideoDefaults, VideoFiles};

    /// A document whose tile 0 is solid local 5 and every other tile empty.
    fn document() -> SpriteDocument {
        let files = VideoFiles {
            obj_tiles: Some(vec![0x55; glaux_machine::sprite::OBJ_TILE_BYTES]),
            ..VideoFiles::default()
        };
        let defaults = CartridgeVideoDefaults::from_rom(&crate::rom::load().unwrap());
        SpriteDocument::from_files(&files, &defaults).unwrap()
    }

    /// The screen point in the middle of tile pixel `(x, y)`, which may lie
    /// off the canvas.
    fn canvas_point(editor: &SpriteEditor, x: i32, y: i32) -> Point {
        let canvas = editor.canvas_rect();
        let half = layout::CANVAS_ZOOM / 2;
        Point::new(
            (canvas.x + x * layout::CANVAS_ZOOM + half) as u16,
            (canvas.y + y * layout::CANVAS_ZOOM + half) as u16,
        )
    }

    fn down(editor: &mut SpriteEditor, doc: &mut SpriteDocument, x: i32, y: i32) {
        let at = canvas_point(editor, x, y);
        editor.handle_input(
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
            },
            Some(doc),
        );
    }

    fn drag(editor: &mut SpriteEditor, doc: &mut SpriteDocument, x: i32, y: i32) {
        let at = canvas_point(editor, x, y);
        editor.handle_input(InputEvent::PointerMove(Some(at)), Some(doc));
    }

    fn up(editor: &mut SpriteEditor, doc: &mut SpriteDocument, x: i32, y: i32) {
        let at = canvas_point(editor, x, y);
        editor.handle_input(
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                at,
            },
            Some(doc),
        );
    }

    /// Tile `tile` as one string of hex digits per pixel row.
    fn rows(doc: &SpriteDocument, tile: usize) -> Vec<String> {
        (0..8)
            .map(|y| {
                (0..8)
                    .map(|x| format!("{:X}", doc.pixel(tile, x, y).unwrap()))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_fast_diagonal_drag_paints_every_pixel_between() {
        let mut editor = editor();
        let mut doc = document();
        editor.select(1, None);
        down(&mut editor, &mut doc, 0, 0);
        drag(&mut editor, &mut doc, 7, 7);
        up(&mut editor, &mut doc, 7, 7);
        for n in 0..8 {
            assert_eq!(doc.pixel(1, n, n), Some(1), "pixel ({n}, {n})");
        }
        assert_eq!(rows(&doc, 1).concat().matches('1').count(), 8);
    }

    #[test]
    fn a_fast_horizontal_drag_leaves_no_gaps() {
        let mut editor = editor();
        let mut doc = document();
        editor.handle_input(InputEvent::Text('e'), None);
        down(&mut editor, &mut doc, 0, 3);
        drag(&mut editor, &mut doc, 5, 3);
        drag(&mut editor, &mut doc, 7, 3);
        up(&mut editor, &mut doc, 7, 3);
        assert_eq!(rows(&doc, 0)[3], "00000000");
        assert_eq!(rows(&doc, 0)[2], "55555555");
    }

    #[test]
    fn a_drag_off_the_canvas_paints_to_the_edge_and_resumes_on_return() {
        let mut editor = editor();
        let mut doc = document();
        editor.select(1, None);
        down(&mut editor, &mut doc, 5, 1);
        drag(&mut editor, &mut doc, 11, 1);
        drag(&mut editor, &mut doc, 11, 5);
        drag(&mut editor, &mut doc, 5, 5);
        up(&mut editor, &mut doc, 5, 5);
        assert_eq!(
            rows(&doc, 1),
            [
                "00000000", //
                "00000111", //
                "00000000", //
                "00000000", //
                "00000000", //
                "00000111", //
                "00000000", //
                "00000000", //
            ]
        );
        assert_eq!(
            rows(&doc, 2).concat(),
            "0".repeat(64),
            "never the next tile"
        );
    }

    #[test]
    fn moves_after_a_release_or_cancel_do_not_paint() {
        let mut editor = editor();
        let mut doc = document();
        editor.select(1, None);
        down(&mut editor, &mut doc, 0, 0);
        up(&mut editor, &mut doc, 0, 0);
        drag(&mut editor, &mut doc, 3, 0);
        assert_eq!(rows(&doc, 1)[0], "10000000");

        down(&mut editor, &mut doc, 0, 2);
        drag(&mut editor, &mut doc, 2, 2);
        editor.handle_input(InputEvent::PointerCancel, Some(&mut doc));
        drag(&mut editor, &mut doc, 6, 2);
        up(&mut editor, &mut doc, 6, 2);
        assert_eq!(rows(&doc, 1)[2], "11100000", "what was drawn stays");
    }

    #[test]
    fn a_press_without_a_release_starts_a_new_stroke() {
        let mut editor = editor();
        let mut doc = document();
        editor.select(1, None);
        down(&mut editor, &mut doc, 0, 0);
        // The release was lost; the next press must not join the two.
        down(&mut editor, &mut doc, 7, 7);
        drag(&mut editor, &mut doc, 7, 6);
        assert_eq!(rows(&doc, 1).concat().matches('1').count(), 3);
        assert_eq!(doc.pixel(1, 3, 3), Some(0));
    }

    #[test]
    fn changing_tool_or_tile_ends_the_stroke() {
        let mut editor = editor();
        let mut doc = document();
        editor.select(1, None);
        down(&mut editor, &mut doc, 0, 0);
        editor.handle_input(InputEvent::Text('e'), Some(&mut doc));
        drag(&mut editor, &mut doc, 4, 0);
        assert_eq!(rows(&doc, 1)[0], "10000000");

        editor.handle_input(InputEvent::Text('p'), Some(&mut doc));
        down(&mut editor, &mut doc, 0, 1);
        editor.handle_input(InputEvent::Down, Some(&mut doc));
        drag(&mut editor, &mut doc, 4, 1);
        assert_eq!(rows(&doc, 1)[1], "10000000");
        assert_eq!(rows(&doc, 17)[1], "00000000");
    }

    #[test]
    fn the_eyedropper_picks_colors_including_transparent() {
        let mut editor = editor();
        let mut doc = document();
        editor.handle_input(InputEvent::Text('I'), None);
        assert_eq!(editor.tool, ToolSlot::Eyedropper);
        down(&mut editor, &mut doc, 2, 2);
        assert_eq!(editor.color(), 5);
        editor.select(1, None);
        down(&mut editor, &mut doc, 2, 2);
        assert_eq!(editor.color(), 0);
        assert!(!doc.is_modified());
    }

    #[test]
    fn fill_acts_once_per_press() {
        let mut editor = editor();
        let mut doc = document();
        editor.handle_input(InputEvent::Text('f'), None);
        editor.color = 9;
        down(&mut editor, &mut doc, 4, 4);
        assert!(rows(&doc, 0).iter().all(|row| row == "99999999"));
        assert!(rows(&doc, 1).iter().all(|row| row == "00000000"));
        assert_eq!(editor.stroke, None);
    }

    #[test]
    fn keys_and_tool_plates_select_tools() {
        let mut editor = editor();
        assert_eq!(editor.tool, ToolSlot::Pencil);
        for (key, tool) in [
            ('e', ToolSlot::Eraser),
            ('i', ToolSlot::Eyedropper),
            ('F', ToolSlot::Fill),
            ('p', ToolSlot::Pencil),
        ] {
            editor.handle_input(InputEvent::Text(key), None);
            assert_eq!(editor.tool, tool, "{key}");
        }
        let layout = editor.layout();
        for (tool, rect) in layout.tools {
            click(&mut editor, rect);
            assert_eq!(editor.tool, tool);
        }
    }

    #[test]
    fn without_a_document_the_canvas_does_nothing() {
        let mut editor = editor();
        let before = editor.clone();
        let canvas = editor.layout().canvas;
        click(&mut editor, canvas);
        assert_eq!(editor, before);
    }

    // Undo, redo and save.

    fn command(
        editor: &mut SpriteEditor,
        doc: &mut SpriteDocument,
        event: InputEvent,
    ) -> Option<SpriteRequest> {
        editor.handle_input(event, Some(doc))
    }

    #[test]
    fn a_pencil_or_eraser_drag_undoes_as_one() {
        for key in ['p', 'e'] {
            let mut editor = editor();
            let mut doc = document();
            editor.handle_input(InputEvent::Text(key), None);
            down(&mut editor, &mut doc, 0, 0);
            drag(&mut editor, &mut doc, 4, 0);
            drag(&mut editor, &mut doc, 4, 3);
            up(&mut editor, &mut doc, 4, 3);
            assert!(doc.is_modified(), "{key}");
            assert_eq!(command(&mut editor, &mut doc, InputEvent::Undo), None);
            assert_eq!(rows(&doc, 0).concat(), "5".repeat(64), "{key}");
            assert!(!doc.can_undo(), "{key}: one drag, one edit");
            command(&mut editor, &mut doc, InputEvent::Redo);
            assert!(doc.is_modified(), "{key}");
        }
    }

    #[test]
    fn a_fill_undoes_as_one_and_the_eyedropper_records_nothing() {
        let mut editor = editor();
        let mut doc = document();
        editor.handle_input(InputEvent::Text('i'), None);
        down(&mut editor, &mut doc, 1, 1);
        up(&mut editor, &mut doc, 1, 1);
        assert!(!doc.can_undo());

        editor.handle_input(InputEvent::Text('f'), None);
        editor.color = 9;
        down(&mut editor, &mut doc, 1, 1);
        up(&mut editor, &mut doc, 1, 1);
        command(&mut editor, &mut doc, InputEvent::Undo);
        assert_eq!(rows(&doc, 0).concat(), "5".repeat(64));
        assert!(!doc.can_undo());
    }

    #[test]
    fn interrupted_strokes_stay_separate_edits() {
        let mut editor = editor();
        let mut doc = document();
        editor.select(1, None);
        down(&mut editor, &mut doc, 0, 0);
        drag(&mut editor, &mut doc, 2, 0);
        editor.handle_input(InputEvent::PointerCancel, Some(&mut doc));
        // A release that never arrived: the next press starts anew.
        down(&mut editor, &mut doc, 0, 2);
        drag(&mut editor, &mut doc, 2, 2);
        down(&mut editor, &mut doc, 0, 4);
        up(&mut editor, &mut doc, 0, 4);

        let mut steps = Vec::new();
        while doc.undo() {
            steps.push(rows(&doc, 1)[..5].concat());
        }
        assert_eq!(
            steps,
            [
                format!("{}{}{}", "11100000", "0".repeat(8), "11100000") + &"0".repeat(16),
                "11100000".to_owned() + &"0".repeat(32),
                "0".repeat(40),
            ]
        );
    }

    #[test]
    fn undo_during_a_drag_undoes_the_whole_drag_so_far() {
        let mut editor = editor();
        let mut doc = document();
        editor.select(1, None);
        down(&mut editor, &mut doc, 0, 0);
        drag(&mut editor, &mut doc, 3, 0);
        command(&mut editor, &mut doc, InputEvent::Undo);
        assert_eq!(rows(&doc, 1)[0], "00000000");
        drag(&mut editor, &mut doc, 6, 0);
        assert_eq!(rows(&doc, 1)[0], "00000000", "the drag ended");
    }

    #[test]
    fn save_is_asked_for_only_with_unsaved_edits() {
        let mut editor = editor();
        let mut doc = document();
        assert_eq!(
            command(&mut editor, &mut doc, InputEvent::Save),
            Some(SpriteRequest::Notice("Nothing to save"))
        );
        assert_eq!(
            command(&mut editor, &mut doc, InputEvent::Undo),
            Some(SpriteRequest::Notice("Nothing to undo"))
        );
        assert_eq!(
            command(&mut editor, &mut doc, InputEvent::Redo),
            Some(SpriteRequest::Notice("Nothing to redo"))
        );
        down(&mut editor, &mut doc, 0, 0);
        // Saving mid-drag ends the drag first, so it is saved whole.
        assert_eq!(
            command(&mut editor, &mut doc, InputEvent::Save),
            Some(SpriteRequest::Save)
        );
        drag(&mut editor, &mut doc, 4, 0);
        assert_eq!(rows(&doc, 0)[0], "15555555");
        assert!(doc.can_undo());

        let button = editor.layout().save_button;
        let at = Point::new(
            (button.x + button.width / 2) as u16,
            (button.y + button.height / 2) as u16,
        );
        let press = InputEvent::PointerDown {
            button: PointerButton::Primary,
            at,
        };
        assert_eq!(
            editor.handle_input(press, Some(&mut doc)),
            Some(SpriteRequest::Save)
        );
        assert_eq!(editor.handle_input(press, None), None);
    }
}
