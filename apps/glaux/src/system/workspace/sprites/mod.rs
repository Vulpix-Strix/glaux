//! The sprite editor draws on the loaded cartridge's object tiles in
//! memory, chooses the palette each tile is drawn with, undoes and redoes
//! those edits, and asks for them to be saved.

mod color_editor;
mod import_preview;
mod layout;
mod palette_picker;
mod render;
mod sheet;
mod stroke;

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::machine::TICK_RATE;
use glaux_machine::obj_palette::{OBJ_PALETTE_COLORS, ObjBank};
use glaux_machine::palette::{Palette, Rgb};
use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_GRID_ROWS, ObjVram};
use glaux_machine::system_rom::SystemCursor;
use glaux_video::preview::ObjPreviewPalette;

use color_editor::{ColorEditor, ColorEditorLayout, ColorOutcome};
pub use color_editor::{SCRATCH_BASE, SCRATCH_COLORS};
pub use import_preview::ImportPreview;
use import_preview::{ImportLayout, ImportOutcome};
pub use layout::Metrics;
use layout::{Hit, SpriteLayout, ToolSlot};
use palette_picker::{PalettePicker, PickerLayout, PickerOutcome};
use sheet::{SheetView, grid_position};

use crate::system::sprite_document::{ColorScope, ColorTarget, SpriteDocument};

/// Most ticks between the presses of a double click.
const DOUBLE_CLICK_TICKS: u32 = TICK_RATE * 2 / 5;

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
    /// Ask the host to choose a PNG spritesheet to import.
    Import,
    /// Tell the user something briefly, such as that there is nothing to
    /// undo.
    Notice(&'static str),
}

/// A modal dialog the editor opened, which takes every input while open.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EditorDialog {
    /// SELECT PALETTE.
    Palette(PalettePicker),
    /// EDIT COLOR.
    Color(ColorEditor),
    /// IMPORT SPRITES.
    Import(Box<ImportPreview>),
}

/// What closing a dialog asks of the document.
enum DialogResult {
    Bank(usize, ObjBank),
    Color(ColorTarget, Rgb, ColorScope),
    Replace(Box<crate::system::video::CartridgeVideo>),
    Nothing,
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
    /// The open dialog, which takes every input while open.
    dialog: Option<EditorDialog>,
    /// The swatch last pressed, and for how many more ticks a press on it
    /// makes a double click.
    last_swatch_press: Option<(u8, u32)>,
    /// Whether the host's file dialog was asked for a spritesheet and has
    /// not answered. Until it does, or Escape stops the wait, the editor
    /// takes no other input.
    awaiting_file: bool,
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
            dialog: None,
            last_swatch_press: None,
            awaiting_file: false,
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

    /// Whether a dialog is open, or the host's file dialog is awaited,
    /// and taking every input.
    pub fn has_dialog(&self) -> bool {
        self.dialog.is_some() || self.awaiting_file
    }

    /// Whether the host's file dialog was asked for a spritesheet and has
    /// not answered.
    pub fn awaits_file(&self) -> bool {
        self.awaiting_file
    }

    /// Stops waiting for the host's file dialog, which answered or will
    /// not be listened to.
    pub fn stop_awaiting_file(&mut self) {
        self.awaiting_file = false;
    }

    /// The pointer for `at`: an hourglass while the host's file dialog is
    /// awaited, a pointing hand over the controls and the tiles and colors
    /// a click picks, and the arrow over the canvas and while a dialog is
    /// open.
    pub fn cursor(&self, at: Point) -> SystemCursor {
        if self.awaiting_file {
            return SystemCursor::Busy;
        }
        if self.dialog.is_some() {
            return SystemCursor::Arrow;
        }
        match self.layout().hit(at, &self.sheet) {
            Hit::Canvas { .. } | Hit::None => SystemCursor::Arrow,
            _ => SystemCursor::Hand,
        }
    }

