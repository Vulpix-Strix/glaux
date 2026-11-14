//! Where the map editor's parts sit, following the approved mockups of the
//! Map editor on the 320 x 240 screen.

use glaux_input::Point;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::tilemap::BgLayer;

use super::Metrics;
use super::tools::Tool;
use crate::system::workspace::Rect;

/// Side of a tool button, which holds one 16x16 icon.
pub(super) const TOOL_PLATE: i32 = 18;
/// Thickness of the scroll indicators.
pub(super) const SCROLLBAR: i32 = 2;
/// Tiles per row of the drawer's tile strip.
pub(super) const STRIP_COLUMNS: usize = 16;
/// Height of the options bar, the drawer's bar and the status bar.
pub(super) const BAR: i32 = 12;
/// Width of the drawer and the canvas column, up to the tools.
const COLUMN_WIDTH: i32 = 298;
/// Left edge of the tools and where the canvas column ends.
const TOOLS_X: i32 = 300;
/// Screen margin of the canvas.
const MARGIN: i32 = 4;
/// Cells reserved for a ruler's numbers, which have up to three digits.
const RULER_CELLS: i32 = 3;
/// Width of an eye icon's cell and of a padlock's, in the layer list.
pub(super) const EYE_WIDTH: i32 = 15;
pub(super) const LOCK_WIDTH: i32 = 11;
/// Where the drawer's separators and the parts between them sit.
const SIZE_BUTTON: i32 = 14;
const TILES_LABEL_X: i32 = 14;
const BRUSH_X: i32 = 46;
const BRUSH_NUMBER_X: i32 = 57;
const FIRST_SEPARATOR_X: i32 = 84;
const BANK_X: i32 = 89;
const SECOND_SEPARATOR_X: i32 = 138;
const LAYERS_X: i32 = 141;
const PICKER_X: i32 = 256;
/// Rows of the strip when the drawer is compact and when it is expanded.
const COMPACT_ROWS: usize = 6;
const EXPANDED_ROWS: usize = 10;
const EDIT_BUTTON_WIDTH: i32 = 100;

/// How much of the screen the drawer takes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum DrawerSize {
    /// Only its bar, which holds the brush and the layer chips.
    Collapsed,
    /// The tile strip and the layers.
    #[default]
    Compact,
    /// The same with a taller strip.
    Expanded,
}

impl DrawerSize {
    /// The size Tab brings next, or the one before it when `reverse`.
    pub(super) fn cycled(self, reverse: bool) -> Self {
        match (self, reverse) {
            (Self::Collapsed, false) | (Self::Expanded, true) => Self::Compact,
            (Self::Compact, false) | (Self::Collapsed, true) => Self::Expanded,
            (Self::Expanded, false) | (Self::Compact, true) => Self::Collapsed,
        }
    }

    /// Rows of tiles the strip shows, none when only the bar is open.
    pub(super) fn strip_rows(self) -> usize {
        match self {
            Self::Collapsed => 0,
            Self::Compact => COMPACT_ROWS,
            Self::Expanded => EXPANDED_ROWS,
        }
    }
}

/// The parts of one layer's row in the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LayerRow {
    pub(super) row: Rect,
    pub(super) eye: Rect,
    pub(super) lock: Rect,
    pub(super) name: Rect,
}

/// What a point falls on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hit {
    Canvas,
    Tool(Tool),
    Undo,
    Redo,
    /// The map's name in the options bar, which opens the list of maps.
    Selector,
    Grid,
    Rulers,
    Attr,
    ZoomOut,
    ZoomIn,
    DrawerSize,
    /// The brush in the drawer's bar.
    Brush,
    Picker,
    EditTile,
    /// A layer's chip in the collapsed drawer's bar.
    Chip(BgLayer),
    /// A tile of the strip, by index.
    Tile(usize),
    Eye(BgLayer),
    Lock(BgLayer),
    LayerName(BgLayer),
    Nothing,
}

/// What the layout depends on besides the font: the drawer, the rulers,
/// the zoom, the tool named in the options bar and the locked layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Shape {
    pub(super) drawer: DrawerSize,
    pub(super) rulers: bool,
    /// Side of a tile on screen.
    pub(super) tile: i32,
    /// Cells of the tool's name.
    pub(super) tool_cells: i32,
    pub(super) locked: [bool; 3],
}

