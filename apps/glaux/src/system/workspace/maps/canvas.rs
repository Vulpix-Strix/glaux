//! Drawing the options bar, the canvas and the tools.

use glaux_input::Point;
use glaux_machine::bg::BgVram;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::palette::PALETTE_LEN;
use glaux_machine::system_rom::SystemIcon;
use glaux_machine::tilemap::BgLayer;
use glaux_video::draw::{draw_ink_image, draw_text, fill_rect};

use super::{MapEditor, Overlay};
use super::compose::{Cell, Composer};
use super::editing::map_size;
use super::frame::{Frame, Hit};
use super::tools::Tool;
use super::view::{TILE, Viewport};
use crate::rom::ICON_SIZE;
use crate::system::map_document::MapDocument;
use crate::system::screen::{ACCENT, BACKGROUND, DIM, ERROR, PALE, TEXT};
use crate::system::workspace::dialog::text_cells;
use crate::system::workspace::sprites::{SpriteSource, checker, outline};
use crate::system::workspace::{
    BAR, HOVER_SHADE, IDLE_SHADE, OUTLINE, Rect, RenderContext, take_cells, wrap,
};
use crate::ui_text;

const GRID_DOT: u8 = IDLE_SHADE;
const FRAME: u8 = DIM;
const ACTIVE_TOOL_INKS: [u8; 3] = [OUTLINE, ACCENT, TEXT];
const HOVER_TOOL_INKS: [u8; 3] = [OUTLINE, HOVER_SHADE, PALE];
const IDLE_TOOL_INKS: [u8; 3] = [OUTLINE, IDLE_SHADE, DIM];
const RULER_STEPS: [i32; 5] = [1, 2, 4, 8, 16];

