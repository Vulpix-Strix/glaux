//! Drawing the sprite editor. UI parts use system palette indices. Tile
//! pixels use the frame indices [`ObjPreviewPalette`] gives their master
//! colors, so they show the cartridge's exact colors.
//!
//! [`ObjPreviewPalette`]: glaux_video::preview::ObjPreviewPalette

use glaux_i18n::Arg;
use glaux_input::Point;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, ObjBank};
use glaux_machine::palette::Rgb;
use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_GRID_ROWS, OBJ_TILE_WIDTH};
use glaux_machine::system_rom::{SystemIcon, SystemUi};
use glaux_video::draw::{
    ObjPaletteChoice, draw_ink_image, draw_obj_tile_scaled, draw_text, fill_rect,
};

use super::layout::{
    CANVAS_ZOOM, Hit, SCROLLBAR, SpriteLayout, ToolSlot, advanced_labels, caption, import_label,
    save_label,
};
use super::sheet::{SheetView, Visible, grid_position};
use super::{EditorDialog, SpriteEditor, SpriteSource};
use crate::rom::ICON_SIZE;
use crate::system::screen::{ACCENT, BACKGROUND, DIM, ERROR, PALE, TEXT};
use crate::system::workspace::{BAR, HOVER_SHADE, IDLE_SHADE, OUTLINE, Rect, wrap};
use crate::ui_text;

/// The two colors of the checkerboard behind transparent pixels.
pub(super) const CHECK_DARK: u8 = 12;
pub(super) const CHECK_LIGHT: u8 = 13;
/// The status bar hint while the palette picker is open.
fn picker_hint() -> String {
    ui_text::get("sprites-arrows-choose-enter-apply-esc-cancel")
}
/// The status bar hint while EDIT COLOR is open.
fn color_hint() -> String {
    ui_text::get("sprites-drag-pick-tab-next-enter-apply-esc-cancel")
}
/// Frames around the canvas and the sheet.
const FRAME: u8 = DIM;
/// Pixel grid lines over the canvas.
const GRID_LINE: u8 = BACKGROUND;
/// Tool icon inks, as for the workspace's tabs.
const ACTIVE_TOOL_INKS: [u8; 3] = [OUTLINE, ACCENT, TEXT];
const HOVER_TOOL_INKS: [u8; 3] = [OUTLINE, HOVER_SHADE, PALE];
const IDLE_TOOL_INKS: [u8; 3] = [OUTLINE, IDLE_SHADE, DIM];
/// Shown in the options bar while the tiles differ from the cartridge's
/// files.
fn modified_label() -> String {
    ui_text::get("sprites-modified")
}
/// The status bar hint while IMPORT SPRITES is open.
fn import_hint() -> String {
    ui_text::get("sprites-t-transparency-enter-import-esc-cancel")
}
/// The status bar notice while the host's file dialog is open.
pub(super) fn waiting_notice() -> String {
    ui_text::get("sprites-waiting-for-the-file-dialog-esc-stop-waiting")
}