/// The editor's rectangles for one shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Frame {
    pub(super) options: Rect,
    /// The map's name, with the room for its marker of unsaved changes.
    pub(super) selector: Rect,
    pub(super) tool_tag: Rect,
    pub(super) layer_tag: Rect,
    pub(super) grid_button: Rect,
    pub(super) rulers_button: Rect,
    pub(super) attr_button: Rect,
    pub(super) overview_button: Rect,
    pub(super) zoom_out: Rect,
    pub(super) zoom_label: Rect,
    pub(super) zoom_in: Rect,
    pub(super) canvas_frame: Rect,
    pub(super) canvas: Rect,
    /// The rulers' bands, empty while they are off.
    pub(super) ruler_top: Rect,
    pub(super) ruler_left: Rect,
    pub(super) scroll_v: Rect,
    pub(super) scroll_h: Rect,
    pub(super) tools: [(Tool, Rect); 6],
    pub(super) undo: Rect,
    pub(super) redo: Rect,
    pub(super) drawer: Rect,
    pub(super) drawer_bar: Rect,
    pub(super) size_button: Rect,
    pub(super) tiles_label: (i32, i32),
    pub(super) brush: Rect,
    pub(super) brush_number: (i32, i32),
    pub(super) bank_text: (i32, i32),
    pub(super) first_separator: Rect,
    pub(super) second_separator: Rect,
    pub(super) picker_button: Rect,
    /// The word before the layers in the open drawer.
    pub(super) layers_label: (i32, i32),
    /// Layers' chips in the bar, by layer index, used while collapsed.
    pub(super) chips: [Rect; 3],
    /// The tiles the strip shows, `None` while the drawer is collapsed.
    pub(super) strip: Option<Rect>,
    pub(super) strip_rows: usize,
    pub(super) strip_scroll: Option<Rect>,
    /// The rule between the strip and the layers.
    pub(super) divider: Option<Rect>,
    pub(super) layer_rows: Option<[LayerRow; 3]>,
    pub(super) tile_rule: Option<Rect>,
    pub(super) tile_info: Option<(i32, i32)>,
    pub(super) edit_button: Option<Rect>,
    /// Where the note about the eyes and locks goes.
    pub(super) layer_note: Option<(i32, i32)>,
    pub(super) status: Rect,
    pub(super) size: DrawerSize,
}

