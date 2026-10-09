//! Drawing the sprite editor. UI parts use system palette indices; tile
//! pixels use the frame indices [`ObjPreviewPalette`] gives their master
//! colors, so they show the cartridge's exact colors.
//!
//! [`ObjPreviewPalette`]: glaux_video::preview::ObjPreviewPalette

use glaux_input::Point;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, ObjBank};
use glaux_machine::palette::Rgb;
use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_GRID_ROWS, OBJ_TILE_WIDTH};
use glaux_machine::system_rom::{SystemIcon, SystemUi};
use glaux_video::draw::{draw_ink_image, draw_obj_tile_scaled, draw_text, fill_rect};

use super::layout::{ADVANCED_LABELS, CANVAS_ZOOM, Hit, SCROLLBAR, SpriteLayout, ToolSlot};
use super::sheet::{SheetView, Visible, grid_position};
use super::{SpriteEditor, SpriteSource};
use crate::rom::ICON_SIZE;
use crate::system::workspace::{BAR, OUTLINE, Rect};
use crate::system::{ACCENT, BACKGROUND, DIM, ERROR, PALE, TEXT};

/// The two colors of the checkerboard behind transparent pixels.
const CHECK_DARK: u8 = 12;
const CHECK_LIGHT: u8 = 13;
/// Frames around the canvas and the sheet.
const FRAME: u8 = DIM;
/// Pixel grid lines over the canvas.
const GRID_LINE: u8 = BACKGROUND;
/// Ink of a disabled tool icon's body and shading.
const DISABLED_INKS: [u8; 3] = [OUTLINE, CHECK_DARK, CHECK_LIGHT];
const DISABLED_HOVER_INKS: [u8; 3] = [OUTLINE, CHECK_LIGHT, DIM];
/// Labels the options bar shows left of its buttons.
const VIEW_ONLY: &str = "VIEW ONLY";