impl SpriteEditor {
    /// Draws the editor below the tab bar from `source`, or explains why
    /// there is nothing to show. `pointer` highlights what it hovers, and
    /// `notice`, when given, takes the status bar's left side in its color.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        ui: &SystemUi,
        source: Result<SpriteSource<'_>, &str>,
        pointer: Option<Point>,
        notice: Option<(&str, u8)>,
    ) {
        let layout = self.layout();
        // Nothing under a dialog reacts to the pointer.
        let hover = match (&self.dialog, pointer) {
            (None, Some(at)) if !self.awaiting_file => layout.hit(at, &self.sheet),
            _ => Hit::None,
        };
        // The import preview's frame palette holds the colors of the sheet
        // being imported, not the document's, so nothing drawn in the
        // document's colors may show while it is open.
        let importing = matches!(self.dialog, Some(EditorDialog::Import(_)));
        let mut painter = Painter {
            framebuffer,
            font,
            layout: &layout,
        };

        let modified = source.is_ok_and(|source| source.modified);
        painter.options_bar(self, hover, modified);
        painter.toolbar(ui, self.tool, hover);
        match source {
            Ok(source) => {
                if !importing {
                    painter.canvas(self, &source, hover);
                }
                painter.save_button(source.modified, hover);
                painter.import_button(self.awaiting_file || importing, hover);
                painter.palette_button(self, &source, hover);
                if !importing {
                    painter.sheet(self, &source, hover);
                    painter.palette(self, &source, hover);
                }
            }
            Err(message) => painter.unavailable(message),
        }
        let (left, mut right) = self.status(source.ok().as_ref(), hover);
        match self.dialog {
            Some(EditorDialog::Palette(_)) => right = picker_hint(),
            Some(EditorDialog::Color(_)) => right = color_hint(),
            Some(EditorDialog::Import(_)) => right = import_hint(),
            None => {}
        }
        let waiting = waiting_notice();
        let notice = if self.awaiting_file {
            Some((waiting.as_str(), PALE))
        } else {
            notice
        };
        match notice {
            // A notice answers what the user just did, so it wins over the
            // hint when both don't fit.
            Some((text, color)) => painter.status_bar(text, color, &right, false),
            None => {
                // A hint about the control under the pointer matters more
                // than the tile summary when both don't fit.
                let control = !matches!(hover, Hit::None | Hit::Canvas { .. });
                painter.status_bar(&left, PALE, &right, control);
            }
        }
        match (&self.dialog, source) {
            (Some(EditorDialog::Palette(picker)), Ok(source)) => {
                let layout = self.picker_layout();
                picker.render(framebuffer, font, &layout, &source, pointer);
            }
            (Some(EditorDialog::Color(editor)), Ok(source)) => {
                let layout = self.edit_color_layout();
                editor.render(framebuffer, font, &layout, &source);
            }
            (Some(EditorDialog::Import(preview)), _) => {
                let layout = self.import_layout();
                preview.render(framebuffer, font, &layout);
            }
            _ => {}
        }
    }

    /// The status bar's text: what is selected or under the pointer on the
    /// left, and a hint for the hovered control on the right.
    fn status(&self, source: Option<&SpriteSource<'_>>, hover: Hit) -> (String, String) {
        let (column, row) = grid_position(self.selected);
        let bank = source.map(|s| s.obj.attributes().bank(self.selected).get());
        let pixel = match hover {
            Hit::Canvas { x, y } => source
                .and_then(|s| s.obj.tiles().pixel(self.selected, x.into(), y.into()))
                .map(|local| (x, y, local)),
            _ => None,
        };
        let left = ui_text::get_with(
            "sprites-status",
            &[
                ("tile", Arg::Text(&format!("{:04}", self.selected))),
                ("column", Arg::Number(column as f64)),
                ("row", Arg::Number(row as f64)),
                (
                    "advanced",
                    Arg::Text(if self.advanced { "yes" } else { "no" }),
                ),
                (
                    "has-bank",
                    Arg::Text(if bank.is_some() { "yes" } else { "no" }),
                ),
                ("bank", Arg::Number(bank.unwrap_or_default().into())),
                (
                    "has-pixel",
                    Arg::Text(if pixel.is_some() { "yes" } else { "no" }),
                ),
                ("x", Arg::Number(pixel.map_or(0, |p| p.0).into())),
                ("y", Arg::Number(pixel.map_or(0, |p| p.1).into())),
                (
                    "color",
                    Arg::Text(&format!("{:X}", pixel.map_or(0, |p| p.2))),
                ),
            ],
        );
        let right = match hover {
            Hit::Canvas { .. } => match self.tool {
                ToolSlot::Pencil => ui_text::get("sprites-drag-draw"),
                ToolSlot::Eraser => ui_text::get("sprites-drag-erase"),
                ToolSlot::Eyedropper => ui_text::get("sprites-click-pick-its-color"),
                ToolSlot::Fill => ui_text::get("sprites-click-fill-the-area"),
            }
            .to_owned(),
            Hit::Sheet(tile) => {
                let (column, row) = grid_position(tile);
                ui_text::get_with(
                    "sprites-click-select-tile",
                    &[
                        ("tile", Arg::Text(&format!("{:04}", tile))),
                        ("column", Arg::Text(&column.to_string())),
                        ("row", Arg::Text(&row.to_string())),
                    ],
                )
            }
            Hit::Swatch(0) => ui_text::get("sprites-click-select-transparent-color-0"),
            Hit::Swatch(local) => ui_text::get_with(
                "sprites-click-select-color-double-click-edit",
                &[("local", Arg::Text(&format!("{:X}", local)))],
            ),
            Hit::Bank(bank) => ui_text::get_with(
                "sprites-click-choose-this-tile-s-palette-from-obp",
                &[("bank", Arg::Text(&bank.to_string()))],
            ),
            Hit::PaletteButton => ui_text::get("sprites-click-choose-this-tile-s-palette"),
            Hit::Tool(tool) => ui_text::get_with(
                "sprites-tool-hint",
                &[
                    (
                        "key",
                        Arg::Text(&tool.key().to_ascii_uppercase().to_string()),
                    ),
                    (
                        "tool",
                        Arg::Text(match tool {
                            ToolSlot::Pencil => "pencil",
                            ToolSlot::Eraser => "eraser",
                            ToolSlot::Eyedropper => "eyedropper",
                            ToolSlot::Fill => "fill",
                        }),
                    ),
                ],
            ),
            Hit::GridButton => ui_text::get("sprites-g-show-or-hide-the-pixel-grid"),
            Hit::AdvancedButton if self.advanced => ui_text::get("sprites-a-hide-palette-details"),
            Hit::AdvancedButton => ui_text::get("sprites-a-show-palette-details"),
            Hit::ZoomOut => match self.sheet.zoom().zoomed_out() {
                Some(_) => ui_text::get("sprites-zoom-the-sheet-out"),
                None => ui_text::get("sprites-the-sheet-is-at-its-smallest"),
            },
            Hit::ZoomIn => match self.sheet.zoom().zoomed_in() {
                Some(_) => ui_text::get("sprites-zoom-the-sheet-in"),
                None => ui_text::get("sprites-the-sheet-is-at-its-largest"),
            },
            Hit::SaveButton if source.is_some_and(|s| s.modified) => {
                ui_text::get("sprites-ctrl-s-write-the-changed-files")
            }
            Hit::SaveButton => ui_text::get("sprites-no-unsaved-edits"),
            Hit::ImportButton => ui_text::get("sprites-click-replace-all-sprites-with-a-png-sheet"),
            Hit::None if source.is_none() => String::new(),
            Hit::None if self.sheet.zoom().zoomed_out().is_some() => {
                ui_text::get("sprites-wheel-scroll-shift-wheel-pan")
            }
            Hit::None => ui_text::get("sprites-wheel-scroll-arrows-select"),
        };
        (left, right)
    }
}