impl Frame {
    pub(super) fn new(metrics: Metrics, shape: Shape) -> Self {
        let Metrics {
            cell_w,
            cell_h,
            top,
        } = metrics;
        let Shape {
            drawer: size,
            rulers,
            tile,
            tool_cells,
            locked,
        } = shape;
        let width = WIDTH as i32;
        let options = Rect {
            x: 0,
            y: top,
            width,
            height: BAR,
        };

        // The toggles stack from the right edge of the bar.
        let mut right = width - MARGIN;
        let mut item = |cells: i32, pad: i32, gap: i32| {
            let item_w = cells * cell_w + 2 * pad;
            right -= item_w;
            let rect = Rect {
                x: right,
                y: options.y,
                width: item_w,
                height: BAR - 1,
            };
            right -= gap;
            rect
        };
        let zoom_in = item(1, 2, 0);
        let zoom_label = item(2, 1, 0);
        let zoom_out = item(1, 2, 4);
        let overview_button = item(8, 2, 4);
        let attr_button = item(4, 2, 4);
        let rulers_button = item(2, 2, 4);
        let grid_button = item(4, 2, 0);

        // The name gets what the tags leave of the room before Grid.
        let tag_h = BAR - 3;
        let tool_w = tool_cells * cell_w + 4;
        let layer_w = 3 * cell_w + 4;
        let room = grid_button.x - 4 - MARGIN - 6 - tool_w - 4 - layer_w;
        let name_cells = (room / cell_w).max(4);
        let selector = Rect {
            x: MARGIN,
            y: options.y,
            width: name_cells * cell_w,
            height: BAR - 1,
        };
        let tool_tag = Rect {
            x: selector.right() + 6,
            y: options.y + 1,
            width: tool_w,
            height: tag_h,
        };
        let layer_tag = Rect {
            x: tool_tag.right() + 4,
            y: options.y + 1,
            width: layer_w,
            height: tag_h,
        };

        let status = Rect {
            x: 0,
            y: HEIGHT as i32 - BAR,
            width,
            height: BAR,
        };
        let rows = size.strip_rows() as i32;
        let content_h = rows * 8 + 2;
        let drawer_h = BAR + if rows > 0 { 1 + content_h + 1 } else { 0 };
        let drawer = Rect {
            x: 0,
            y: status.y - drawer_h,
            width: COLUMN_WIDTH,
            height: drawer_h,
        };
        let drawer_bar = Rect {
            height: BAR,
            ..drawer
        };

        // The canvas shows whole tiles. Its frame is as high as they are
        // and sits at the top of the room the drawer leaves.
        let area_top = options.bottom() + 2;
        let ruler_w = if rulers { RULER_CELLS * cell_w } else { 0 };
        let ruler_h = if rulers { cell_h } else { 0 };
        let frame_top = area_top + ruler_h;
        let room_w = COLUMN_WIDTH - 2 * MARGIN - SCROLLBAR - 2 - ruler_w - 2;
        let room_h = drawer.y - 4 - frame_top - 2;
        let tile = tile.max(1);
        let canvas_w = (room_w / tile).max(1) * tile;
        let canvas_h = (room_h / tile).max(1) * tile;
        let canvas_frame = Rect {
            x: MARGIN + ruler_w,
            y: frame_top,
            width: canvas_w + 2,
            height: canvas_h + 2,
        };
        let canvas = canvas_frame.inset(1);
        let ruler_top = Rect {
            x: canvas.x,
            y: area_top,
            width: canvas.width,
            height: ruler_h,
        };
        let ruler_left = Rect {
            x: MARGIN,
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

        let mut tool_y = options.bottom() + 2;
        let tools = Tool::ALL.map(|tool| {
            let rect = Rect {
                x: TOOLS_X,
                y: tool_y,
                width: TOOL_PLATE,
                height: TOOL_PLATE,
            };
            tool_y += TOOL_PLATE + 1;
            (tool, rect)
        });
        let undo = Rect {
            x: TOOLS_X,
            y: tool_y + 4,
            width: TOOL_PLATE,
            height: TOOL_PLATE,
        };
        let redo = Rect {
            y: undo.bottom() + 1,
            ..undo
        };

        let size_button = Rect {
            width: SIZE_BUTTON,
            ..drawer_bar
        };
        let brush = Rect {
            x: BRUSH_X,
            y: drawer_bar.y + 2,
            width: 8,
            height: 8,
        };
        let first_separator = Rect {
            x: FIRST_SEPARATOR_X,
            width: 1,
            ..drawer_bar
        };
        let second_separator = Rect {
            x: SECOND_SEPARATOR_X,
            width: 1,
            ..drawer_bar
        };
        let picker_button = Rect {
            x: PICKER_X,
            width: COLUMN_WIDTH - PICKER_X,
            ..drawer_bar
        };

        // Chips run BG2 to BG0 from the layers' column. An eye and a name
        // fill most of a chip, and a lock is shown where the bar has room.
        let chip_room = PICKER_X - 2 - LAYERS_X;
        let base = EYE_WIDTH + 3 * cell_w;
        let mut spare = chip_room - 3 * base - 2;
        let mut chip_x = LAYERS_X;
        let mut chips = [drawer_bar; 3];
        for index in [2, 1, 0] {
            let with_lock = locked[index] && spare > LOCK_WIDTH;
            let chip_w = base + if with_lock { LOCK_WIDTH + 1 } else { 0 };
            if with_lock {
                spare -= LOCK_WIDTH + 1;
            }
            chips[index] = Rect {
                x: chip_x,
                width: chip_w,
                ..drawer_bar
            };
            chip_x += chip_w + 1;
        }

        let content_top = drawer_bar.bottom() + 1;
        let (strip, strip_scroll, divider, layer_rows, layer_note, tile_parts) = if rows > 0 {
            let strip = Rect {
                x: MARGIN + 1,
                y: content_top + 1,
                width: STRIP_COLUMNS as i32 * 8,
                height: rows * 8,
            };
            let scroll = Rect {
                x: strip.right() + 1,
                y: strip.y,
                width: SCROLLBAR,
                height: strip.height,
            };
            let divider = Rect {
                x: SECOND_SEPARATOR_X,
                y: drawer_bar.bottom(),
                width: 1,
                height: drawer.bottom() - drawer_bar.bottom(),
            };
            let row_w = COLUMN_WIDTH - 1 - LAYERS_X;
            let layer_rows: [LayerRow; 3] = std::array::from_fn(|slot| {
                let row = Rect {
                    x: LAYERS_X,
                    y: strip.y + slot as i32 * (BAR + 1),
                    width: row_w,
                    height: BAR,
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
                        width: row_w - EYE_WIDTH - LOCK_WIDTH,
                        ..row
                    },
                }
            });
            let expanded = size == DrawerSize::Expanded;
            let below = layer_rows[2].row.bottom();
            let tile_parts = expanded.then(|| {
                let rule = Rect {
                    x: LAYERS_X,
                    y: below + 2,
                    width: row_w,
                    height: 1,
                };
                let info = (LAYERS_X, rule.bottom() + 2);
                let button = Rect {
                    x: LAYERS_X,
                    y: info.1 + BAR,
                    width: EDIT_BUTTON_WIDTH,
                    height: BAR,
                };
                (rule, info, button)
            });
            let note_y = tile_parts.map_or(below + 1, |(.., button)| button.bottom() + 2);
            (
                Some(strip),
                Some(scroll),
                Some(divider),
                Some(layer_rows),
                Some((LAYERS_X, note_y)),
                tile_parts,
            )
        } else {
            (None, None, None, None, None, None)
        };

        Self {
            options,
            selector,
            tool_tag,
            layer_tag,
            grid_button,
            rulers_button,
            attr_button,
            overview_button,
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
            undo,
            redo,
            drawer,
            drawer_bar,
            size_button,
            tiles_label: (TILES_LABEL_X, drawer_bar.y),
            brush,
            brush_number: (BRUSH_NUMBER_X, drawer_bar.y),
            bank_text: (BANK_X, drawer_bar.y),
            first_separator,
            second_separator,
            picker_button,
            layers_label: (LAYERS_X + 2, drawer_bar.y),
            chips,
            strip,
            strip_rows: size.strip_rows(),
            strip_scroll,
            divider,
            layer_rows,
            tile_rule: tile_parts.map(|parts| parts.0),
            tile_info: tile_parts.map(|parts| parts.1),
            edit_button: tile_parts.map(|parts| parts.2),
            layer_note,
            status,
            size,
        }
    }

