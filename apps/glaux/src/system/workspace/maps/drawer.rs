//! Drawing the drawer.

use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::system_rom::SystemIcon;
use glaux_machine::tilemap::{BgLayer, TileEntry};
use glaux_video::draw::{draw_ink_image_centered, draw_text, fill_rect};

use super::MapEditor;
use super::pattern::uses;
use super::canvas::draw_cell;
use super::compose::Cell;
use super::frame::{DrawerSize, EYE_WIDTH, Frame, Hit, LOCK_WIDTH, STRIP_COLUMNS};
use crate::system::map_document::MapDocument;
use crate::system::screen::{ACCENT, BACKGROUND, DIM, PALE, TEXT};
use crate::system::workspace::sprites::{SpriteSource, checker, outline};
use crate::system::workspace::{BAR, HOVER_SHADE, IDLE_SHADE, Rect, RenderContext, take_cells};
use crate::ui_text;
use glaux_i18n::Arg;

const TILES: usize = 1024;

fn flags(entry: TileEntry) -> String {
    let mut out = String::new();
    for (set, letter) in [
        (entry.flip_x(), 'X'),
        (entry.flip_y(), 'Y'),
        (entry.priority(), 'P'),
    ] {
        if set {
            out.push(letter);
        }
    }
    out
}

/// The bank and the flags the brush carries, as `BGP1 XY`.
pub(super) fn brush_bank_text(entry: TileEntry) -> String {
    let flags = flags(entry);
    let bank = entry.bank().get();
    if flags.is_empty() {
        format!("BGP{bank}")
    } else {
        format!("BGP{bank} {flags}")
    }
}

impl MapEditor {
    pub(super) fn drawer(
        &self,
        ctx: &mut RenderContext<'_>,
        frame: &Frame,
        document: &MapDocument,
        source: &SpriteSource<'_>,
        hover: Hit,
    ) {
        let bar = frame.drawer_bar;
        fill_rect(ctx.framebuffer, bar.x, bar.y, bar.width, bar.height, BAR);

        let icon = match frame.size {
            DrawerSize::Collapsed | DrawerSize::Compact => SystemIcon::Expand,
            DrawerSize::Expanded => SystemIcon::Collapse,
        };
        let ink = if hover == Hit::DrawerSize { TEXT } else { PALE };
        let button = frame.size_button;
        draw_ink_image_centered(
            ctx.framebuffer,
            ctx.ui.icon(icon),
            (button.x, button.y, button.width, button.height),
            &[ink, ink, BAR],
        );
        draw_text(
            ctx.framebuffer,
            ctx.font,
            &ui_text::get("maps-tiles"),
            frame.tiles_label.0,
            frame.tiles_label.1,
            DIM,
        );

        let brush = self.edit.brush;
        checker(ctx.framebuffer, frame.brush, 2);
        draw_cell(
            ctx.framebuffer,
            source.bg,
            &Cell::single(brush),
            (frame.brush.x, frame.brush.y),
            1,
            source.preview.slots(),
            frame.brush,
        );
        outline(ctx.framebuffer, frame.brush.inset(-1), DIM);
        draw_text(
            ctx.framebuffer,
            ctx.font,
            &format!("{:04}", brush.tile()),
            frame.brush_number.0,
            frame.brush_number.1,
            TEXT,
        );
        let brush_info = match self.edit.pattern {
            (1, 1) => brush_bank_text(brush),
            (width, height) => format!("{width}x{height}"),
        };
        draw_text(
            ctx.framebuffer,
            ctx.font,
            &brush_info,
            frame.bank_text.0,
            frame.bank_text.1,
            PALE,
        );
        self.picker_button(ctx, frame, hover);
        for separator in [frame.first_separator, frame.second_separator] {
            fill_rect(
                ctx.framebuffer,
                separator.x,
                separator.y + 2,
                1,
                separator.height - 4,
                IDLE_SHADE,
            );
        }

        if frame.size == DrawerSize::Collapsed {
            self.chips(ctx, frame, hover);
        } else {
            draw_text(
                ctx.framebuffer,
                ctx.font,
                &ui_text::get("maps-layers"),
                frame.layers_label.0,
                frame.layers_label.1,
                DIM,
            );
            self.strip(ctx.framebuffer, frame, source, hover);
            self.layer_list(ctx, frame, document, hover);
            self.tile_info(ctx, frame, document, hover);
        }
    }