/// Draws into one frame with one layout.
struct Painter<'a> {
    framebuffer: &'a mut Framebuffer,
    font: &'a BitmapFont,
    layout: &'a SpriteLayout,
}

impl Painter<'_> {
    fn text(&mut self, text: &str, x: i32, y: i32, color: u8) {
        draw_text(self.framebuffer, self.font, text, x, y, color);
    }

    fn fill(&mut self, rect: Rect, color: u8) {
        fill_rect(
            self.framebuffer,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            color,
        );
    }

    fn outline(&mut self, rect: Rect, color: u8) {
        outline(self.framebuffer, rect, color);
    }

    fn checker(&mut self, rect: Rect, square: i32) {
        checker(self.framebuffer, rect, square);
    }

    /// Top of a text row centred in a bar that starts at `y`.
    fn bar_text_y(&self, y: i32, height: i32) -> i32 {
        y + (height - self.layout.metrics.cell_h) / 2 + 1
    }

    /// Width of `text` in pixels.
    fn cells(&self, text: &str) -> i32 {
        i32::try_from(self.font.text_cells(text)).unwrap_or(i32::MAX) * self.layout.metrics.cell_w
    }

    fn options_bar(&mut self, editor: &SpriteEditor, hover: Hit, modified: bool) {
        let layout = self.layout;
        let bar = layout.options;
        let cell_w = layout.metrics.cell_w;
        fill_rect(self.framebuffer, bar.x, bar.bottom() - 1, bar.width, 1, BAR);
        let text_y = self.bar_text_y(bar.y, bar.height - 1);
        let tile = ui_text::get_with(
            "sprites-tile",
            &[("tile", Arg::Text(&format!("{:04}", editor.selected)))],
        );
        self.text(&tile, 4, text_y, TEXT);

        let tool = editor.tool.name();
        let tag = Rect {
            x: 4 + self.cells(&tile) + cell_w,
            y: bar.y + 1,
            width: self.cells(&tool) + 4,
            height: bar.height - 3,
        };
        self.fill(tag, BAR);
        self.text(&tool, tag.x + 2, text_y, PALE);
        if modified {
            self.text(&modified_label(), tag.right() + cell_w, text_y, ACCENT);
        }

        let zoom = editor.sheet.zoom();
        let buttons: [(Rect, &str, Hit, bool, bool); 4] = [
            (
                layout.grid_button,
                &ui_text::get("sprites-grid"),
                Hit::GridButton,
                editor.grid,
                true,
            ),
            (
                layout.advanced_button,
                &ui_text::get("sprites-advanced"),
                Hit::AdvancedButton,
                editor.advanced,
                true,
            ),
            (
                layout.zoom_out,
                "-",
                Hit::ZoomOut,
                false,
                zoom.zoomed_out().is_some(),
            ),
            (
                layout.zoom_in,
                "+",
                Hit::ZoomIn,
                false,
                zoom.zoomed_in().is_some(),
            ),
        ];
        for (rect, label, hit, on, enabled) in buttons {
            let hovered = hover == hit && enabled;
            if hovered {
                self.fill(rect, BAR);
            }
            let color = match (enabled, on, hovered) {
                (false, ..) => CHECK_LIGHT,
                (true, true, _) => ACCENT,
                (true, false, true) => PALE,
                (true, false, false) => DIM,
            };
            let x = rect.x + (rect.width - self.cells(label)) / 2;
            self.text(label, x, text_y, color);
        }
        self.text(zoom.label(), layout.zoom_label.x, text_y, TEXT);
    }

    fn toolbar(&mut self, ui: &SystemUi, active: ToolSlot, hover: Hit) {
        for (tool, rect) in self.layout.tools {
            let icon = match tool {
                ToolSlot::Pencil => SystemIcon::Sprites,
                ToolSlot::Eraser => SystemIcon::Eraser,
                ToolSlot::Eyedropper => SystemIcon::Eyedropper,
                ToolSlot::Fill => SystemIcon::Fill,
            };
            let inks = if tool == active {
                // The active plate takes the body's color, like the active
                // tab.
                self.fill(rect, BACKGROUND);
                self.outline(rect, ACCENT);
                ACTIVE_TOOL_INKS
            } else if hover == Hit::Tool(tool) {
                self.fill(rect, BAR);
                HOVER_TOOL_INKS
            } else {
                self.fill(rect, BAR);
                IDLE_TOOL_INKS
            };
            let pad = (rect.width - ICON_SIZE as i32) / 2;
            draw_ink_image(
                self.framebuffer,
                ui.icon(icon),
                rect.x + pad,
                rect.y + pad,
                &inks,
            );
        }
    }

    fn canvas(&mut self, editor: &SpriteEditor, source: &SpriteSource<'_>, hover: Hit) {
        let layout = self.layout;
        let canvas = layout.canvas;
        self.outline(layout.canvas_frame, FRAME);
        self.checker(canvas, CANVAS_ZOOM / 2);
        draw_obj_tile_scaled(
            self.framebuffer,
            source.obj,
            editor.selected,
            ObjPaletteChoice::TileDefault,
            (canvas.x, canvas.y),
            CANVAS_ZOOM,
            source.preview.slots(),
        );
        if editor.grid {
            for k in 1..OBJ_TILE_WIDTH as i32 {
                let offset = k * CANVAS_ZOOM;
                fill_rect(
                    self.framebuffer,
                    canvas.x + offset,
                    canvas.y,
                    1,
                    canvas.height,
                    GRID_LINE,
                );
                fill_rect(
                    self.framebuffer,
                    canvas.x,
                    canvas.y + offset,
                    canvas.width,
                    1,
                    GRID_LINE,
                );
            }
        }
        if let Hit::Canvas { x, y } = hover {
            let cell = Rect {
                x: canvas.x + i32::from(x) * CANVAS_ZOOM,
                y: canvas.y + i32::from(y) * CANVAS_ZOOM,
                width: CANVAS_ZOOM,
                height: CANVAS_ZOOM,
            };
            self.outline(cell, PALE);
        }

        let caption = caption(editor.selected, editor.advanced);
        self.text(&caption, layout.caption.x, layout.caption.y, DIM);
    }

    fn sheet(&mut self, editor: &SpriteEditor, source: &SpriteSource<'_>, hover: Hit) {
        let layout = self.layout;
        let view = editor.sheet;
        let zoom = view.zoom();
        let visible = layout.visible;
        self.outline(layout.sheet.inset(-1), FRAME);

        for row in 0..visible.rows {
            for column in 0..visible.columns {
                let Some(tile) = view.tile_in_view(column, row, visible) else {
                    continue;
                };
                let cell = layout.sheet_cell(column, row, zoom);
                self.checker(cell, zoom.tile_size() / 2);
                draw_obj_tile_scaled(
                    self.framebuffer,
                    source.obj,
                    tile,
                    ObjPaletteChoice::TileDefault,
                    (cell.x, cell.y),
                    zoom.scale(),
                    source.preview.slots(),
                );
            }
        }

        let cell_of = |tile: usize| {
            let (column, row) = grid_position(tile);
            layout.sheet_cell(column - view.left_column(), row - view.top_row(), zoom)
        };
        if let Hit::Sheet(tile) = hover
            && tile != editor.selected
        {
            self.outline(cell_of(tile).inset(-1), PALE);
        }
        if view.is_visible(editor.selected, visible) {
            let cell = cell_of(editor.selected);
            self.outline(cell.inset(-1), ACCENT);
        }
        self.scroll_indicators(&view, visible);
    }

    /// Thin bars right of and below the sheet showing which part of the
    /// grid is on screen. The horizontal one only shows while the sheet
    /// is narrower than the grid.
    fn scroll_indicators(&mut self, view: &SheetView, visible: Visible) {
        let sheet = self.layout.sheet;
        let track = |start: i32, length: i32, first: usize, shown: usize, total: usize| {
            let total = total as i32;
            let offset = length * first as i32 / total;
            let size = (length * shown as i32 / total).max(2);
            (start, length, start + offset, size)
        };
        let (y, height, thumb_y, thumb_h) = track(
            sheet.y,
            sheet.height,
            view.top_row(),
            visible.rows,
            OBJ_GRID_ROWS,
        );
        let x = sheet.right() + 2;
        fill_rect(self.framebuffer, x, y, SCROLLBAR, height, BAR);
        fill_rect(self.framebuffer, x, thumb_y, SCROLLBAR, thumb_h, DIM);

        if visible.columns < OBJ_GRID_COLUMNS {
            let (x, width, thumb_x, thumb_w) = track(
                sheet.x,
                sheet.width,
                view.left_column(),
                visible.columns,
                OBJ_GRID_COLUMNS,
            );
            let y = sheet.bottom() + 2;
            fill_rect(self.framebuffer, x, y, width, SCROLLBAR, BAR);
            fill_rect(self.framebuffer, thumb_x, y, thumb_w, SCROLLBAR, DIM);
        }
    }

    fn palette(&mut self, editor: &SpriteEditor, source: &SpriteSource<'_>, hover: Hit) {
        let layout = self.layout;
        let panel = layout.palette;
        let cell_w = layout.metrics.cell_w;
        fill_rect(self.framebuffer, panel.x, panel.y, panel.width, 1, BAR);
        let bank = source.obj.attributes().bank(editor.selected);
        let master = |local: u8| source.obj.palettes().resolve(bank, local);
        let color = editor.color();

        let swatch_label = if layout.advanced {
            &advanced_labels()[1]
        } else {
            &ui_text::get("sprites-palette-2")
        };
        self.text(swatch_label, 4, layout.swatch_y, DIM);
        for (local, &rect) in layout.swatches.iter().enumerate() {
            let local = local as u8;
            let ring = if local == color {
                ACCENT
            } else if hover == Hit::Swatch(local) {
                PALE
            } else {
                BAR
            };
            self.outline(rect, ring);
            let inner = rect.inset(1);
            match master(local) {
                Some(index) => {
                    self.fill(inner, source.preview.slots()[usize::from(index)]);
                    if layout.advanced {
                        let ink = contrasting(source.master.get(index));
                        self.text(
                            &format!("{local:X}"),
                            rect.x + (rect.width - cell_w) / 2,
                            rect.y + 1,
                            ink,
                        );
                    }
                }
                None => {
                    self.checker(inner, 2);
                    if layout.advanced {
                        self.text("0", rect.x + (rect.width - cell_w) / 2, rect.y + 1, TEXT);
                    }
                }
            }
        }
        if !layout.advanced {
            return;
        }

        self.text(&advanced_labels()[0], 4, layout.bank_y, DIM);
        for (number, &rect) in layout.banks.iter().enumerate().take(OBJ_PALETTE_BANKS) {
            let shown = ObjBank::new(number as u8).unwrap_or(ObjBank::ZERO);
            let ink = if shown == bank {
                self.fill(rect, PALE);
                BACKGROUND
            } else if hover == Hit::Bank(number as u8) {
                PALE
            } else {
                DIM
            };
            self.text(
                &number.to_string(),
                rect.x + (rect.width - cell_w) / 2,
                rect.y + 1,
                ink,
            );
        }

        // Master indices are the cartridge's, never the frame's slots.
        self.text(&advanced_labels()[2], 4, layout.master_y, DIM);
        for (local, &rect) in layout.swatches.iter().enumerate() {
            let label = master_label(source, bank, local as u8);
            self.text(&label, rect.x, layout.master_y + 1, TEXT);
        }
        let selected = selection_text(source, bank, color);
        self.text(&selected, 4, layout.selection_y + 1, PALE);
    }

    /// Replaces the canvas, sheet and palette with `message`, wrapped to
    /// the screen.
    fn unavailable(&mut self, message: &str) {
        let layout = self.layout;
        let cell_h = layout.metrics.cell_h;
        // Wrap left of the toolbar.
        let right = layout.tools[0].1.x - 4;
        let columns = (right - 4) / layout.metrics.cell_w;
        let mut y = layout.canvas_frame.y + cell_h;
        self.text(
            &ui_text::get("sprites-can-t-show-this-cartridge-s-sprites"),
            4,
            y,
            TEXT,
        );
        y += cell_h + 2;
        for line in wrap(self.font, message, usize::try_from(columns).unwrap_or(1)) {
            self.text(&line, 4, y, ERROR);
            y += cell_h;
        }
        y += cell_h;
        self.text(
            &ui_text::get("sprites-fix-the-file-then-reopen-the-workspace"),
            4,
            y,
            DIM,
        );
    }

    /// The status bar: `left` in `left_color`, and `right` if both fit.
    fn status_bar(&mut self, left: &str, left_color: u8, right: &str, prefer_right: bool) {
        let bar = self.layout.status;
        self.fill(bar, BAR);
        let y = self.bar_text_y(bar.y, bar.height);
        let columns = (bar.width - 8) / self.layout.metrics.cell_w;
        let length = |text: &str| i32::try_from(self.font.text_cells(text)).unwrap_or(i32::MAX);
        let (left_len, right_len) = (length(left), length(right));
        if left_len + 2 + right_len <= columns {
            self.text(left, 4, y, left_color);
            let x = bar.right() - 4 - self.cells(right);
            self.text(right, x, y, DIM);
        } else if prefer_right {
            self.text(right, 4, y, PALE);
        } else {
            self.text(left, 4, y, left_color);
        }
    }

    /// The button that opens the palette picker, naming the selected
    /// tile's palette.
    fn palette_button(&mut self, editor: &SpriteEditor, source: &SpriteSource<'_>, hover: Hit) {
        let rect = self.layout.palette_button;
        let bank = source.obj.attributes().bank(editor.selected);
        let label = ui_text::get_with(
            "sprites-palette-button",
            &[
                ("bank", Arg::Number(bank.get().into())),
                (
                    "advanced",
                    Arg::Text(if editor.advanced { "yes" } else { "no" }),
                ),
            ],
        );
        let hovered = hover == Hit::PaletteButton;
        self.fill(rect, if hovered { HOVER_SHADE } else { BAR });
        self.outline(rect, DIM);
        let x = rect.x + (rect.width - self.cells(&label)) / 2;
        let y = self.bar_text_y(rect.y, rect.height);
        self.text(&label, x, y, if hovered { TEXT } else { PALE });
    }

    /// The Import button, held down while its file dialog or preview is
    /// open.
    fn import_button(&mut self, held: bool, hover: Hit) {
        let rect = self.layout.import_button;
        let hovered = hover == Hit::ImportButton;
        let (face, ink) = if held || hovered {
            (HOVER_SHADE, TEXT)
        } else {
            (BAR, PALE)
        };
        self.fill(rect, face);
        self.outline(rect, if held { TEXT } else { DIM });
        let x = rect.x + (rect.width - self.cells(&import_label())) / 2;
        let y = self.bar_text_y(rect.y, rect.height);
        self.text(&import_label(), x, y, ink);
    }

    /// The Save button, which only looks pressable while there is
    /// something to save.
    fn save_button(&mut self, modified: bool, hover: Hit) {
        let rect = self.layout.save_button;
        let hovered = modified && hover == Hit::SaveButton;
        self.fill(rect, if hovered { HOVER_SHADE } else { BAR });
        self.outline(rect, if modified { DIM } else { BAR });
        let color = match (modified, hovered) {
            (false, _) => CHECK_LIGHT,
            (true, true) => TEXT,
            (true, false) => PALE,
        };
        let x = rect.x + (rect.width - self.cells(&save_label())) / 2;
        let y = self.bar_text_y(rect.y, rect.height);
        self.text(&save_label(), x, y, color);
    }
}

