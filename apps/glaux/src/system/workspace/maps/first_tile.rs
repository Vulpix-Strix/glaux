//! Starting a map that has no tiles yet.

use glaux_i18n::Arg;
use glaux_input::Point;
use glaux_machine::bg::{BG_TILE_BYTES, BgTileBank};
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::tilemap::{BgLayer, TileEntry};
use glaux_video::draw::draw_text;

use super::frame::Frame;
use super::panels::{Look, Panel, button, fill, focus_ring, window};
use super::pattern::TILES;
use super::tile_editor::TileEditor;
use super::{MapEditor, MapOut};
use crate::system::map_document::MapDocument;
use crate::system::screen::{ACCENT, BACKGROUND, DIM, PALE, TEXT};
use crate::system::sprite_document::SpriteDocument;
use crate::system::workspace::dialog::text_cells;
use crate::system::workspace::request::MapRequest;
use crate::system::workspace::{BAR, Rect, RenderContext};
use crate::ui_text;

const CARD: (i32, i32) = (180, 90);
const TIP_LINE: i32 = 11;

pub(super) fn is_blank(tiles: &BgTileBank, tile: usize) -> bool {
    let bytes = tiles.as_bytes();
    bytes
        .get(tile * BG_TILE_BYTES..(tile + 1) * BG_TILE_BYTES)
        .is_none_or(|tile| tile.iter().all(|&byte| byte == 0))
}

/// The lowest tile with a pixel drawn. Tile 0 never shows, so it does not
/// count.
pub(super) fn first_drawn(tiles: &BgTileBank) -> Option<usize> {
    (1..TILES).find(|&tile| !is_blank(tiles, tile))
}

pub(super) fn first_free(tiles: &BgTileBank) -> Option<usize> {
    (1..TILES).find(|&tile| is_blank(tiles, tile))
}

pub(super) fn map_is_blank(document: &MapDocument) -> bool {
    BgLayer::ALL
        .into_iter()
        .all(|layer| document.layer(layer).is_blank())
}

fn tile_arg(tile: u16) -> String {
    format!("{tile:04}")
}

impl MapEditor {
    fn brush_tile(&mut self, tile: usize) {
        let brush = self.edit.brush;
        let entry = u16::try_from(tile).ok().and_then(|tile| {
            TileEntry::new(
                tile,
                brush.flip_x(),
                brush.flip_y(),
                brush.priority(),
                brush.bank(),
            )
        });
        if let Some(entry) = entry {
            self.edit.brush = entry;
            self.edit.pattern = (1, 1);
        }
    }

    /// Moves a brush left on a blank tile to the first drawn one, so a map
    /// can be painted as soon as the cartridge has tiles.
    pub(super) fn arm_brush(&mut self, tiles: &BgTileBank) {
        if self.has_panel() || !is_blank(tiles, usize::from(self.edit.brush.tile())) {
            return;
        }
        if let Some(tile) = first_drawn(tiles) {
            self.brush_tile(tile);
        }
    }

    pub(super) fn open_tile_editor(&mut self, sprites: &mut SpriteDocument) -> Option<MapOut> {
        let tile = self.edit.brush.tile();
        if tile == 0 {
            return Some(MapOut::Notice(ui_text::get("maps-tile-0-transparent")));
        }
        sprites.end_edit();
        self.close_picker();
        self.edit.attrs = None;
        self.edit.coach = false;
        self.edit.panels = vec![Panel::Tile(TileEditor::new(tile, false))];
        Some(MapOut::Request(MapRequest::Survey))
    }

    pub(super) fn open_palettes(&mut self, sprites: &mut SpriteDocument) -> Option<MapOut> {
        sprites.end_edit();
        self.close_picker();
        self.edit.attrs = None;
        self.edit.panels = vec![Panel::Banks];
        Some(MapOut::Request(MapRequest::Survey))
    }

    pub(super) fn new_tile(&mut self, sprites: &mut SpriteDocument) -> Option<MapOut> {
        let Some(tile) = first_free(sprites.bg_tiles()) else {
            return Some(MapOut::Notice(ui_text::get("maps-no-free-tile")));
        };
        sprites.end_edit();
        self.close_picker();
        self.edit.attrs = None;
        self.edit.coach = false;
        self.brush_tile(tile);
        // `first_free` is below the tile count, which fits.
        self.edit.panels = vec![Panel::Tile(TileEditor::new(tile as u16, true))];
        None
    }

    pub(super) fn shows_guide(&self, document: &MapDocument, tiles: &BgTileBank) -> bool {
        !self.has_panel()
            && self.edit.picker.is_none()
            && map_is_blank(document)
            && first_drawn(tiles).is_none()
    }

    pub(super) fn guide_card(frame: &Frame) -> Rect {
        let canvas = frame.canvas;
        Rect {
            x: canvas.x + (canvas.width - CARD.0) / 2,
            y: canvas.y + (canvas.height - CARD.1) / 2,
            width: CARD.0,
            height: CARD.1,
        }
    }

