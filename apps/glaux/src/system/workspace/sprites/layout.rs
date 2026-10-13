//! Where each part of the sprite editor sits on the 320×240 screen.
//! Rendering and hit-testing both read these rectangles.

use glaux_input::Point;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, OBJ_PALETTE_COLORS};
use glaux_machine::sprite::{OBJ_TILE_CAPACITY, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH};

use super::sheet::{SheetView, Visible, Zoom, grid_position};
use crate::system::workspace::Rect;

/// Canvas pixels per tile pixel.
pub const CANVAS_ZOOM: i32 = 12;
/// Side of a tool button, which holds one 16×16 icon.
pub const TOOL_PLATE: i32 = 18;
/// Space between the screen edges and the panels.
const MARGIN: i32 = 4;
/// Space between neighbouring panels.
const GAP: i32 = 4;
/// Thickness of the sheet's scroll indicators, with the pixel between
/// them and the sheet frame.
pub const SCROLLBAR: i32 = 2;
/// Horizontal padding inside a text button.
const BUTTON_PAD: i32 = 2;
/// Space between neighbouring palette cells, wide enough that the
/// two-digit master indices under them read as separate numbers.
const SWATCH_GAP: i32 = 3;
/// Character cells reserved for the palette panel's row labels.
const LABEL_CELLS: i32 = 7;
/// The labels of the buttons under the canvas caption.
pub const SAVE_LABEL: &str = "Save";
pub const IMPORT_LABEL: &str = "Import";
/// Row labels of the palette panel in Advanced mode, top to bottom.
pub const ADVANCED_LABELS: [&str; 3] = ["OBP", "Local", "Master"];

/// Font cell size and where the editor's area begins below the tab bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    pub cell_w: i32,
    pub cell_h: i32,
    pub top: i32,
}

/// The sprite editor's tools, one per toolbar slot, top to bottom.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ToolSlot {
    /// Draws the selected color.
    #[default]
    Pencil,
    /// Draws transparent local 0.
    Eraser,
    /// Selects the color of the pixel it touches.
    Eyedropper,
    /// Recolors the area of one color around a pixel, within the tile.
    Fill,
}

impl ToolSlot {
    pub const ALL: [Self; 4] = [Self::Pencil, Self::Eraser, Self::Eyedropper, Self::Fill];

    pub fn name(self) -> &'static str {
        match self {
            Self::Pencil => "Pencil",
            Self::Eraser => "Eraser",
            Self::Eyedropper => "Eyedropper",
            Self::Fill => "Fill",
        }
    }

    /// The key that selects the tool, as typed without Shift.
    pub fn key(self) -> char {
        match self {
            Self::Pencil => 'p',
            Self::Eraser => 'e',
            Self::Eyedropper => 'i',
            Self::Fill => 'f',
        }
    }

    /// The tool `key` selects, ignoring ASCII case.
    pub fn from_key(key: char) -> Option<Self> {
        let key = key.to_ascii_lowercase();
        Self::ALL.into_iter().find(|tool| tool.key() == key)
    }

    /// Whether the tool keeps acting while the pointer is dragged.
    pub fn drags(self) -> bool {
        !matches!(self, Self::Fill)
    }
}

/// What a screen position falls on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// A pixel of the enlarged tile, in tile coordinates.
    Canvas {
        x: u8,
        y: u8,
    },
    /// A tile shown in the sheet.
    Sheet(usize),
    Tool(ToolSlot),
    /// A local color of the palette panel.
    Swatch(u8),
    /// An object palette bank in Advanced mode's OBP row.
    Bank(u8),
    GridButton,
    AdvancedButton,
    ZoomOut,
    ZoomIn,
    SaveButton,
    /// Replaces the sprites with a PNG spritesheet.
    ImportButton,
    /// Opens the palette picker for the selected tile.
    PaletteButton,
    None,
}

/// The editor's rectangles for one mode and sheet zoom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteLayout {
    pub metrics: Metrics,
    pub options: Rect,
    pub status: Rect,
    pub palette: Rect,
    /// The canvas's frame, one pixel around [`canvas`](Self::canvas).
    pub canvas_frame: Rect,
    pub canvas: Rect,
    /// Where the selected tile's caption goes, wide enough for the widest
    /// one: under the canvas normally, and right of the OBP banks in
    /// Advanced mode, where the canvas column has no row to spare.
    pub caption: Rect,
    /// The tiles the sheet shows, exactly `visible` whole tiles.
    pub sheet: Rect,
    pub visible: Visible,
    pub tools: [(ToolSlot, Rect); 4],
    pub grid_button: Rect,
    pub advanced_button: Rect,
    pub zoom_out: Rect,
    pub zoom_label: Rect,
    pub zoom_in: Rect,
    /// Saves the changes; the left half of the row under the canvas
    /// caption.
    pub save_button: Rect,
    /// Imports a spritesheet; the right half of that row.
    pub import_button: Rect,
    /// Opens the palette picker; under that row, as wide as the canvas.
    pub palette_button: Rect,
    /// Tops of the palette panel's rows, the swatch row's first.
    pub swatch_y: i32,
    /// The 16 local color cells, by local index.
    pub swatches: [Rect; OBJ_PALETTE_COLORS],
    /// The 8 bank cells; only shown and hit in Advanced mode.
    pub banks: [Rect; OBJ_PALETTE_BANKS],
    /// Tops of the OBP, Master and selection rows in Advanced mode.
    pub bank_y: i32,
    pub master_y: i32,
    pub selection_y: i32,
    pub advanced: bool,
}