    /// Opens IMPORT SPRITES on `preview`, ending the wait for the file it
    /// shows.
    pub fn open_import(&mut self, preview: ImportPreview) {
        self.awaiting_file = false;
        self.dialog = Some(EditorDialog::Import(Box::new(preview)));
    }

    /// The open IMPORT SPRITES dialog, if any.
    #[cfg(test)]
    pub fn import_preview(&self) -> Option<&ImportPreview> {
        match &self.dialog {
            Some(EditorDialog::Import(preview)) => Some(preview),
            _ => None,
        }
    }

    /// Where the open IMPORT SPRITES dialog's parts sit.
    #[cfg(test)]
    pub fn import_preview_layout(&self) -> ImportLayout {
        self.import_layout()
    }

    /// The palette frames are presented with while IMPORT SPRITES shows a
    /// converted sheet.
    pub fn import_frame_palette(&self) -> Option<&Palette> {
        match &self.dialog {
            Some(EditorDialog::Import(preview)) => preview.frame_palette(),
            _ => None,
        }
    }

    /// The open SELECT PALETTE dialog, if any.
    #[cfg(test)]
    pub fn picker(&self) -> Option<&PalettePicker> {
        match &self.dialog {
            Some(EditorDialog::Palette(picker)) => Some(picker),
            _ => None,
        }
    }

    /// The open EDIT COLOR dialog, if any.
    #[cfg(test)]
    pub fn color_editor(&self) -> Option<&ColorEditor> {
        match &self.dialog {
            Some(EditorDialog::Color(editor)) => Some(editor),
            _ => None,
        }
    }

    /// Where the open EDIT COLOR dialog's parts sit.
    #[cfg(test)]
    pub fn color_editor_layout(&self) -> ColorEditorLayout {
        self.edit_color_layout()
    }

    /// Where the swatch of local color `local` is drawn.
    #[cfg(test)]
    pub fn swatch_rect(&self, local: u8) -> crate::system::workspace::Rect {
        self.layout().swatches[usize::from(local)]
    }

    /// Advances the time double clicks are measured in by one tick.
    pub fn tick(&mut self) {
        self.last_swatch_press = match self.last_swatch_press {
            Some((local, ticks)) if ticks > 0 => Some((local, ticks - 1)),
            _ => None,
        };
    }

