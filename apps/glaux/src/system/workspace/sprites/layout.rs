//! Where each part of the sprite editor sits on the 320×240 screen.
//! Rendering and hit-testing both read these rectangles.

use glaux_input::Point;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, OBJ_PALETTE_COLORS};
use glaux_machine::sprite::{OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH};

use super::sheet::{SheetView, Visible, Zoom};
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
    /// Top of the caption under the canvas.
    pub caption_y: i32,
    /// The tiles the sheet shows, exactly `visible` whole tiles.
    pub sheet: Rect,
    pub visible: Visible,
    pub tools: [(ToolSlot, Rect); 4],
    pub grid_button: Rect,
    pub advanced_button: Rect,
    pub zoom_out: Rect,
    pub zoom_label: Rect,
    pub zoom_in: Rect,
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

        Self {
            metrics,
            options,
            status,
            palette,
            canvas_frame,
            canvas,
            caption_y: canvas_frame.bottom() + 2,
            sheet,
            visible,
            tools,
            grid_button,
            advanced_button,
            zoom_out,
            zoom_label,
            zoom_in,
            swatch_y,
            swatches: std::array::from_fn(|i| cell(i, swatch_y)),
            banks: std::array::from_fn(|i| cell(i, bank_y)),
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

fn text_cells(text: &str) -> i32 {
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const METRICS: Metrics = Metrics {
        cell_w: 6,
        cell_h: 10,
        top: 16,
    };

    fn layouts() -> Vec<SpriteLayout> {
        [false, true]
            .into_iter()
            .flat_map(|advanced| {
                [Zoom::X1, Zoom::X2].map(|zoom| SpriteLayout::new(METRICS, advanced, zoom))
            })
            .collect()
    }

    fn centre(rect: Rect) -> Point {
        Point::new(
            (rect.x + rect.width / 2) as u16,
            (rect.y + rect.height / 2) as u16,
        )
    }

    /// Every rectangle the editor hit-tests or fills, by name.
    fn regions(layout: &SpriteLayout) -> Vec<(String, Rect)> {
        let mut out = vec![
            ("options".into(), layout.options),
            ("status".into(), layout.status),
            ("palette".into(), layout.palette),
            ("canvas".into(), layout.canvas_frame),
            ("sheet".into(), layout.sheet.inset(-1)),
        ];
        out.extend(layout.tools.iter().map(|(t, r)| (format!("{t:?}"), *r)));
        out
    }

    #[test]
    fn panels_fit_the_screen_without_overlapping() {
        for layout in layouts() {
            let regions = regions(&layout);
            for (name, rect) in &regions {
                assert!(
                    rect.x >= 0
                        && rect.y >= METRICS.top
                        && rect.right() <= WIDTH as i32
                        && rect.bottom() <= HEIGHT as i32,
                    "{name} {rect:?} advanced={}",
                    layout.advanced
                );
            }
            for (i, (a_name, a)) in regions.iter().enumerate() {
                for (b_name, b) in &regions[i + 1..] {
                    assert!(!a.overlaps(*b), "{a_name} {a:?} overlaps {b_name} {b:?}");
                }
            }
        }
    }

    #[test]
    fn the_canvas_shows_the_tile_at_twelve_x() {
        for layout in layouts() {
            assert_eq!((layout.canvas.width, layout.canvas.height), (96, 96));
            assert!(layout.caption_y + METRICS.cell_h <= layout.palette.y);
        }
    }

    #[test]
    fn one_x_shows_all_sixteen_columns_and_two_x_pans() {
        let normal = SpriteLayout::new(METRICS, false, Zoom::X1);
        assert_eq!(normal.visible, Visible::new(16, 22));
        let zoomed = SpriteLayout::new(METRICS, false, Zoom::X2);
        assert_eq!(zoomed.visible, Visible::new(11, 11));
        let advanced = SpriteLayout::new(METRICS, true, Zoom::X1);
        assert_eq!(advanced.visible, Visible::new(16, 18));
    }

    #[test]
    fn canvas_pixels_continue_past_the_canvas_edges() {
        let layout = SpriteLayout::new(METRICS, false, Zoom::X1);
        let at = |dx: i32, dy: i32| {
            layout.canvas_pixel(Point::new(
                (layout.canvas.x + dx) as u16,
                (layout.canvas.y + dy) as u16,
            ))
        };
        assert_eq!(at(0, 0), (0, 0));
        assert_eq!(at(95, 95), (7, 7));
        assert_eq!(at(96, 0), (8, 0));
        assert_eq!(at(-1, -1), (-1, -1));
        assert_eq!(at(30, -12), (2, -1));
        assert_eq!(at(30, -13), (2, -2));
    }

    #[test]
    fn options_buttons_fit_beside_the_tool_tag() {
        for layout in layouts() {
            let buttons = [
                layout.grid_button,
                layout.advanced_button,
                layout.zoom_out,
                layout.zoom_label,
                layout.zoom_in,
            ];
            for pair in buttons.windows(2) {
                assert!(pair[0].right() <= pair[1].x, "{pair:?}");
            }
            // "Tile 1023", the longest tool name and "MODIFIED", with their
            // padding.
            let cells = 9 + 1 + 10 + 1 + 8;
            assert!(layout.grid_button.x >= MARGIN + cells * METRICS.cell_w + 4);
        }
    }

    #[test]
    fn every_region_hits_what_it_shows() {
        let view = SheetView::default();
        for layout in layouts() {
            assert_eq!(
                layout.hit(centre(layout.grid_button), &view),
                Hit::GridButton
            );
            assert_eq!(
                layout.hit(centre(layout.advanced_button), &view),
                Hit::AdvancedButton
            );
            assert_eq!(layout.hit(centre(layout.zoom_out), &view), Hit::ZoomOut);
            assert_eq!(layout.hit(centre(layout.zoom_in), &view), Hit::ZoomIn);
            assert_eq!(layout.hit(centre(layout.zoom_label), &view), Hit::None);
            for (tool, rect) in layout.tools {
                assert_eq!(layout.hit(centre(rect), &view), Hit::Tool(tool));
            }
            for (local, rect) in layout.swatches.iter().enumerate() {
                assert_eq!(layout.hit(centre(*rect), &view), Hit::Swatch(local as u8));
            }
            for (bank, rect) in layout.banks.iter().enumerate() {
                let expected = if layout.advanced {
                    Hit::Bank(bank as u8)
                } else {
                    Hit::Swatch(bank as u8)
                };
                assert_eq!(layout.hit(centre(*rect), &view), expected);
            }
            assert_eq!(layout.hit(Point::new(0, 239), &view), Hit::None);
        }
    }

    #[test]
    fn canvas_points_map_to_tile_pixels() {
        let layout = SpriteLayout::new(METRICS, false, Zoom::X1);
        let view = SheetView::default();
        let at = |dx: i32, dy: i32| {
            let point = Point::new((layout.canvas.x + dx) as u16, (layout.canvas.y + dy) as u16);
            layout.hit(point, &view)
        };
        assert_eq!(at(0, 0), Hit::Canvas { x: 0, y: 0 });
        assert_eq!(at(11, 11), Hit::Canvas { x: 0, y: 0 });
        assert_eq!(at(12, 0), Hit::Canvas { x: 1, y: 0 });
        assert_eq!(at(3 * 12 + 5, 5 * 12), Hit::Canvas { x: 3, y: 5 });
        assert_eq!(at(95, 95), Hit::Canvas { x: 7, y: 7 });
        assert_eq!(at(96, 0), Hit::None, "the frame");
    }

    #[test]
    fn sheet_points_map_to_scrolled_tiles() {
        for zoom in [Zoom::X1, Zoom::X2] {
            let layout = SpriteLayout::new(METRICS, false, zoom);
            let mut view = SheetView::default();
            view.set_zoom(zoom, layout.visible, 0);
            let cell = |column, row| centre(layout.sheet_cell(column, row, zoom));
            assert_eq!(layout.hit(cell(0, 0), &view), Hit::Sheet(0));
            assert_eq!(layout.hit(cell(3, 2), &view), Hit::Sheet(35));

            view.scroll_rows(5, layout.visible);
            view.scroll_columns(2, layout.visible);
            let left = if zoom == Zoom::X2 { 2 } else { 0 };
            assert_eq!(
                layout.hit(cell(3, 2), &view),
                Hit::Sheet(7 * 16 + 3 + left),
                "{zoom:?}"
            );
            let last = cell(layout.visible.columns - 1, layout.visible.rows - 1);
            assert!(matches!(layout.hit(last, &view), Hit::Sheet(_)));
        }
    }

    #[test]
    fn advanced_mode_keeps_the_main_layout() {
        let normal = SpriteLayout::new(METRICS, false, Zoom::X1);
        let advanced = SpriteLayout::new(METRICS, true, Zoom::X1);
        assert_eq!(advanced.canvas, normal.canvas);
        assert_eq!(advanced.sheet.x, normal.sheet.x);
        assert_eq!(advanced.tools, normal.tools);
        assert_eq!(advanced.options, normal.options);
        assert!(advanced.palette.height > normal.palette.height);
    }
}