impl SpriteEditor {
    /// Draws the editor below the tab bar from `source`, or explains why
    /// there is nothing to show. `pointer` highlights what it hovers.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        ui: &SystemUi,
        source: Result<SpriteSource<'_>, &str>,
        pointer: Option<Point>,
    ) {
        let layout = self.layout();
        let hover = pointer.map_or(Hit::None, |at| layout.hit(at, &self.sheet));
        let mut painter = Painter {
            framebuffer,
            font,
            layout: &layout,
        };

        painter.options_bar(self, hover);
        painter.toolbar(ui, hover);
        match source {
            Ok(source) => {
                painter.canvas(self, &source, hover);
                painter.sheet(self, &source, hover);
                painter.palette(self, &source, hover);
            }
            Err(message) => painter.unavailable(message),
        }
        let (left, right) = self.status(source.ok().as_ref(), hover);
        // A hint about the control under the pointer matters more than the
        // tile summary when both don't fit.
        let control = !matches!(hover, Hit::None | Hit::Canvas { .. });
        painter.status_bar(&left, &right, control);
    }

    /// The status bar's text: what is selected or under the pointer on the
    /// left, and a hint for the hovered control on the right.
    fn status(&self, source: Option<&SpriteSource<'_>>, hover: Hit) -> (String, String) {
        let (column, row) = grid_position(self.selected);
        let mut left = format!("Tile {:04}", self.selected);
        if self.advanced {
            left += &format!(" ({column},{row})");
            if let Some(source) = source {
                let bank = source.obj.attributes().bank(self.selected);
                left += &format!(" | OBP {} | 4 bpp", bank.get());
            }
        }
        let right = match hover {
            Hit::Canvas { x, y } => {
                if let Some(local) =
                    source.and_then(|s| s.obj.tiles().pixel(self.selected, x.into(), y.into()))
                {
                    left += &format!(" | X:{x} Y:{y} | Color:{local:X}");
                }
                "View only".to_owned()
            }
            Hit::Sheet(tile) => {
                let (column, row) = grid_position(tile);
                format!("Click: select tile {tile:04} ({column},{row})")
            }
            Hit::Swatch(local) => format!("Click: select color {local:X}"),
            Hit::Bank(bank) => format!("OBP {bank}: read-only"),
            Hit::Tool(tool) => format!("{}: not available yet", tool.name()),
            Hit::GridButton => "G: show or hide the pixel grid".to_owned(),
            Hit::AdvancedButton if self.advanced => "A: hide palette details".to_owned(),
            Hit::AdvancedButton => "A: show palette details".to_owned(),
            Hit::ZoomOut => match self.sheet.zoom().zoomed_out() {
                Some(_) => "-: zoom the sheet out".to_owned(),
                None => "The sheet is at its smallest".to_owned(),
            },
            Hit::ZoomIn => match self.sheet.zoom().zoomed_in() {
                Some(_) => "+: zoom the sheet in".to_owned(),
                None => "The sheet is at its largest".to_owned(),
            },
            Hit::None if source.is_none() => String::new(),
            Hit::None if self.sheet.zoom().zoomed_out().is_some() => {
                "Wheel: scroll  Shift+wheel: pan".to_owned()
            }
            Hit::None => "Wheel: scroll  Arrows: select".to_owned(),
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

    /// A one-pixel outline just inside `rect`.
    fn outline(&mut self, rect: Rect, color: u8) {
        let Rect {
            x,
            y,
            width,
            height,
        } = rect;
        let fb = &mut *self.framebuffer;
        fill_rect(fb, x, y, width, 1, color);
        fill_rect(fb, x, y + height - 1, width, 1, color);
        fill_rect(fb, x, y, 1, height, color);
        fill_rect(fb, x + width - 1, y, 1, height, color);
    }

    /// Fills `rect` with a checkerboard of `square`-pixel squares, the
    /// top-left one dark.
    fn checker(&mut self, rect: Rect, square: i32) {
        let square = square.max(1);
        let mut y = rect.y;
        while y < rect.bottom() {
            let mut x = rect.x;
            while x < rect.right() {
                let parity = ((x - rect.x) / square + (y - rect.y) / square) % 2;
                let color = if parity == 0 { CHECK_DARK } else { CHECK_LIGHT };
                let w = square.min(rect.right() - x);
                let h = square.min(rect.bottom() - y);
                fill_rect(self.framebuffer, x, y, w, h, color);
                x += square;
            }
            y += square;
        }
    }

    /// Top of a text row centred in a bar that starts at `y`.
    fn bar_text_y(&self, y: i32, height: i32) -> i32 {
        y + (height - self.layout.metrics.cell_h) / 2 + 1
    }

    fn cells(&self, text: &str) -> i32 {
        i32::try_from(text.chars().count()).unwrap_or(i32::MAX) * self.layout.metrics.cell_w
    }

    fn options_bar(&mut self, editor: &SpriteEditor, hover: Hit) {
        let layout = self.layout;
        let bar = layout.options;
        let cell_w = layout.metrics.cell_w;
        fill_rect(self.framebuffer, bar.x, bar.bottom() - 1, bar.width, 1, BAR);
        let text_y = self.bar_text_y(bar.y, bar.height - 1);
        let tile = format!("Tile {:04}", editor.selected);
        self.text(&tile, 4, text_y, TEXT);

        // The tag tells that the editor shows sprites but cannot change
        // them yet.
        let tag = Rect {
            x: 4 + self.cells(&tile) + cell_w,
            y: bar.y + 1,
            width: self.cells(VIEW_ONLY) + 4,
            height: bar.height - 3,
        };
        self.fill(tag, BAR);
        self.text(VIEW_ONLY, tag.x + 2, text_y, DIM);

        let zoom = editor.sheet.zoom();
        let buttons = [
            (
                layout.grid_button,
                "Grid",
                Hit::GridButton,
                editor.grid,
                true,
            ),
            (
                layout.advanced_button,
                "Advanced",
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

    fn toolbar(&mut self, ui: &SystemUi, hover: Hit) {
        for (tool, rect) in self.layout.tools {
            self.fill(rect, BAR);
            let icon = match tool {
                ToolSlot::Pencil => SystemIcon::Sprites,
                ToolSlot::Eraser => SystemIcon::Eraser,
                ToolSlot::Eyedropper => SystemIcon::Eyedropper,
            };
            let inks = if hover == Hit::Tool(tool) {
                DISABLED_HOVER_INKS
            } else {
                DISABLED_INKS
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

        let (column, row) = grid_position(editor.selected);
        let caption = if editor.advanced {
            format!("Tile {:04} ({column},{row})", editor.selected)
        } else {
            format!("Tile {:04}", editor.selected)
        };
        self.text(&caption, layout.canvas_frame.x, layout.caption_y, DIM);
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
            ADVANCED_LABELS[1]
        } else {
            "Palette"
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

        self.text(ADVANCED_LABELS[0], 4, layout.bank_y, DIM);
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
        self.text(ADVANCED_LABELS[2], 4, layout.master_y, DIM);
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
        self.text("Can't show this cartridge's sprites:", 4, y, TEXT);
        y += cell_h + 2;
        for line in wrap(message, usize::try_from(columns).unwrap_or(1)) {
            self.text(&line, 4, y, ERROR);
            y += cell_h;
        }
        y += cell_h;
        self.text("Fix the file, then reopen the workspace.", 4, y, DIM);
    }

    fn status_bar(&mut self, left: &str, right: &str, prefer_right: bool) {
        let bar = self.layout.status;
        self.fill(bar, BAR);
        let y = self.bar_text_y(bar.y, bar.height);
        let columns = (bar.width - 8) / self.layout.metrics.cell_w;
        let (left_len, right_len) = (left.chars().count() as i32, right.chars().count() as i32);
        if left_len + 2 + right_len <= columns {
            self.text(left, 4, y, PALE);
            let x = bar.right() - 4 - self.cells(right);
            self.text(right, x, y, DIM);
        } else if prefer_right {
            self.text(right, 4, y, PALE);
        } else {
            self.text(left, 4, y, PALE);
        }
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
            format!(
                "Selected: Local {local:X} -> Master {master} (${master:02X}) -> #{r:02X}{g:02X}{b:02X}"
            )
        }
        None => format!("Selected: Local {local:X} -> transparent"),
    }
}

/// System palette index of text that reads on a swatch of `color`.
fn contrasting(color: Rgb) -> u8 {
    let luma = 299 * u32::from(color.r) + 587 * u32::from(color.g) + 114 * u32::from(color.b);
    if luma > 140_000 { BACKGROUND } else { TEXT }
}

/// `text` broken into lines of at most `columns` characters, at spaces
/// where possible.
fn wrap(text: &str, columns: usize) -> Vec<String> {
    let columns = columns.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word: Vec<char> = word.chars().collect();
        while word.len() > columns {
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            lines.push(word.drain(..columns).collect());
        }
        let word: String = word.into_iter().collect();
        let len = line.chars().count();
        if len > 0 && len + 1 + word.chars().count() > columns {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaux_machine::font::FontMetrics;
    use glaux_machine::obj_palette::{OBJ_PALETTES_BYTES, ObjPalettes};
    use glaux_machine::palette::Palette;
    use glaux_machine::sprite::{
        OBJ_TILE_BYTES, OBJ_TILE_CAPACITY, ObjTileAttributes, ObjTileBank, ObjVram,
    };
    use glaux_video::preview::ObjPreviewPalette;
    use pretty_assertions::assert_eq;

    use super::super::Metrics;
    use crate::system::workspace::SYSTEM_COLORS;

    const METRICS: Metrics = Metrics {
        cell_w: 6,
        cell_h: 10,
        top: 16,
    };

    /// Tile 0 is solid local 2 and tile 1 is empty. OBP0 maps local `n` to
    /// master `200 + n`, so frame slots and master indices differ.
    struct Fixture {
        obj: ObjVram,
        master: Palette,
        preview: ObjPreviewPalette,
    }

    impl Fixture {
        fn new() -> Self {
            let mut tiles = vec![0; OBJ_TILE_CAPACITY * OBJ_TILE_BYTES];
            tiles[..OBJ_TILE_BYTES].fill(0x22);
            let mut obp = [0; OBJ_PALETTES_BYTES];
            for (local, entry) in obp.iter_mut().enumerate().take(16).skip(1) {
                *entry = 200 + local as u8;
            }
            let obj = ObjVram::new(
                ObjTileBank::from_bytes(&tiles).unwrap(),
                ObjTileAttributes::new(),
                ObjPalettes::from_bytes(&obp).unwrap(),
            );
            let mut entries = *Palette::provisional().entries();
            entries[202] = Rgb::from_hex(0x62a7d5);
            let master = Palette::new(entries);
            let preview = ObjPreviewPalette::new(
                &Palette::provisional(),
                SYSTEM_COLORS,
                &master,
                obj.palettes(),
            )
            .unwrap();
            Self {
                obj,
                master,
                preview,
            }
        }

        fn source(&self) -> SpriteSource<'_> {
            SpriteSource {
                obj: &self.obj,
                master: &self.master,
                preview: &self.preview,
            }
        }
    }

    fn render(editor: &SpriteEditor, fixture: &Fixture) -> Framebuffer {
        let mut fb = Framebuffer::new();
        let font = BitmapFont::new(FontMetrics::SYSTEM);
        editor.render(
            &mut fb,
            &font,
            &SystemUi::default(),
            Ok(fixture.source()),
            None,
        );
        fb
    }

    fn pixels(fb: &Framebuffer, rect: Rect) -> Vec<u8> {
        (rect.y..rect.bottom())
            .flat_map(|y| (rect.x..rect.right()).map(move |x| (x, y)))
            .map(|(x, y)| fb.pixel(x, y).unwrap())
            .collect()
    }

    #[test]
    fn the_canvas_shows_the_tile_in_its_cartridge_color() {
        let fixture = Fixture::new();
        let editor = SpriteEditor::new(METRICS);
        let fb = render(&editor, &fixture);
        let canvas = editor.layout().canvas;
        let slot = fixture.preview.slot(202).unwrap();
        assert_eq!(fixture.preview.palette().get(slot), Rgb::from_hex(0x62a7d5));
        // Inside the first pixel, clear of the grid lines.
        assert_eq!(fb.pixel(canvas.x + 5, canvas.y + 5), Some(slot));
        assert_eq!(
            fb.pixel(canvas.right() - 1, canvas.bottom() - 1),
            Some(slot)
        );
    }

    #[test]
    fn transparent_pixels_show_a_checkerboard() {
        let fixture = Fixture::new();
        let mut editor = SpriteEditor::new(METRICS);
        editor.selected = 1;
        editor.grid = false;
        let fb = render(&editor, &fixture);
        let canvas = pixels(&fb, editor.layout().canvas);
        assert!(canvas.contains(&CHECK_DARK) && canvas.contains(&CHECK_LIGHT));
        assert!(canvas.iter().all(|&p| p == CHECK_DARK || p == CHECK_LIGHT));
    }

    #[test]
    fn empty_tiles_stay_visible_in_the_sheet() {
        let fixture = Fixture::new();
        let editor = SpriteEditor::new(METRICS);
        let fb = render(&editor, &fixture);
        let layout = editor.layout();
        let zoom = editor.sheet.zoom();
        let slot = fixture.preview.slot(202).unwrap();
        assert!(
            pixels(&fb, layout.sheet_cell(0, 0, zoom))
                .iter()
                .all(|&p| p == slot)
        );
        // The last visible tile is empty capacity, drawn as a checkerboard.
        let last = layout.sheet_cell(layout.visible.columns - 1, layout.visible.rows - 1, zoom);
        let empty = pixels(&fb, last);
        assert!(empty.contains(&CHECK_DARK) && empty.contains(&CHECK_LIGHT));
        assert!(!empty.contains(&slot));
    }

    #[test]
    fn the_grid_toggle_only_changes_grid_lines() {
        let fixture = Fixture::new();
        let mut editor = SpriteEditor::new(METRICS);
        let with_grid = render(&editor, &fixture);
        editor.grid = false;
        let without = render(&editor, &fixture);
        let canvas = editor.layout().canvas;
        let mut changed = 0;
        for y in 0..240 {
            for x in 0..320 {
                if with_grid.pixel(x, y) == without.pixel(x, y) {
                    continue;
                }
                changed += 1;
                let on_line =
                    (x - canvas.x) % CANVAS_ZOOM == 0 || (y - canvas.y) % CANVAS_ZOOM == 0;
                assert!(canvas.contains_xy(x, y) && on_line, "({x}, {y})");
                assert_eq!(with_grid.pixel(x, y), Some(GRID_LINE));
            }
        }
        assert!(changed > 0);
    }

    #[test]
    fn advanced_text_names_cartridge_master_indices_not_frame_slots() {
        let fixture = Fixture::new();
        let source = fixture.source();
        let slot = fixture.preview.slot(202).unwrap();
        assert_ne!(slot, 202);
        assert_eq!(
            selection_text(&source, ObjBank::ZERO, 2),
            "Selected: Local 2 -> Master 202 ($CA) -> #62A7D5"
        );
        assert_eq!(
            selection_text(&source, ObjBank::ZERO, 0),
            "Selected: Local 0 -> transparent"
        );
        assert_eq!(master_label(&source, ObjBank::ZERO, 2), "CA");
        assert_eq!(master_label(&source, ObjBank::ZERO, 0), "--");
    }

    #[test]
    fn advanced_mode_adds_rows_without_moving_the_canvas() {
        let fixture = Fixture::new();
        let mut editor = SpriteEditor::new(METRICS);
        let normal = render(&editor, &fixture);
        editor.advanced = true;
        let advanced = render(&editor, &fixture);
        let canvas = editor.layout().canvas;
        assert_eq!(pixels(&normal, canvas), pixels(&advanced, canvas));
        // The OBP row shows the tile's bank highlighted.
        let bank0 = editor.layout().banks[0];
        assert!(pixels(&advanced, bank0).contains(&PALE));
    }

    #[test]
    fn status_reports_the_hovered_pixel_and_tile_coordinates() {
        let fixture = Fixture::new();
        let mut editor = SpriteEditor::new(METRICS);
        let source = fixture.source();
        let (left, _) = editor.status(Some(&source), Hit::Canvas { x: 3, y: 5 });
        assert_eq!(left, "Tile 0000 | X:3 Y:5 | Color:2");
        editor.selected = 12;
        editor.advanced = true;
        let (left, right) = editor.status(Some(&source), Hit::Sheet(1023));
        assert_eq!(left, "Tile 0012 (12,0) | OBP 0 | 4 bpp");
        assert_eq!(right, "Click: select tile 1023 (15,63)");
        let (_, right) = editor.status(Some(&source), Hit::Tool(ToolSlot::Eraser));
        assert_eq!(right, "Eraser: not available yet");
    }

    #[test]
    fn long_messages_wrap_at_spaces() {
        assert_eq!(
            wrap("Invalid obp.bin: must be exactly 128 bytes", 16),
            ["Invalid obp.bin:", "must be exactly", "128 bytes"]
        );
        assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    }
}
