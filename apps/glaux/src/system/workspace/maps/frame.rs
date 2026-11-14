//! Where the map editor's parts sit below the header.

use glaux_input::Point;
use glaux_machine::framebuffer::WIDTH;
use glaux_machine::tilemap::BgLayer;

use super::Metrics;
use super::layout::Layout;
use super::tools::Tool;
use crate::system::workspace::Rect;

pub(super) const TOOL_PLATE: i32 = 18;
pub(super) const SCROLLBAR: i32 = 2;
pub(super) const STRIP_COLUMNS: usize = 16;
const COMPACT_ROWS: i32 = 5;
const EXPANDED_ROWS: i32 = 10;
const RULER_CELLS: i32 = 3;
const EYE_WIDTH: i32 = 16;
const LOCK_WIDTH: i32 = 16;
const GAP: i32 = 2;
const BUTTON_PAD: i32 = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum DrawerSize {
    Collapsed,
    #[default]
    Compact,
    Expanded,
}

impl DrawerSize {
    pub(super) fn cycled(self, reverse: bool) -> Self {
        match (self, reverse) {
            (Self::Collapsed, false) | (Self::Expanded, true) => Self::Compact,
            (Self::Compact, false) | (Self::Collapsed, true) => Self::Expanded,
            (Self::Expanded, false) | (Self::Compact, true) => Self::Collapsed,
        }
    }