    /// The layer shown in slot `slot` of the list, top to bottom.
    pub(super) fn layer_in_slot(slot: usize) -> BgLayer {
        BgLayer::from_index(2 - slot).unwrap_or(BgLayer::Bg0)
    }

    /// The strip's frame, one pixel around the tiles.
    pub(super) fn strip_frame(&self) -> Option<Rect> {
        self.strip.map(|strip| strip.inset(-1))
    }

    /// The tile of the strip under `at` when the strip's first row is
    /// `top_row`.
    fn tile_at(&self, at: Point, top_row: usize) -> Option<usize> {
        let strip = self.strip?;
        strip.contains(at).then(|| {
            let column = (i32::from(at.x) - strip.x) / 8;
            let row = (i32::from(at.y) - strip.y) / 8;
            (top_row + row as usize) * STRIP_COLUMNS + column as usize
        })
    }

    /// What `at` falls on, with the strip scrolled to `top_row`. Parts are
    /// tried from the top: drawer controls, tools, options bar, canvas.
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
        if self.picker_button.contains(at) {
            return Hit::Picker;
        }
        if self.edit_button.is_some_and(|button| button.contains(at)) {
            return Hit::EditTile;
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
            (self.undo, Hit::Undo),
            (self.redo, Hit::Redo),
            (self.selector, Hit::Selector),
            (self.grid_button, Hit::Grid),
            (self.rulers_button, Hit::Rulers),
            (self.attr_button, Hit::Attr),
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