/// Draws a one-pixel outline just inside `rect`.
pub(super) fn outline(framebuffer: &mut Framebuffer, rect: Rect, color: u8) {
    let Rect {
        x,
        y,
        width,
        height,
    } = rect;
    fill_rect(framebuffer, x, y, width, 1, color);
    fill_rect(framebuffer, x, y + height - 1, width, 1, color);
    fill_rect(framebuffer, x, y, 1, height, color);
    fill_rect(framebuffer, x + width - 1, y, 1, height, color);
}

/// Fills `rect` with the transparency checkerboard of `square`-pixel
/// squares, the top-left one dark.
pub(super) fn checker(framebuffer: &mut Framebuffer, rect: Rect, square: i32) {
    let square = square.max(1);
    let mut y = rect.y;
    while y < rect.bottom() {
        let mut x = rect.x;
        while x < rect.right() {
            let parity = ((x - rect.x) / square + (y - rect.y) / square) % 2;
            let color = if parity == 0 { CHECK_DARK } else { CHECK_LIGHT };
            let w = square.min(rect.right() - x);
            let h = square.min(rect.bottom() - y);
            fill_rect(framebuffer, x, y, w, h, color);
            x += square;
        }
        y += square;
    }
}

/// The master index local color `local` of `bank` maps to, as two hex
/// digits, or `--` for the transparent local 0.
fn master_label(source: &SpriteSource<'_>, bank: ObjBank, local: u8) -> String {
    source
        .obj
        .palettes()
        .resolve(bank, local)
        .map_or_else(|| "--".to_owned(), |master| format!("{master:02X}"))
}