    pub(super) fn strip_rows(self) -> usize {
        match self {
            Self::Collapsed => 0,
            Self::Compact => COMPACT_ROWS as usize,
            Self::Expanded => EXPANDED_ROWS as usize,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LayerRow {
    pub(super) row: Rect,
    pub(super) eye: Rect,
    pub(super) lock: Rect,
    pub(super) name: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hit {
    Canvas,
    Tool(Tool),
    Grid,
    Rulers,
    ZoomOut,
    ZoomIn,
    DrawerSize,
    Brush,
    Chip(BgLayer),
    Tile(usize),
    Eye(BgLayer),
    Lock(BgLayer),
    LayerName(BgLayer),
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Frame {
    pub(super) options: Rect,
    pub(super) grid_button: Rect,
    pub(super) rulers_button: Rect,
    pub(super) zoom_out: Rect,
    pub(super) zoom_label: Rect,
    pub(super) zoom_in: Rect,
    pub(super) canvas_frame: Rect,
    pub(super) canvas: Rect,
    /// Zero-sized while the rulers are off.
    pub(super) ruler_top: Rect,
    pub(super) ruler_left: Rect,
    pub(super) scroll_v: Rect,
    pub(super) scroll_h: Rect,
    pub(super) tools: [(Tool, Rect); 6],
    pub(super) drawer: Rect,
    pub(super) drawer_bar: Rect,
    pub(super) size_button: Rect,
    pub(super) brush: Rect,
    pub(super) brush_text: (i32, i32),
    pub(super) chips: [Rect; 3],
    /// The tiles the strip shows, `None` while the drawer is collapsed.
    pub(super) strip: Option<Rect>,
    pub(super) strip_rows: usize,
    pub(super) layer_rows: Option<[LayerRow; 3]>,
    pub(super) size: DrawerSize,
}

impl Frame {
    pub(super) fn new(
        metrics: Metrics,
        header: &Layout,
        size: DrawerSize,
        rulers: bool,
        zoom_label_cells: i32,
    ) -> Self {
        let Metrics { cell_w, cell_h, .. } = metrics;
        let body = header.body();
        let bar = cell_h + 2;
        let options = Rect {
            x: body.x,
            y: body.y,
            width: body.width,
            height: bar,
        };

        let mut right = options.right();
        let mut button = |cells: i32, pad: i32, gap: i32| {
            let width = cells * cell_w + 2 * pad;
            right -= width;
            let rect = Rect {
                x: right,
                y: options.y,
                width,
                height: bar - 1,
            };
            right -= gap;
            rect
        };
        let zoom_in = button(1, BUTTON_PAD, 0);
        let zoom_label = button(zoom_label_cells, 0, 0);
        let zoom_out = button(1, BUTTON_PAD, 2 * GAP);
        let rulers_button = button(2, BUTTON_PAD, GAP);
        let grid_button = button(4, BUTTON_PAD, GAP);

        let tool_x = WIDTH as i32 - GAP - TOOL_PLATE;
        let main_right = tool_x - 2 * GAP;
        let area_top = options.bottom() + GAP;

        let rows = size.strip_rows() as i32;
        let content_h = if rows > 0 { rows * 8 + 2 } else { 0 };
        let drawer_h = bar + if rows > 0 { GAP + content_h } else { 0 };
        let drawer = Rect {
            x: body.x,
            y: body.bottom() - drawer_h,
            width: main_right - body.x,
            height: drawer_h,
        };
        let drawer_bar = Rect {
            height: bar,
            ..drawer
        };
        let area_bottom = drawer.y - GAP;

        let ruler_w = if rulers { RULER_CELLS * cell_w } else { 0 };
        let ruler_h = if rulers { cell_h } else { 0 };
        let canvas_frame = Rect {
            x: body.x + ruler_w,
            y: area_top + ruler_h,
            width: main_right - SCROLLBAR - 1 - (body.x + ruler_w),
            height: area_bottom - SCROLLBAR - 1 - (area_top + ruler_h),
        };
        let canvas = canvas_frame.inset(1);
        let ruler_top = Rect {
            x: canvas.x,
            y: area_top,
            width: canvas.width,
            height: ruler_h,
        };
        let ruler_left = Rect {
            x: body.x,
            y: canvas.y,
            width: ruler_w,
            height: canvas.height,
        };
        let scroll_v = Rect {
            x: canvas_frame.right() + 1,
            y: canvas.y,
            width: SCROLLBAR,
            height: canvas.height,
        };
        let scroll_h = Rect {
            x: canvas.x,
            y: canvas_frame.bottom() + 1,
            width: canvas.width,
            height: SCROLLBAR,
        };

        let mut tool_y = area_top;
        let tools = Tool::ALL.map(|tool| {
            let rect = Rect {
                x: tool_x,
                y: tool_y,
                width: TOOL_PLATE,
                height: TOOL_PLATE,
            };
            tool_y += TOOL_PLATE + 1;
            (tool, rect)
        });

        let size_button = Rect {
            width: 16,
            ..drawer_bar
        };
        let brush = Rect {
            x: drawer_bar.x + 20,
            y: drawer_bar.y + (bar - 8) / 2,
            width: 8,
            height: 8,
        };
        let brush_text = (
            brush.right() + cell_w,
            drawer_bar.y + (bar - cell_h) / 2 + 1,
        );
        let chip_w = 3 * cell_w + 2 * GAP;
        let chips = std::array::from_fn(|index| {
            let from_right = 2 - index as i32;
            Rect {
                x: drawer_bar.right() - (from_right + 1) * (chip_w + GAP) + GAP,
                y: drawer_bar.y + 1,
                width: chip_w,
                height: bar - 2,
            }
        });

        let content_y = drawer_bar.bottom() + GAP;
        let (strip, layer_rows) = if rows > 0 {
            let strip = Rect {
                x: drawer.x + 1,
                y: content_y + 1,
                width: STRIP_COLUMNS as i32 * 8,
                height: rows * 8,
            };
            let panel_x = strip.right() + 1 + SCROLLBAR + 2 * GAP;
            let row_h = cell_h + 1;
            let layer_rows = std::array::from_fn(|slot| {
                let y = content_y + slot as i32 * row_h;
                let row = Rect {
                    x: panel_x,
                    y,
                    width: drawer.right() - panel_x,
                    height: row_h,
                };
                LayerRow {
                    row,
                    eye: Rect {
                        width: EYE_WIDTH,
                        ..row
                    },
                    lock: Rect {
                        x: row.x + EYE_WIDTH,
                        width: LOCK_WIDTH,
                        ..row
                    },
                    name: Rect {
                        x: row.x + EYE_WIDTH + LOCK_WIDTH,
                        width: row.width - EYE_WIDTH - LOCK_WIDTH,
                        ..row
                    },
                }
            });
            (Some(strip), Some(layer_rows))
        } else {
            (None, None)
        };

        Self {
            options,
            grid_button,
            rulers_button,
            zoom_out,
            zoom_label,
            zoom_in,
            canvas_frame,
            canvas,
            ruler_top,
            ruler_left,
            scroll_v,
            scroll_h,
            tools,
            drawer,
            drawer_bar,
            size_button,
            brush,
            brush_text,
            chips,
            strip,
            strip_rows: size.strip_rows(),
            layer_rows,
            size,
        }
    }

    pub(super) fn layer_in_slot(slot: usize) -> BgLayer {
        BgLayer::from_index(2 - slot).unwrap_or(BgLayer::Bg0)
    }

    pub(super) fn strip_frame(&self) -> Option<Rect> {
        self.strip.map(|strip| strip.inset(-1))
    }

    fn tile_at(&self, at: Point, top_row: usize) -> Option<usize> {
        let strip = self.strip?;
        strip.contains(at).then(|| {
            let column = (i32::from(at.x) - strip.x) / 8;
            let row = (i32::from(at.y) - strip.y) / 8;
            (top_row + row as usize) * STRIP_COLUMNS + column as usize
        })
    }

    /// Parts are tried from the top: drawer controls, tools, options bar, canvas.
    pub(super) fn hit(&self, at: Point, top_row: usize) -> Hit {
        if self.size_button.contains(at) {
            return Hit::DrawerSize;
        }
        if self.size == DrawerSize::Collapsed {
            for layer in BgLayer::ALL {
                if self.chips[layer.index()].contains(at) {
                    return Hit::Chip(layer);
                }
            }
        }
        if self.brush.contains(at) {
            return Hit::Brush;
        }
        if let Some(tile) = self.tile_at(at, top_row) {
            return Hit::Tile(tile);
        }
        if let Some(rows) = self.layer_rows {
            for (slot, row) in rows.iter().enumerate() {
                let layer = Self::layer_in_slot(slot);
                if row.eye.contains(at) {
                    return Hit::Eye(layer);
                }
                if row.lock.contains(at) {
                    return Hit::Lock(layer);
                }
                if row.name.contains(at) {
                    return Hit::LayerName(layer);
                }
            }
        }
        if let Some(&(tool, _)) = self.tools.iter().find(|(_, rect)| rect.contains(at)) {
            return Hit::Tool(tool);
        }
        for (rect, hit) in [
            (self.grid_button, Hit::Grid),
            (self.rulers_button, Hit::Rulers),
            (self.zoom_out, Hit::ZoomOut),
            (self.zoom_in, Hit::ZoomIn),
        ] {
            if rect.contains(at) {
                return hit;
            }
        }
        if self.canvas.contains(at) {
            return Hit::Canvas;
        }
        Hit::Nothing
    }
}

#[cfg(test)]
mod tests;