impl SpriteLayout {
    pub fn new(metrics: Metrics, advanced: bool, zoom: Zoom) -> Self {
        let Metrics { cell_w, cell_h, .. } = metrics;
        let width = WIDTH as i32;
        let bar = cell_h + 2;

        let options = Rect {
            x: 0,
            y: metrics.top,
            width,
            height: bar,
        };
        let status = Rect {
            x: 0,
            y: HEIGHT as i32 - bar,
            width,
            height: bar,
        };
        let palette_rows = if advanced { 4 } else { 1 };
        let palette_height = palette_rows * cell_h + 4;
        let palette = Rect {
            x: 0,
            y: status.y - palette_height,
            width,
            height: palette_height,
        };

        let main_top = options.bottom() + 2;
        let main_bottom = palette.y - 2;

        let canvas_side = OBJ_TILE_WIDTH as i32 * CANVAS_ZOOM;
        let canvas_frame = Rect {
            x: MARGIN,
            y: main_top,
            width: canvas_side + 2,
            height: canvas_side + 2,
        };
        let canvas = canvas_frame.inset(1);

        let tool_x = width - 2 - TOOL_PLATE;
        let tools = {
            let mut y = main_top;
            ToolSlot::ALL.map(|tool| {
                let rect = Rect {
                    x: tool_x,
                    y,
                    width: TOOL_PLATE,
                    height: TOOL_PLATE,
                };
                y += TOOL_PLATE + 1;
                (tool, rect)
            })
        };

        // The sheet sits right of the canvas, inside a one-pixel frame,
        // with room for its scroll indicators on the right and below.
        let sheet_x = canvas_frame.right() + GAP + 1;
        let sheet_y = main_top + 1;
        let room_w = tool_x - GAP - SCROLLBAR - 2 - sheet_x;
        let room_h = main_bottom - SCROLLBAR - 2 - sheet_y;
        let tile = zoom.tile_size();
        let visible = Visible::new(
            usize::try_from(room_w / tile).unwrap_or(0),
            usize::try_from(room_h / tile).unwrap_or(0),
        );
        let sheet = Rect {
            x: sheet_x,
            y: sheet_y,
            width: visible.columns as i32 * tile,
            height: visible.rows as i32 * tile,
        };

        // Options bar buttons, right to left from the right margin. Each
        // takes `cells` text cells, `pad` pixels on both sides and leaves
        // `gap` pixels before the next one.
        let mut right = width - MARGIN;
        let mut button = |cells: i32, pad: i32, gap: i32| {
            let w = cells * cell_w + 2 * pad;
            right -= w;
            let x = right;
            right -= gap;
            Rect {
                x,
                y: options.y,
                width: w,
                height: bar - 1,
            }
        };
        let zoom_in = button(1, BUTTON_PAD, 0);
        let zoom_label = button(2, 0, 0);
        let zoom_out = button(1, BUTTON_PAD, GAP);
        let advanced_button = button(text_cells("Advanced"), BUTTON_PAD, GAP);
        let grid_button = button(text_cells("Grid"), BUTTON_PAD, 0);

        let caption_cells = text_cells(&caption(OBJ_TILE_CAPACITY - 1, advanced));
        let under_canvas = Rect {
            x: canvas_frame.x,
            y: canvas_frame.bottom() + 2,
            width: caption_cells * cell_w,
            height: cell_h,
        };
        let button_row = Rect {
            x: canvas_frame.x,
            y: if advanced {
                canvas_frame.bottom() + 2
            } else {
                under_canvas.bottom() + 2
            },
            width: canvas_frame.width,
            height: bar,
        };
        let half = (button_row.width - GAP) / 2;
        let save_button = Rect {
            width: half,
            ..button_row
        };
        let import_button = Rect {
            x: button_row.right() - half,
            width: half,
            ..button_row
        };
        let palette_button = Rect {
            y: button_row.bottom() + 2,
            ..button_row
        };

        let swatch_x = MARGIN + LABEL_CELLS * cell_w;
        let pitch = (2 * cell_w).max(cell_h) + SWATCH_GAP;
        let row = |n: i32| palette.y + 2 + n * cell_h;
        let (bank_y, swatch_y, master_y, selection_y) = if advanced {
            (row(0), row(1), row(2), row(3))
        } else {
            (row(0), row(0), row(0), row(0))
        };
        let cell = |i: usize, y: i32| Rect {
            x: swatch_x + i as i32 * pitch,
            y,
            width: pitch - SWATCH_GAP,
            height: cell_h,
        };

        let banks: [Rect; OBJ_PALETTE_BANKS] = std::array::from_fn(|i| cell(i, bank_y));
        let caption = if advanced {
            Rect {
                x: banks[OBJ_PALETTE_BANKS - 1].right() + 2 * GAP,
                y: bank_y,
                ..under_canvas
            }
        } else {
            under_canvas
        };

        Self {
            metrics,
            options,
            status,
            palette,
            canvas_frame,
            canvas,
            caption,
            sheet,
            visible,
            tools,
            grid_button,
            advanced_button,
            zoom_out,
            zoom_label,
            zoom_in,
            save_button,
            import_button,
            palette_button,
            swatch_y,
            swatches: std::array::from_fn(|i| cell(i, swatch_y)),
            banks,
            bank_y,
            master_y,
            selection_y,
            advanced,
        }
    }