/// How local color `local` of `bank` resolves: its master index and that
/// master entry's RGB value.
fn selection_text(source: &SpriteSource<'_>, bank: ObjBank, local: u8) -> String {
    match source.obj.palettes().resolve(bank, local) {
        Some(master) => {
            let Rgb { r, g, b } = source.master.get(master);
            ui_text::get_with(
                "sprites-selected-color",
                &[
                    ("local", Arg::Text(&format!("{:X}", local))),
                    ("master", Arg::Text(&master.to_string())),
                    ("master-hex", Arg::Text(&format!("{:02X}", master))),
                    ("r", Arg::Text(&format!("{:02X}", r))),
                    ("g", Arg::Text(&format!("{:02X}", g))),
                    ("b", Arg::Text(&format!("{:02X}", b))),
                ],
            )
        }
        None => ui_text::get_with(
            "sprites-selected-local-transparent",
            &[("local", Arg::Text(&format!("{:X}", local)))],
        ),
    }
}

/// What the palette bank `bank` is called: `Palette n` normally, `OBPn` in
/// Advanced mode.
pub(super) fn bank_name(bank: ObjBank, advanced: bool) -> String {
    if advanced {
        format!("OBP{}", bank.get())
    } else {
        ui_text::get_with(
            "sprites-palette-name",
            &[("bank", Arg::Text(&(bank.get().to_string())))],
        )
    }
}

/// System palette index of text that reads on a swatch of `color`.
pub(super) fn contrasting(color: Rgb) -> u8 {
    let luma = 299 * u32::from(color.r) + 587 * u32::from(color.g) + 114 * u32::from(color.b);
    if luma > 140_000 { BACKGROUND } else { TEXT }
}

#[cfg(test)]
mod tests;