    fn picker_button(&self, ctx: &mut RenderContext<'_>, frame: &Frame, hover: Hit) {
        let rect = frame.picker_button;
        let open = self.edit.picker.is_some();
        let ink = if open {
            fill_rect(ctx.framebuffer, rect.x, rect.y, rect.width, rect.height, BACKGROUND);
            fill_rect(ctx.framebuffer, rect.x, rect.y, rect.width, 1, ACCENT);
            ACCENT
        } else if hover == Hit::Picker {
            TEXT
        } else {
            PALE
        };
        let label = ui_text::get("maps-picker");
        let x = rect.x + (rect.width - self.cells(&label)) / 2;
        draw_text(ctx.framebuffer, ctx.font, &label, x, rect.y, ink);
    }

    fn tile_info(
        &self,
        ctx: &mut RenderContext<'_>,
        frame: &Frame,
        document: &MapDocument,
        hover: Hit,
    ) {
        let (Some(rule), Some(info), Some(button)) =
            (frame.tile_rule, frame.tile_info, frame.edit_button)
        else {
            return;
        };
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        fill_rect(framebuffer, rule.x, rule.y, rule.width, rule.height, BAR);
        let tile = self.edit.brush.tile();
        let used = (0..3)
            .filter_map(BgLayer::from_index)
            .map(|layer| uses(document.layer(layer), tile))
            .sum::<usize>();
        let head = ui_text::get_with("maps-tile-info", &[("tile", Arg::Text(&format!("{tile:04}")))]);
        draw_text(framebuffer, font, &head, info.0, info.1, TEXT);
        let used = ui_text::get_with("maps-tile-used", &[("count", Arg::Number(used as f64))]);
        let x = info.0 + self.cells(&head) + 12;
        let room = usize::try_from((frame.drawer.right() - x) / self.metrics.cell_w).unwrap_or(0);
        draw_text(framebuffer, font, take_cells(font, &used, room), x, info.1, DIM);

        let hovered = hover == Hit::EditTile;
        fill_rect(
            framebuffer,
            button.x,
            button.y,
            button.width,
            button.height,
            if hovered { HOVER_SHADE } else { BAR },
        );
        outline(framebuffer, button, DIM);
        let label = ui_text::get("maps-tile-menu-edit");
        let x = button.x + (button.width - self.cells(&label)) / 2;
        draw_text(framebuffer, font, &label, x, button.y, if hovered { TEXT } else { PALE });
    }

