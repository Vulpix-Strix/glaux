//! The enlarged tile picker.
use glaux_i18n::Arg;
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::tilemap::TileEntry;
use glaux_video::draw::{draw_text, fill_rect};

use super::canvas::{draw_cell, outline_clipped};
use super::compose::Cell;
use super::frame::{BAR as BAR_HEIGHT, DrawerSize, Frame, STRIP_COLUMNS};
use super::pattern::TILES;
use super::{MapEditor, MapOut};
use crate::system::map_document::MapDocument;
use crate::system::screen::{ACCENT, BACKGROUND, DIM, PALE, TEXT};
use crate::system::workspace::sprites::{SpriteSource, checker};
use crate::system::workspace::{BAR, Rect, RenderContext};
use crate::ui_text;

const PAGE_ROWS: usize = 9;
const TILE_PX: i32 = 16;
const SHEET_ROWS: usize = TILES / STRIP_COLUMNS;
const PAGES: usize = SHEET_ROWS.div_ceil(PAGE_ROWS);
const LEFT: i32 = 12;
const GAP_BELOW_OPTIONS: i32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Picker {
    pub(super) page: usize,
    drag: Option<usize>,
    hover: Option<usize>,
    pub(super) restore: DrawerSize,
}

impl Picker {
    pub(super) fn new(restore: DrawerSize, brush_tile: usize) -> Self {
        Self {
            page: brush_tile / STRIP_COLUMNS / PAGE_ROWS,
            drag: None,
            hover: None,
            restore,
        }
    }