    pub(super) fn guide_button(frame: &Frame) -> Rect {
        let card = Self::guide_card(frame).inset(1);
        Rect {
            x: card.x + 7,
            y: card.y + 70,
            width: 76,
            height: 12,
        }
    }

    pub(super) fn render_guide(
        &self,
        ctx: &mut RenderContext<'_>,
        frame: &Frame,
        document: &MapDocument,
        pointer: Option<Point>,
    ) {
        let card = Self::guide_card(frame);
        window(ctx.framebuffer, card);
        let inner = card.inset(1);
        let cell_w = self.metrics.cell_w;
        let x = inner.x + 7;
        let title = ui_text::get_with(
            "maps-empty-title",
            &[("name", Arg::Text(&document.meta().name))],
        );
        let columns = usize::try_from((inner.width - 14) / cell_w).unwrap_or(0);
        let title = crate::system::workspace::take_cells(ctx.font, &title, columns);
        draw_text(ctx.framebuffer, ctx.font, title, x, inner.y + 4, TEXT);
        let about = ui_text::get("maps-empty-about");
        draw_text(ctx.framebuffer, ctx.font, &about, x, inner.y + 16, DIM);
        for (step, id) in [
            "maps-empty-step-1",
            "maps-empty-step-2",
            "maps-empty-step-3",
        ]
        .into_iter()
        .enumerate()
        {
            let y = inner.y + 30 + step as i32 * 11;
            draw_text(
                ctx.framebuffer,
                ctx.font,
                &(step + 1).to_string(),
                x,
                y,
                ACCENT,
            );
            draw_text(
                ctx.framebuffer,
                ctx.font,
                &ui_text::get(id),
                x + 3 * cell_w,
                y,
                PALE,
            );
        }
        let rect = Self::guide_button(frame);
        let hovered = pointer.is_some_and(|at| rect.contains(at));
        button(
            ctx,
            rect,
            &ui_text::get("maps-empty-new"),
            Look::Accent,
            hovered,
            cell_w,
        );
        focus_ring(ctx.framebuffer, rect);
        let key = ui_text::get("maps-empty-key");
        draw_text(
            ctx.framebuffer,
            ctx.font,
            &key,
            rect.right() + 8,
            rect.y,
            DIM,
        );
    }

    pub(super) fn render_tip(
        &self,
        ctx: &mut RenderContext<'_>,
        ring: Rect,
        canvas: Rect,
        blank: bool,
    ) {
        if !blank && !self.edit.coach {
            return;
        }
        let cell_w = self.metrics.cell_w;
        let paint = ui_text::get("maps-tip-paint");
        let keys = [
            ("D", ui_text::get("maps-tip-edit")),
            ("T", ui_text::get("maps-tip-picker")),
        ];
        let lines = if blank { 1 } else { 2 };
        let keys_cells: i32 = keys
            .iter()
            .map(|(key, text)| 2 + text_cells(key) + text_cells(text))
            .sum();
        let cells = if blank {
            text_cells(&paint)
        } else {
            text_cells(&paint).max(keys_cells)
        };
        let width = cells * cell_w + 8;
        let height = lines * TIP_LINE + 3;
        let y = if blank { ring.y + 2 } else { ring.y - height };
        let tip = Rect {
            x: (ring.right() + 4).min(canvas.right() - width).max(canvas.x),
            y: y.max(canvas.y),
            width,
            height,
        };
        tip_box(ctx.framebuffer, tip);
        draw_text(
            ctx.framebuffer,
            ctx.font,
            &paint,
            tip.x + 4,
            tip.y,
            BACKGROUND,
        );
        if !blank {
            let mut x = tip.x + 4;
            for (key, text) in keys {
                draw_text(ctx.framebuffer, ctx.font, key, x, tip.y + TIP_LINE, BAR);
                x += (text_cells(key) + 1) * cell_w;
                draw_text(
                    ctx.framebuffer,
                    ctx.font,
                    &text,
                    x,
                    tip.y + TIP_LINE,
                    BACKGROUND,
                );
                x += (text_cells(&text) + 2) * cell_w;
            }
        }
    }

    pub(super) fn empty_status(
        &self,
        document: &MapDocument,
        tiles: &BgTileBank,
    ) -> Option<String> {
        if self.has_panel() || !map_is_blank(document) {
            return None;
        }
        let name = Arg::Text(&document.meta().name);
        let tile = self.edit.brush.tile();
        Some(if is_blank(tiles, usize::from(tile)) {
            ui_text::get_with("maps-status-empty-new", &[("name", name)])
        } else {
            ui_text::get_with(
                "maps-status-empty-ready",
                &[("name", name), ("tile", Arg::Text(&tile_arg(tile)))],
            )
        })
    }
}

fn tip_box(framebuffer: &mut Framebuffer, rect: Rect) {
    fill(framebuffer, rect, BACKGROUND);
    fill(framebuffer, rect.inset(1), PALE);
}

#[cfg(test)]
mod tests;