    fn chips(&self, ctx: &mut RenderContext<'_>, frame: &Frame, hover: Hit) {
        let (framebuffer, font, ui) = (&mut *ctx.framebuffer, ctx.font, ctx.ui);
        for layer in BgLayer::ALL {
            let rect = frame.chips[layer.index()];
            let index = layer.index();
            let editing = layer == self.edit.layer;
            let shown = self.edit.visible[index];
            let (face, ink) = if editing {
                (ACCENT, BACKGROUND)
            } else if hover == Hit::Chip(layer) {
                (HOVER_SHADE, TEXT)
            } else {
                (BAR, if shown { PALE } else { IDLE_SHADE })
            };
            fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, face);
            let eye = if shown {
                SystemIcon::Visible
            } else {
                SystemIcon::Hidden
            };
            draw_ink_image_centered(
                framebuffer,
                ui.icon(eye),
                (rect.x, rect.y, EYE_WIDTH, rect.height),
                &[ink, ink, face],
            );
            draw_text(
                framebuffer,
                font,
                &layer.to_string(),
                rect.x + EYE_WIDTH,
                rect.y,
                ink,
            );
            let name_end = rect.x + EYE_WIDTH + self.cells("BG0");
            if rect.right() - name_end >= LOCK_WIDTH {
                let lock_ink = if self.edit.locked[index] {
                    ink
                } else {
                    IDLE_SHADE
                };
                draw_ink_image_centered(
                    framebuffer,
                    ui.icon(SystemIcon::Locked),
                    (name_end, rect.y, LOCK_WIDTH, rect.height),
                    &[lock_ink, lock_ink, face],
                );
            }
        }
    }

    fn strip(
        &self,
        framebuffer: &mut Framebuffer,
        frame: &Frame,
        source: &SpriteSource<'_>,
        hover: Hit,
    ) {
        let (Some(strip), Some(framed)) = (frame.strip, frame.strip_frame()) else {
            return;
        };
        outline(framebuffer, framed, DIM);
        checker(framebuffer, strip, 4);
        let bank = self.edit.brush.bank();
        for row in 0..frame.strip_rows {
            for column in 0..STRIP_COLUMNS {
                let tile = (self.edit.strip_top + row) * STRIP_COLUMNS + column;
                let Some(entry) = u16::try_from(tile)
                    .ok()
                    .and_then(|tile| TileEntry::new(tile, false, false, false, bank))
                else {
                    continue;
                };
                let at = (strip.x + column as i32 * 8, strip.y + row as i32 * 8);
                draw_cell(
                    framebuffer,
                    source.bg,
                    &Cell::single(entry),
                    at,
                    1,
                    source.preview.slots(),
                    strip,
                );
            }
        }
        let cell_of = |tile: usize| {
            let row = (tile / STRIP_COLUMNS).checked_sub(self.edit.strip_top)?;
            (row < frame.strip_rows).then(|| Rect {
                x: strip.x + (tile % STRIP_COLUMNS) as i32 * 8,
                y: strip.y + row as i32 * 8,
                width: 8,
                height: 8,
            })
        };
        if let Hit::Tile(tile) = hover
            && let Some(rect) = cell_of(tile)
        {
            outline(framebuffer, rect, PALE);
        }
        if let Some(rect) = cell_of(usize::from(self.edit.brush.tile())) {
            outline(framebuffer, rect, ACCENT);
        }

        let track = Rect {
            x: framed.right() + 1,
            y: strip.y,
            width: super::frame::SCROLLBAR,
            height: strip.height,
        };
        fill_rect(
            framebuffer,
            track.x,
            track.y,
            track.width,
            track.height,
            BAR,
        );
        let total = (TILES / STRIP_COLUMNS) as i32;
        let (top, rows) = (self.edit.strip_top as i32, frame.strip_rows as i32);
        let from = track.y + track.height * top / total;
        let to = track.y + track.height * (top + rows).min(total) / total;
        let thumb = (to - from).max(2);
        fill_rect(
            framebuffer,
            track.x,
            from.min(track.bottom() - thumb),
            track.width,
            thumb,
            DIM,
        );
    }

    /// Hiding and locking belong to this editor and never reach the cartridge.
    fn layer_list(
        &self,
        ctx: &mut RenderContext<'_>,
        frame: &Frame,
        document: &MapDocument,
        hover: Hit,
    ) {
        let (framebuffer, font, ui) = (&mut *ctx.framebuffer, ctx.font, ctx.ui);
        let Some(rows) = frame.layer_rows else {
            return;
        };
        let cell_w = self.metrics.cell_w;
        for (slot, row) in rows.iter().enumerate() {
            let layer = Frame::layer_in_slot(slot);
            let index = layer.index();
            let editing = layer == self.edit.layer;
            let hovered = matches!(
                hover,
                Hit::LayerName(l) | Hit::Eye(l) | Hit::Lock(l) if l == layer
            );
            let (face, ink, dim) = if editing {
                (PALE, BACKGROUND, IDLE_SHADE)
            } else if hovered {
                (HOVER_SHADE, TEXT, PALE)
            } else {
                (BACKGROUND, TEXT, IDLE_SHADE)
            };
            fill_rect(
                framebuffer,
                row.row.x,
                row.row.y,
                row.row.width,
                row.row.height,
                face,
            );

            let shown = self.edit.visible[index];
            let eye = if shown {
                SystemIcon::Visible
            } else {
                SystemIcon::Hidden
            };
            let eye_ink = if shown { ink } else { dim };
            let area = (row.eye.x, row.eye.y, row.eye.width, row.eye.height);
            draw_ink_image_centered(framebuffer, ui.icon(eye), area, &[eye_ink, eye_ink, face]);
            let locked = self.edit.locked[index];
            let lock_ink = if locked { ink } else { dim };
            let area = (row.lock.x, row.lock.y, row.lock.width, row.lock.height);
            draw_ink_image_centered(
                framebuffer,
                ui.icon(SystemIcon::Locked),
                area,
                &[lock_ink, lock_ink, face],
            );

            let name = match &document.meta().layer_names[index] {
                Some(name) => format!("{layer} {name}"),
                None => layer.to_string(),
            };
            let tag = ui_text::get("maps-layer-edit");
            let columns = usize::try_from(row.name.width / cell_w - 1).unwrap_or(0);
            let text = take_cells(font, &name, columns);
            let y = self.text_y(row.row.y, row.row.height);
            let text_ink = if shown { ink } else { dim };
            draw_text(
                framebuffer,
                font,
                text,
                row.name.x + cell_w / 2,
                y,
                text_ink,
            );
            let used = i32::try_from(font.text_cells(text)).unwrap_or(0) + 1;
            let tag_cells = i32::try_from(font.text_cells(&tag)).unwrap_or(0);
            if editing && used + tag_cells < row.name.width / cell_w {
                let x = row.name.right() - (tag_cells + 1) * cell_w;
                draw_text(framebuffer, font, &tag, x, y, BACKGROUND);
            }
        }
        if let Some((x, y)) = frame.layer_note {
            draw_text(
                framebuffer,
                font,
                &ui_text::get("maps-layer-note"),
                x,
                y,
                DIM,
            );
        }
    }
}

#[cfg(test)]
mod tests;