    pub(super) fn focus(&self, brush_tile: usize) -> usize {
        self.hover.unwrap_or(brush_tile)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PickerLayout {
    pub(super) window: Rect,
    pub(super) header: Rect,
    pub(super) grid: Rect,
    pub(super) scroll: Rect,
    pub(super) foot: Rect,
}

impl PickerLayout {
    pub(super) fn new(frame: &Frame) -> Self {
        let width = STRIP_COLUMNS as i32 * TILE_PX;
        let height = PAGE_ROWS as i32 * TILE_PX;
        let window = Rect {
            x: LEFT,
            y: frame.options.bottom() + GAP_BELOW_OPTIONS,
            width: width + 6,
            height: 2 + BAR_HEIGHT + 1 + height + BAR_HEIGHT,
        };
        let inner = window.inset(1);
        let header = Rect {
            height: BAR_HEIGHT,
            ..inner
        };
        let grid = Rect {
            x: inner.x,
            y: header.bottom() + 1,
            width,
            height,
        };
        let scroll = Rect {
            x: grid.right() + 1,
            width: super::frame::SCROLLBAR,
            ..grid
        };
        let foot = Rect {
            y: grid.bottom(),
            height: BAR_HEIGHT,
            ..inner
        };
        Self {
            window,
            header,
            grid,
            scroll,
            foot,
        }
    }

    pub(super) fn tile_at(&self, at: Point, page: usize) -> Option<usize> {
        self.grid.contains(at).then(|| {
            let column = (i32::from(at.x) - self.grid.x) / TILE_PX;
            let row = (i32::from(at.y) - self.grid.y) / TILE_PX;
            (page * PAGE_ROWS + row as usize) * STRIP_COLUMNS + column as usize
        })
    }

    pub(super) fn tile_rect(&self, tile: usize, page: usize) -> Option<Rect> {
        let row = (tile / STRIP_COLUMNS).checked_sub(page * PAGE_ROWS)?;
        (row < PAGE_ROWS).then(|| Rect {
            x: self.grid.x + (tile % STRIP_COLUMNS) as i32 * TILE_PX,
            y: self.grid.y + row as i32 * TILE_PX,
            width: TILE_PX,
            height: TILE_PX,
        })
    }
}

pub(super) fn between(a: usize, b: usize) -> (usize, (usize, usize)) {
    let (ca, cb) = (a % STRIP_COLUMNS, b % STRIP_COLUMNS);
    let (ra, rb) = (a / STRIP_COLUMNS, b / STRIP_COLUMNS);
    let (left, top) = (ca.min(cb), ra.min(rb));
    (
        top * STRIP_COLUMNS + left,
        (ca.max(cb) - left + 1, ra.max(rb) - top + 1),
    )
}

impl MapEditor {
    pub(super) fn toggle_picker(&mut self) {
        if self.edit.picker.is_some() {
            self.close_picker();
        } else {
            self.edit.picker = Some(Picker::new(
                self.edit.drawer,
                usize::from(self.edit.brush.tile()),
            ));
            // The window needs the room the open drawer takes.
            self.edit.drawer = DrawerSize::Collapsed;
        }
    }

    pub(super) fn close_picker(&mut self) {
        if let Some(picker) = self.edit.picker.take() {
            self.edit.drawer = picker.restore;
        }
    }

    pub(super) fn picker_input(
        &mut self,
        event: InputEvent,
        frame: &Frame,
        document: &MapDocument,
    ) -> Option<Option<MapOut>> {
        let layout = PickerLayout::new(frame);
        let mut picker = self.edit.picker?;
        let mut out = None;
        match event {
            InputEvent::Break => {
                if picker.drag.take().is_none() {
                    self.close_picker();
                    return Some(None);
                }
            }
            InputEvent::Text('t' | 'T') => {
                self.close_picker();
                return Some(None);
            }
            InputEvent::Wheel { delta } => {
                let page = picker.page as i32 - i32::from(delta);
                picker.page = page.clamp(0, PAGES as i32 - 1) as usize;
            }
            InputEvent::PageUp { .. } => picker.page = picker.page.saturating_sub(1),
            InputEvent::PageDown { .. } => picker.page = (picker.page + 1).min(PAGES - 1),
            InputEvent::PointerMove(at) => {
                self.edit.pointer = at;
                picker.hover = at.and_then(|at| layout.tile_at(at, picker.page));
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => picker.drag = layout.tile_at(at, picker.page),
            InputEvent::PointerDown {
                button: PointerButton::Secondary,
                at,
                ..
            } => {
                if let Some(tile) = layout.tile_at(at, picker.page) {
                    self.open_tile_menu(tile, at, document);
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                at,
            } => {
                let end = layout.tile_at(at, picker.page).or(picker.hover);
                if let (Some(start), Some(end)) = (picker.drag.take(), end) {
                    let (first, size) = between(start, end);
                    out = self.choose_pattern(first, size);
                }
            }
            InputEvent::PointerCancel => picker.drag = None,
            InputEvent::Function(2) => {
                let tile = picker.focus(usize::from(self.edit.brush.tile()));
                out = self.edit_tile_out(tile);
            }
            InputEvent::Undo | InputEvent::Redo | InputEvent::Save => return None,
            _ => {}
        }
        self.edit.picker = Some(picker);
        Some(out)
    }

    pub(super) fn choose_pattern(&mut self, first: usize, size: (usize, usize)) -> Option<MapOut> {
        let brush = self.edit.brush;
        let entry = u16::try_from(first).ok().and_then(|tile| {
            TileEntry::new(
                tile,
                brush.flip_x(),
                brush.flip_y(),
                brush.priority(),
                brush.bank(),
            )
        })?;
        if first == 0 && size == (1, 1) {
            return Some(MapOut::Notice(ui_text::get("maps-tile-0-transparent")));
        }
        self.edit.brush = entry;
        self.edit.pattern = size;
        None
    }

    fn shown_pattern(&self, picker: &Picker) -> (usize, (usize, usize)) {
        match (picker.drag, picker.hover) {
            (Some(start), Some(end)) => between(start, end),
            _ => (usize::from(self.edit.brush.tile()), self.edit.pattern),
        }
    }

    pub(super) fn picker_status(&self, picker: &Picker) -> String {
        let (first, (width, height)) = self.shown_pattern(picker);
        if (width, height) == (1, 1) {
            return ui_text::get_with("maps-status-tile", &[("tile", Arg::Text(&format!("{first:04}")))]);
        }
        let last = first + (height - 1) * STRIP_COLUMNS + width - 1;
        ui_text::get_with(
            "maps-status-pattern",
            &[
                ("width", Arg::Number(width as f64)),
                ("height", Arg::Number(height as f64)),
                ("first", Arg::Text(&format!("{first:04}"))),
                ("last", Arg::Text(&format!("{last:04}"))),
            ],
        )
    }

    pub(super) fn render_picker(
        &self,
        ctx: &mut RenderContext<'_>,
        frame: &Frame,
        source: &SpriteSource<'_>,
        picker: &Picker,
    ) {
        let layout = PickerLayout::new(frame);
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let fill = |framebuffer: &mut Framebuffer, rect: Rect, color| {
            fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, color);
        };
        let window = layout.window;
        fill(framebuffer, Rect { x: window.x + 2, y: window.y + 2, ..window }, BAR);
        fill(framebuffer, window, DIM);
        fill(framebuffer, window.inset(1), BACKGROUND);

        let (first, (width, height)) = self.shown_pattern(picker);
        let cells = |text: &str| i32::try_from(font.text_cells(text)).unwrap_or(0) * self.metrics.cell_w;
        let mut x = layout.header.x + 4;
        for (text, ink, gap) in [
            (ui_text::get("maps-picker-title"), PALE, 8),
            (format!("{first:04}"), TEXT, 6),
            (format!("{width}x{height}"), ACCENT, 0),
        ] {
            draw_text(framebuffer, font, &text, x, layout.header.y, ink);
            x += cells(&text) + gap;
        }
        let page = ui_text::get_with(
            "maps-picker-page",
            &[
                ("page", Arg::Number((picker.page + 1) as f64)),
                ("pages", Arg::Number(PAGES as f64)),
            ],
        );
        draw_text(
            framebuffer,
            font,
            &page,
            layout.header.right() - 4 - cells(&page),
            layout.header.y,
            DIM,
        );
        fill(
            framebuffer,
            Rect {
                y: layout.header.bottom(),
                height: 1,
                ..layout.header
            },
            BAR,
        );

        checker(framebuffer, layout.grid, 4);
        for row in 0..PAGE_ROWS {
            for column in 0..STRIP_COLUMNS {
                let tile = (picker.page * PAGE_ROWS + row) * STRIP_COLUMNS + column;
                let Some(entry) = u16::try_from(tile)
                    .ok()
                    .filter(|_| tile < TILES)
                    .and_then(|tile| TileEntry::new(tile, false, false, false, self.edit.brush.bank()))
                else {
                    continue;
                };
                let at = (
                    layout.grid.x + column as i32 * TILE_PX,
                    layout.grid.y + row as i32 * TILE_PX,
                );
                draw_cell(
                    framebuffer,
                    source.bg,
                    &Cell::single(entry),
                    at,
                    2,
                    source.preview.slots(),
                    layout.grid,
                );
            }
        }
        if let Some(hover) = picker.hover.filter(|_| picker.drag.is_none())
            && let Some(rect) = layout.tile_rect(hover, picker.page)
        {
            outline_clipped(framebuffer, rect, layout.grid, PALE);
        }
        let last = first + (height - 1) * STRIP_COLUMNS + width - 1;
        if let (Some(from), Some(to)) = (
            layout.tile_rect(first, picker.page),
            layout.tile_rect(last, picker.page),
        ) {
            let rect = Rect {
                width: to.right() - from.x,
                height: to.bottom() - from.y,
                ..from
            };
            outline_clipped(framebuffer, rect, layout.window, ACCENT);
        }

        fill(framebuffer, layout.scroll, BAR);
        let thumb = layout.scroll.height / PAGES as i32;
        fill(
            framebuffer,
            Rect {
                y: layout.scroll.y + thumb * picker.page as i32,
                height: thumb.max(2),
                ..layout.scroll
            },
            DIM,
        );

        fill(framebuffer, layout.foot, BAR);
        let mut x = layout.foot.x + 4;
        for id in ["maps-picker-drag", "maps-picker-wheel", "maps-picker-close"] {
            let text = ui_text::get(id);
            let (key, rest) = text.split_once(' ').unwrap_or((&text, ""));
            draw_text(framebuffer, font, key, x, layout.foot.y, PALE);
            let key_end = x + cells(key) + self.metrics.cell_w;
            draw_text(framebuffer, font, rest, key_end, layout.foot.y, DIM);
            x = key_end + cells(rest) + 8;
        }
    }
}

#[cfg(test)]
mod tests;