    /// The screen rectangle of the sheet cell `column` tiles right of and
    /// `row` tiles below its top-left tile.
    pub fn sheet_cell(&self, column: usize, row: usize, zoom: Zoom) -> Rect {
        let tile = zoom.tile_size();
        Rect {
            x: self.sheet.x + column as i32 * tile,
            y: self.sheet.y + row as i32 * tile,
            width: tile,
            height: tile,
        }
    }

    /// The tile pixel under `point` as if the canvas went on past its
    /// edges, so drags that leave it keep their direction.
    pub fn canvas_pixel(&self, point: Point) -> (i32, i32) {
        (
            (i32::from(point.x) - self.canvas.x).div_euclid(CANVAS_ZOOM),
            (i32::from(point.y) - self.canvas.y).div_euclid(CANVAS_ZOOM),
        )
    }

    /// What `point` falls on, with the sheet scrolled as in `view`.
    pub fn hit(&self, point: Point, view: &SheetView) -> Hit {
        let (x, y) = (i32::from(point.x), i32::from(point.y));
        let buttons = [
            (self.grid_button, Hit::GridButton),
            (self.advanced_button, Hit::AdvancedButton),
            (self.zoom_out, Hit::ZoomOut),
            (self.zoom_in, Hit::ZoomIn),
            (self.save_button, Hit::SaveButton),
            (self.import_button, Hit::ImportButton),
            (self.palette_button, Hit::PaletteButton),
        ];
        if let Some(&(_, hit)) = buttons.iter().find(|(rect, _)| rect.contains(point)) {
            return hit;
        }
        if let Some(&(tool, _)) = self.tools.iter().find(|(_, rect)| rect.contains(point)) {
            return Hit::Tool(tool);
        }
        if self.canvas.contains(point) {
            // Both offsets are inside the canvas, so below the tile size.
            let px = (x - self.canvas.x) / CANVAS_ZOOM;
            let py = (y - self.canvas.y) / CANVAS_ZOOM;
            debug_assert!(px < OBJ_TILE_WIDTH as i32 && py < OBJ_TILE_HEIGHT as i32);
            return Hit::Canvas {
                x: px as u8,
                y: py as u8,
            };
        }
        if self.sheet.contains(point) {
            let tile = view.zoom().tile_size();
            let column = ((x - self.sheet.x) / tile) as usize;
            let row = ((y - self.sheet.y) / tile) as usize;
            return view
                .tile_in_view(column, row, self.visible)
                .map_or(Hit::None, Hit::Sheet);
        }
        if let Some(local) = self.swatches.iter().position(|rect| rect.contains(point)) {
            return Hit::Swatch(local as u8);
        }
        if self.advanced
            && let Some(bank) = self.banks.iter().position(|rect| rect.contains(point))
        {
            return Hit::Bank(bank as u8);
        }
        Hit::None
    }
}

/// The caption naming `tile`, with its grid position in Advanced mode.
pub fn caption(tile: usize, advanced: bool) -> String {
    if advanced {
        let (column, row) = grid_position(tile);
        format!("Tile {tile:04} ({column},{row})")
    } else {
        format!("Tile {tile:04}")
    }
}

fn text_cells(text: &str) -> i32 {
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests;