    /// The colors the open EDIT COLOR dialog draws with, for the frame
    /// palette from [`SCRATCH_BASE`] on.
    pub fn scratch_colors(&self) -> Option<[Rgb; SCRATCH_COLORS]> {
        match &self.dialog {
            Some(EditorDialog::Color(editor)) => Some(editor.scratch_colors()),
            _ => None,
        }
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

    /// Where the palette button is drawn.
    #[cfg(test)]
    pub fn palette_button_rect(&self) -> crate::system::workspace::Rect {
        self.layout().palette_button
    }

    /// Where the open palette picker draws the tile through `bank`.
    #[cfg(test)]
    pub fn picker_preview_rect(&self, bank: usize) -> crate::system::workspace::Rect {
        let layout = self.picker_layout();
        layout.preview(layout.rows[bank])
    }

    /// Where the Import button is drawn.
    #[cfg(test)]
    pub fn import_button_rect(&self) -> crate::system::workspace::Rect {
        self.layout().import_button
    }

    /// Where the Save button is drawn.
    #[cfg(test)]
    pub fn save_button_rect(&self) -> crate::system::workspace::Rect {
        self.layout().save_button
    }

    fn layout(&self) -> SpriteLayout {
        SpriteLayout::new(self.metrics, self.advanced, self.sheet.zoom())
    }

    /// The screen below the tab bar, where the dialogs are centred.
    fn dialog_area(&self) -> crate::system::workspace::Rect {
        crate::system::workspace::Rect {
            x: 0,
            y: self.metrics.top,
            width: WIDTH as i32,
            height: HEIGHT as i32 - self.metrics.top,
        }
    }

    /// The palette picker, centred in the editor's area.
    fn picker_layout(&self) -> PickerLayout {
        PickerLayout::new(self.dialog_area(), self.metrics, self.advanced)
    }

    /// The color editor, centred in the editor's area.
    fn edit_color_layout(&self) -> ColorEditorLayout {
        ColorEditorLayout::new(self.dialog_area(), self.metrics, self.advanced)
    }

    /// The import dialog, over the editor's area.
    fn import_layout(&self) -> ImportLayout {
        ImportLayout::new(self.dialog_area(), self.metrics)
    }

    /// Applies `event`, drawing on the selected tile of `document` when
    /// there is one. Returns what the workspace should do about it, if
    /// anything.
    ///
    /// A press on the canvas uses the selected tool there, and the pencil,
    /// eraser and eyedropper keep acting along a drag until its release.
    /// Each drag or fill is one undoable edit. A click elsewhere selects a
    /// sheet tile, a palette color, a tool or an options button, or asks to
    /// save. The wheel scrolls the sheet's rows, and Shift with the wheel
    /// pans its columns. Arrow keys move the selection. `p`, `e`, `i` and
    /// `f` pick a tool, `g` toggles the grid, `a` Advanced mode, and `+`
    /// and `-` zoom the sheet. Undo, redo and save first end any drag, so
    /// it counts as one finished edit.
    ///
    /// The palette button, or an OBP cell in Advanced mode, opens the
    /// palette picker. A double click on a swatch, or `c`, opens EDIT COLOR
    /// on that color. Either dialog then takes every event until it
    /// closes.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        mut document: Option<&mut SpriteDocument>,
    ) -> Option<SpriteRequest> {
        if self.awaiting_file {
            if event == InputEvent::Break {
                self.awaiting_file = false;
                return Some(SpriteRequest::Notice("Stopped waiting for the file dialog"));
            }
            return None;
        }
        if self.dialog.is_some() {
            self.dialog_input(event, document);
            return None;
        }
        let layout = self.layout();
        match event {
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
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
                    Hit::Swatch(local) => return self.press_swatch(local, document),
                    Hit::Tool(tool) => self.set_tool(tool, document),
                    Hit::GridButton => self.grid = !self.grid,
                    Hit::AdvancedButton => self.toggle_advanced(),
                    Hit::ZoomOut => self.zoom(-1),
                    Hit::ZoomIn => self.zoom(1),
                    Hit::SaveButton => return self.save(document),
                    Hit::ImportButton => return self.import(document),
                    Hit::PaletteButton => self.open_picker(document, None),
                    Hit::Bank(bank) => self.open_picker(document, ObjBank::new(bank)),
                    Hit::None => {}
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
            InputEvent::Up { .. } => self.step(0, -1, document),
            InputEvent::Down { .. } => self.step(0, 1, document),
            InputEvent::Text('g' | 'G') => self.grid = !self.grid,
            InputEvent::Text('a' | 'A') => self.toggle_advanced(),
            InputEvent::Text('+' | '=') => self.zoom(1),
            InputEvent::Text('-' | '_') => self.zoom(-1),
            InputEvent::Text('c' | 'C') => return self.open_color_editor(document, self.color),
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

    /// Hands `event` to the open dialog, and carries out how it closes, if
    /// it does.
    fn dialog_input(&mut self, event: InputEvent, document: Option<&mut SpriteDocument>) {
        let picker_layout = self.picker_layout();
        let color_layout = self.edit_color_layout();
        let import_layout = self.import_layout();
        let outcome = match &mut self.dialog {
            Some(EditorDialog::Palette(picker)) => {
                let Some(outcome) = picker.handle_input(event, &picker_layout) else {
                    return;
                };
                let tile = picker.tile();
                // Applying the bank the tile has is a no-op the document
                // records nothing for.
                Some(match outcome {
                    PickerOutcome::Apply(bank) => DialogResult::Bank(tile, bank),
                    PickerOutcome::Cancel => DialogResult::Nothing,
                })
            }
            Some(EditorDialog::Color(editor)) => {
                let Some(outcome) = editor.handle_input(event, &color_layout) else {
                    return;
                };
                Some(match outcome {
                    ColorOutcome::Apply { color, scope } => {
                        DialogResult::Color(editor.target(), color, scope)
                    }
                    ColorOutcome::Cancel => DialogResult::Nothing,
                })
            }
            Some(EditorDialog::Import(preview)) => {
                // The preview is only opened on a document. Without one
                // there is nothing to convert against or replace.
                let outcome = match document.as_deref() {
                    Some(document) => {
                        let Some(outcome) = preview.handle_input(event, &import_layout, document)
                        else {
                            return;
                        };
                        outcome
                    }
                    None => ImportOutcome::Cancel,
                };
                Some(match outcome {
                    ImportOutcome::Import(video) => DialogResult::Replace(video),
                    ImportOutcome::Cancel => DialogResult::Nothing,
                })
            }
            None => None,
        };
        let Some(outcome) = outcome else {
            return;
        };
        self.dialog = None;
        let Some(document) = document else {
            return;
        };
        match outcome {
            DialogResult::Bank(tile, bank) => {
                document.set_tile_bank(tile, bank);
            }
            DialogResult::Color(target, color, scope) => {
                document.recolor(target, color, scope);
            }
            DialogResult::Replace(video) => {
                document.replace_video(&video);
            }
            DialogResult::Nothing => {}
        }
    }

    /// Selects local color `local`, opening EDIT COLOR on it when this
    /// press follows another on the same swatch quickly enough.
    fn press_swatch(
        &mut self,
        local: u8,
        document: Option<&mut SpriteDocument>,
    ) -> Option<SpriteRequest> {
        self.color = local;
        if self
            .last_swatch_press
            .take()
            .is_some_and(|(last, _)| last == local)
        {
            return self.open_color_editor(document, local);
        }
        self.last_swatch_press = Some((local, DOUBLE_CLICK_TICKS));
        None
    }

    /// Opens EDIT COLOR on local color `local` of the selected tile's
    /// palette. The transparent local 0 has no color to edit, and without
    /// a document nothing opens.
    fn open_color_editor(
        &mut self,
        document: Option<&mut SpriteDocument>,
        local: u8,
    ) -> Option<SpriteRequest> {
        let document = document?;
        let target = ColorTarget {
            bank: document.obj().attributes().bank(self.selected),
            local,
        };
        let Some(uses) = document.color_uses(target, self.selected) else {
            return Some(SpriteRequest::Notice("Color 0 is transparent"));
        };
        self.end_stroke(Some(&mut *document));
        let old = document.video().palette.get(uses.master);
        self.dialog = Some(EditorDialog::Color(ColorEditor::new(
            self.selected,
            target,
            uses,
            old,
        )));
        None
    }

    /// Opens the palette picker on the selected tile of `document`, with
    /// `pending` chosen, or else the tile's own bank. Without a document
    /// there is no tile to recolor, so nothing opens.
    fn open_picker(&mut self, document: Option<&mut SpriteDocument>, pending: Option<ObjBank>) {
        let Some(document) = document else {
            return;
        };
        self.end_stroke(Some(&mut *document));
        let current = document.obj().attributes().bank(self.selected);
        self.dialog = Some(EditorDialog::Palette(PalettePicker::new(
            self.selected,
            pending.unwrap_or(current),
        )));
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

    /// Ends any drag, then asks the host for a spritesheet to import into
    /// `document`, waiting for its answer. Without a document there is
    /// nothing to import into.
    fn import(&mut self, mut document: Option<&mut SpriteDocument>) -> Option<SpriteRequest> {
        self.end_stroke(document.as_deref_mut());
        document?;
        self.awaiting_file = true;
        Some(SpriteRequest::Import)
    }

    /// Uses the tool at tile pixel `at`, starting a drag if the tool keeps
    /// acting while dragged.
    fn press(&mut self, document: &mut SpriteDocument, at: (i32, i32)) {
        // A fill records itself as one edit. A drag's edit stays open
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

    /// Advanced mode changes the sheet's height. The sheet keeps its
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