pub(super) fn clipped(rect: Rect, clip: Rect) -> Option<Rect> {
    let left = rect.x.max(clip.x);
    let top = rect.y.max(clip.y);
    let right = rect.right().min(clip.right());
    let bottom = rect.bottom().min(clip.bottom());
    (left < right && top < bottom).then_some(Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

pub(super) fn fill_clipped(framebuffer: &mut Framebuffer, rect: Rect, clip: Rect, color: u8) {
    if let Some(rect) = clipped(rect, clip) {
        fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, color);
    }
}

pub(super) fn outline_clipped(framebuffer: &mut Framebuffer, rect: Rect, clip: Rect, color: u8) {
    let Rect {
        x,
        y,
        width,
        height,
    } = rect;
    for edge in [
        Rect { height: 1, ..rect },
        Rect {
            y: y + height - 1,
            height: 1,
            ..rect
        },
        Rect { width: 1, ..rect },
        Rect {
            x: x + width - 1,
            width: 1,
            ..rect
        },
    ] {
        fill_clipped(framebuffer, edge, clip, color);
    }
}

/// Master color `m` is drawn as `slots[m]`.
pub(super) fn draw_cell(
    framebuffer: &mut Framebuffer,
    bg: &BgVram,
    cell: &Cell,
    at: (i32, i32),
    scale: i32,
    slots: &[u8; PALETTE_LEN],
    clip: Rect,
) {
    let side = TILE as usize;
    for y in 0..side {
        for x in 0..side {
            if let Some(master) = cell.pixel(bg, x, y) {
                let block = Rect {
                    x: at.0 + x as i32 * scale,
                    y: at.1 + y as i32 * scale,
                    width: scale,
                    height: scale,
                };
                fill_clipped(framebuffer, block, clip, slots[usize::from(master)]);
            }
        }
    }
}

fn ruler_step(tile: i32, needed: i32) -> i32 {
    RULER_STEPS
        .into_iter()
        .find(|step| step * tile >= needed)
        .unwrap_or(RULER_STEPS[RULER_STEPS.len() - 1])
}

impl MapEditor {
    pub(super) fn render_editor(
        &self,
        ctx: &mut RenderContext<'_>,
        source: Result<SpriteSource<'_>, &str>,
        document: &MapDocument,
        pointer: Option<Point>,
        hover: Hit,
    ) {
        let frame = self.frame();
        self.toolbar(ctx, &frame, document, hover);
        match source {
            Ok(source) => {
                self.canvas(ctx.framebuffer, &frame, document, &source, pointer, hover);
                self.rulers(ctx, &frame, document, pointer);
                self.drawer(ctx, &frame, document, &source, hover);
            }
            Err(message) => self.unavailable(ctx, &frame, message),
        }
    }

    pub(super) fn text_y(&self, y: i32, height: i32) -> i32 {
        y + (height - self.metrics.cell_h) / 2
    }

    pub(super) fn cells(&self, text: &str) -> i32 {
        text_cells(text) * self.metrics.cell_w
    }

    /// The bar above the canvas: the map selector, what is being drawn with
    /// and on, and the toggles. Without a map to edit it holds the selector only.
    pub(super) fn options_bar(
        &self,
        ctx: &mut RenderContext<'_>,
        frame: &Frame,
        document: Option<&MapDocument>,
        editing: bool,
        hover: Hit,
    ) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let bar = frame.options;
        fill_rect(framebuffer, bar.x, bar.bottom() - 1, bar.width, 1, BAR);
        let y = bar.y;

        let selector = frame.selector;
        let lit = hover == Hit::Selector || matches!(self.overlay, Some(Overlay::Maps(_)));
        if lit {
            fill_rect(
                framebuffer,
                selector.x - 2,
                selector.y,
                selector.width + 4,
                selector.height,
                BAR,
            );
        }
        let columns = usize::try_from(selector.width / self.metrics.cell_w).unwrap_or(0);
        match document {
            Some(document) => {
                let modified = document.is_modified();
                let name = take_cells(
                    font,
                    &document.meta().name,
                    columns.saturating_sub(usize::from(modified)),
                );
                draw_text(framebuffer, font, name, selector.x, y, TEXT);
                if modified {
                    let x = selector.x + self.cells(name);
                    draw_text(framebuffer, font, "*", x, y, ACCENT);
                }
            }
            None => {
                let label = ui_text::get("maps-select");
                draw_text(
                    framebuffer,
                    font,
                    take_cells(font, &label, columns),
                    selector.x,
                    y,
                    DIM,
                );
            }
        }
        if !editing {
            return;
        }

        let tool = ui_text::get(self.edit.tool.message_id());
        for (rect, text, ink) in [
            (frame.tool_tag, tool, PALE),
            (frame.layer_tag, self.edit.layer.to_string(), ACCENT),
        ] {
            fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, BAR);
            draw_text(framebuffer, font, &text, rect.x + 2, y, ink);
        }

        let zoom = self.edit.view.zoom();
        let buttons: [(Rect, String, Hit, bool, bool); 4] = [
            (
                frame.grid_button,
                ui_text::get("maps-grid"),
                Hit::Grid,
                self.edit.grid,
                true,
            ),
            (
                frame.rulers_button,
                ui_text::get("maps-rulers"),
                Hit::Rulers,
                self.edit.rulers,
                true,
            ),
            (
                frame.zoom_out,
                "-".to_owned(),
                Hit::ZoomOut,
                false,
                zoom.zoomed_out().is_some(),
            ),
            (
                frame.zoom_in,
                "+".to_owned(),
                Hit::ZoomIn,
                false,
                zoom.zoomed_in().is_some(),
            ),
        ];
        for (rect, label, which, on, enabled) in buttons {
            let hovered = hover == which && enabled;
            if hovered || on {
                fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, BAR);
            }
            let ink = match (enabled, on, hovered) {
                (false, ..) => IDLE_SHADE,
                (true, true, _) => ACCENT,
                (true, false, true) => PALE,
                (true, false, false) => DIM,
            };
            let x = rect.x + (rect.width - self.cells(&label)) / 2;
            draw_text(framebuffer, font, &label, x, y, ink);
        }
        let label_x = frame.zoom_label.x + (frame.zoom_label.width - self.cells(zoom.label())) / 2;
        draw_text(framebuffer, font, zoom.label(), label_x, y, TEXT);
    }

    fn toolbar(
        &self,
        ctx: &mut RenderContext<'_>,
        frame: &Frame,
        document: &MapDocument,
        hover: Hit,
    ) {
        let (framebuffer, ui) = (&mut *ctx.framebuffer, ctx.ui);
        for (tool, rect) in frame.tools {
            let inks = if tool == self.edit.tool {
                fill_rect(
                    framebuffer,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    BACKGROUND,
                );
                outline(framebuffer, rect, ACCENT);
                ACTIVE_TOOL_INKS
            } else if hover == Hit::Tool(tool) {
                fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, BAR);
                HOVER_TOOL_INKS
            } else {
                fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, BAR);
                IDLE_TOOL_INKS
            };
            let pad = (rect.width - ICON_SIZE as i32) / 2;
            let at = (rect.x + pad, rect.y + pad);
            let icon = match tool {
                Tool::Pencil => SystemIcon::Sprites,
                Tool::Eraser => SystemIcon::Eraser,
                Tool::Fill => SystemIcon::Fill,
                Tool::Select => SystemIcon::Select,
                Tool::Eyedropper => SystemIcon::Eyedropper,
                Tool::Hand => SystemIcon::Hand,
            };
            draw_ink_image(framebuffer, ui.icon(icon), at.0, at.1, &inks);
        }
        for (rect, icon, which, enabled) in [
            (frame.undo, SystemIcon::Undo, Hit::Undo, document.can_undo()),
            (frame.redo, SystemIcon::Redo, Hit::Redo, document.can_redo()),
        ] {
            fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, BAR);
            let inks = match (enabled, hover == which) {
                (false, _) => [OUTLINE, IDLE_SHADE, IDLE_SHADE],
                (true, true) => HOVER_TOOL_INKS,
                (true, false) => IDLE_TOOL_INKS,
            };
            let pad = (rect.width - ICON_SIZE as i32) / 2;
            draw_ink_image(
                framebuffer,
                ui.icon(icon),
                rect.x + pad,
                rect.y + pad,
                &inks,
            );
        }
    }

    fn canvas(
        &self,
        framebuffer: &mut Framebuffer,
        frame: &Frame,
        document: &MapDocument,
        source: &SpriteSource<'_>,
        pointer: Option<Point>,
        hover: Hit,
    ) {
        let canvas = frame.canvas;
        outline(framebuffer, frame.canvas_frame, FRAME);
        fill_rect(
            framebuffer,
            canvas.x,
            canvas.y,
            canvas.width,
            canvas.height,
            BACKGROUND,
        );
        let view = self.viewport(frame, document);
        let size = map_size(document);
        let (offset_x, offset_y) = view.offset();
        let tile = view.zoom().tile_px();
        let scale = view.zoom().scale();
        let map_rect = Rect {
            x: canvas.x - offset_x,
            y: canvas.y - offset_y,
            width: size.0 as i32 * tile,
            height: size.1 as i32 * tile,
        };
        if let Some(shown) = clipped(map_rect, canvas) {
            checker(framebuffer, shown, tile / 2);
        }

        let composer = Composer::new(
            BgLayer::ALL.map(|layer| document.layer(layer)),
            self.edit.visible,
        );
        let area = (canvas.width, canvas.height);
        let (first, count) = view.visible_cells(size, area);
        for cy in first.1..first.1 + count.1 {
            for cx in first.0..first.0 + count.0 {
                let cell = composer.cell(cx, cy);
                if cell.is_empty() {
                    continue;
                }
                let at = view.cell_rect(canvas, (cx as i32, cy as i32));
                draw_cell(
                    framebuffer,
                    source.bg,
                    &cell,
                    (at.x, at.y),
                    scale,
                    source.preview.slots(),
                    canvas,
                );
            }
        }

        if self.edit.grid {
            self.grid(framebuffer, view, canvas, size, first, count);
        }
        if let Some(selection) = self.edit.selection {
            let top_left = view.cell_rect(canvas, (selection.left as i32, selection.top as i32));
            let bottom_right =
                view.cell_rect(canvas, (selection.right as i32, selection.bottom as i32));
            let rect = Rect {
                x: top_left.x,
                y: top_left.y,
                width: bottom_right.right() - top_left.x,
                height: bottom_right.bottom() - top_left.y,
            };
            outline_clipped(framebuffer, rect, canvas, ACCENT);
        }
        if hover == Hit::Canvas
            && self.edit.tool != Tool::Hand
            && let Some(at) = pointer
        {
            let cell = view.cell_at(canvas, (i32::from(at.x), i32::from(at.y)));
            if in_map(cell, size) {
                let rect = view.cell_rect(canvas, cell);
                if self.edit.tool == Tool::Pencil && !self.edit.locked[self.edit.layer.index()] {
                    draw_cell(
                        framebuffer,
                        source.bg,
                        &Cell::single(self.edit.brush),
                        (rect.x, rect.y),
                        scale,
                        source.preview.slots(),
                        canvas,
                    );
                }
                outline_clipped(framebuffer, rect.inset(-1), canvas, ACCENT);
            }
        }
        self.scroll_indicators(framebuffer, frame, view, size);
    }

    fn grid(
        &self,
        framebuffer: &mut Framebuffer,
        view: Viewport,
        canvas: Rect,
        size: (usize, usize),
        first: (usize, usize),
        count: (usize, usize),
    ) {
        let tile = view.zoom().tile_px();
        let (offset_x, offset_y) = view.offset();
        let map_rect = Rect {
            x: canvas.x - offset_x,
            y: canvas.y - offset_y,
            width: size.0 as i32 * tile,
            height: size.1 as i32 * tile,
        };
        let last_x = (map_rect.right()).min(canvas.right());
        let last_y = (map_rect.bottom()).min(canvas.bottom());
        let (top, left) = (map_rect.y.max(canvas.y), map_rect.x.max(canvas.x));
        for column in first.0..=first.0 + count.0 {
            let x = map_rect.x + column as i32 * tile;
            if (canvas.x..canvas.right()).contains(&x) {
                for y in (top..last_y).step_by(2) {
                    fill_rect(framebuffer, x, y, 1, 1, GRID_DOT);
                }
            }
        }
        for row in first.1..=first.1 + count.1 {
            let y = map_rect.y + row as i32 * tile;
            if (canvas.y..canvas.bottom()).contains(&y) {
                for x in (left..last_x).step_by(2) {
                    fill_rect(framebuffer, x, y, 1, 1, GRID_DOT);
                }
            }
        }
    }

    fn scroll_indicators(
        &self,
        framebuffer: &mut Framebuffer,
        frame: &Frame,
        view: Viewport,
        size: (usize, usize),
    ) {
        let (content_w, content_h) = view.content(size);
        let (offset_x, offset_y) = view.offset();
        // Ends at the bar's end exactly when the view reaches the map's end.
        let track = |start: i32, length: i32, offset: i32, shown: i32, total: i32| {
            let from = start + length * offset / total;
            let to = start + length * (offset + shown).min(total) / total;
            let thumb = (to - from).clamp(2, length);
            (from.min(start + length - thumb), thumb)
        };
        if content_h > frame.canvas.height {
            let bar = frame.scroll_v;
            fill_rect(framebuffer, bar.x, bar.y, bar.width, bar.height, BAR);
            let (y, height) = track(bar.y, bar.height, offset_y, frame.canvas.height, content_h);
            fill_rect(framebuffer, bar.x, y, bar.width, height, DIM);
        }
        if content_w > frame.canvas.width {
            let bar = frame.scroll_h;
            fill_rect(framebuffer, bar.x, bar.y, bar.width, bar.height, BAR);
            let (x, width) = track(bar.x, bar.width, offset_x, frame.canvas.width, content_w);
            fill_rect(framebuffer, x, bar.y, width, bar.height, DIM);
        }
    }

    fn rulers(
        &self,
        ctx: &mut RenderContext<'_>,
        frame: &Frame,
        document: &MapDocument,
        pointer: Option<Point>,
    ) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        if !self.edit.rulers {
            return;
        }
        let view = self.viewport(frame, document);
        let size = map_size(document);
        let tile = view.zoom().tile_px();
        let cell_h = self.metrics.cell_h;
        let under = pointer
            .filter(|at| frame.canvas.contains(*at))
            .map(|at| view.cell_at(frame.canvas, (i32::from(at.x), i32::from(at.y))));
        let area = (frame.canvas.width, frame.canvas.height);
        let (first, count) = view.visible_cells(size, area);

        let across = ruler_step(tile, 4 * self.metrics.cell_w);
        for column in (first.0..first.0 + count.0).filter(|c| *c as i32 % across == 0) {
            let at = view.cell_rect(frame.canvas, (column as i32, 0));
            let label = column.to_string();
            let width = self.cells(&label);
            let rect = Rect {
                x: at.x + 1,
                y: frame.ruler_top.y,
                width,
                height: cell_h,
            };
            if rect.x >= frame.ruler_top.x && rect.right() <= frame.ruler_top.right() {
                let ink = if under.is_some_and(|c| c.0 == column as i32) {
                    ACCENT
                } else {
                    DIM
                };
                draw_text(framebuffer, font, &label, rect.x, rect.y + 1, ink);
            }
        }
        let down = ruler_step(tile, cell_h);
        for row in (first.1..first.1 + count.1).filter(|r| *r as i32 % down == 0) {
            let at = view.cell_rect(frame.canvas, (0, row as i32));
            let label = row.to_string();
            let y = at.y + (tile - cell_h).max(0) / 2;
            if y >= frame.ruler_left.y && y + cell_h <= frame.ruler_left.bottom() {
                let ink = if under.is_some_and(|c| c.1 == row as i32) {
                    ACCENT
                } else {
                    DIM
                };
                let x = frame.ruler_left.right() - self.cells(&label) - 1;
                draw_text(framebuffer, font, &label, x, y + 1, ink);
            }
        }
    }

    fn unavailable(&self, ctx: &mut RenderContext<'_>, frame: &Frame, message: &str) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let area = frame.canvas_frame;
        let columns = usize::try_from(area.width / self.metrics.cell_w - 1).unwrap_or(1);
        let mut y = area.y + 2;
        draw_text(
            framebuffer,
            font,
            &ui_text::get("maps-can-t-show-the-tiles"),
            area.x + 2,
            y,
            TEXT,
        );
        y += self.metrics.cell_h + 2;
        for line in wrap(font, message, columns) {
            draw_text(framebuffer, font, &line, area.x + 2, y, ERROR);
            y += self.metrics.cell_h;
        }
    }
}

pub(super) fn in_map(cell: (i32, i32), size: (usize, usize)) -> bool {
    (0..size.0 as i32).contains(&cell.0) && (0..size.1 as i32).contains(&cell.1)
}

#[cfg(test)]
mod tests;
