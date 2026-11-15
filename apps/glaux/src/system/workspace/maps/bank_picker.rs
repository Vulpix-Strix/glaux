//! The selector of the palette bank the brush paints with.

use glaux_i18n::Arg;
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::bg::BG_PALETTE_COLORS;
use glaux_machine::system_rom::SystemIcon;
use glaux_machine::tilemap::{BG_PALETTE_BANKS, BgBank};
use glaux_video::draw::{draw_bg_tile_scaled, draw_text, fill_rect};

use super::bank_slots::SlotEditor;
use super::panels::{Look, Panel, Step, button, fill, foot, header, icon_line, window};
use super::pattern::Attr;
use super::usage::Census;
use super::{MapEditor, MapOut};
use crate::system::screen::{ACCENT, BACKGROUND, DIM, PALE, TEXT, WARN};
use crate::system::sprite_document::SpriteDocument;
use crate::system::workspace::sprites::{SpriteSource, checker, outline};
use crate::system::workspace::{BAR, Rect, RenderContext};
use crate::ui_text;

pub(super) fn area(top: i32) -> Rect {
    Rect {
        x: 34,
        y: top + 50,
        width: 228,
        height: 148,
    }
}

pub(super) fn row(top: i32, bank: usize) -> Rect {
    let window = area(top);
    Rect {
        x: window.x + 3,
        y: window.y + 15 + bank as i32 * 23,
        width: 222,
        height: 22,
    }
}

pub(super) fn edit_button(top: i32, bank: usize) -> Rect {
    let row = row(top, bank);
    Rect {
        x: row.x + 176,
        y: row.y + 4,
        width: 40,
        height: 12,
    }
}

fn banks() -> impl Iterator<Item = BgBank> {
    (0..BG_PALETTE_BANKS as u8).filter_map(BgBank::new)
}

impl MapEditor {
    fn use_bank(&mut self, bank: BgBank) {
        self.edit.brush = Attr::Bank(bank).apply(self.edit.brush);
    }

    pub(super) fn banks_input(
        &mut self,
        event: InputEvent,
        sprites: &mut SpriteDocument,
    ) -> (Step, Option<MapOut>) {
        let top = self.metrics.top;
        match event {
            InputEvent::Break | InputEvent::Enter => return (Step::Close, None),
            InputEvent::Text(key @ '0'..='3') => {
                if let Some(bank) = BgBank::new(key as u8 - b'0') {
                    self.use_bank(bank);
                }
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                for bank in banks() {
                    let index = usize::from(bank.get());
                    if edit_button(top, index).contains(at) {
                        let editor = SlotEditor::open(bank, sprites);
                        return (Step::Open(Panel::Slots(editor)), None);
                    }
                    if row(top, index).contains(at) {
                        self.use_bank(bank);
                    }
                }
            }
            _ => {}
        }
        (Step::Stay, None)
    }

    pub(super) fn render_banks(
        &self,
        ctx: &mut RenderContext<'_>,
        source: &SpriteSource<'_>,
        census: &Census<'_>,
        pointer: Option<Point>,
    ) {
        let top = self.metrics.top;
        let cell_w = self.metrics.cell_w;
        let window_rect = area(top);
        window(ctx.framebuffer, window_rect);
        let brush = self.edit.brush;
        let title = ui_text::get("maps-banks-title");
        let tile = ui_text::get_with(
            "maps-tile-info",
            &[("tile", Arg::Text(&format!("{:04}", brush.tile())))],
        );
        header(ctx, window_rect, &[(&title, PALE)], (&tile, DIM), cell_w);
        let slots = source.preview.slots();
        for bank in banks() {
            let index = usize::from(bank.get());
            let row = row(top, index);
            let chosen = bank == brush.bank();
            if chosen {
                fill(ctx.framebuffer, row, BAR);
            }
            let ink = if chosen { ACCENT } else { TEXT };
            let radio = Rect {
                x: row.x + 2,
                y: row.y + 4,
                width: 5,
                height: 5,
            };
            outline(ctx.framebuffer, radio, ink);
            if chosen {
                fill(ctx.framebuffer, radio.inset(1), ink);
            }
            let name = format!("BGP{index}");
            draw_text(ctx.framebuffer, ctx.font, &name, row.x + 13, row.y, ink);
            let strip = Rect {
                x: row.x + 44,
                y: row.y + 3,
                width: 6,
                height: 8,
            };
            checker(ctx.framebuffer, strip, 2);
            for local in 1..BG_PALETTE_COLORS as u8 {
                if let Some(master) = source.bg.palettes().resolve(bank, local) {
                    let x = strip.x + i32::from(local) * 6;
                    fill_rect(
                        ctx.framebuffer,
                        x,
                        strip.y,
                        6,
                        8,
                        slots[usize::from(master)],
                    );
                }
            }
            let cells = census.bank(bank).cells;
            let used = if cells == 0 {
                ui_text::get("maps-banks-unused")
            } else {
                ui_text::get_with("maps-banks-used", &[("cells", Arg::Number(cells as f64))])
            };
            let ink = if chosen { PALE } else { DIM };
            draw_text(ctx.framebuffer, ctx.font, &used, strip.x, row.y + 10, ink);
            let preview = Rect {
                x: row.x + 147,
                y: row.y + 3,
                width: 16,
                height: 16,
            };
            let edge = if chosen { ACCENT } else { DIM };
            outline(ctx.framebuffer, preview.inset(-1), edge);
            checker(ctx.framebuffer, preview, 4);
            let tile = usize::from(brush.tile());
            draw_bg_tile_scaled(
                ctx.framebuffer,
                source.bg,
                tile,
                bank,
                (preview.x, preview.y),
                2,
                slots,
            );
            let edit = edit_button(top, index);
            let hovered = pointer.is_some_and(|at| edit.contains(at));
            button(
                ctx,
                edit,
                &ui_text::get("maps-layer-edit"),
                Look::Plain,
                hovered,
                cell_w,
            );
        }
        let inner = window_rect.inset(1);
        fill_rect(ctx.framebuffer, inner.x, inner.y + 106, inner.width, 1, BAR);
        let pick = ui_text::get("maps-banks-pick");
        draw_text(
            ctx.framebuffer,
            ctx.font,
            &pick,
            inner.x + 4,
            inner.y + 107,
            PALE,
        );
        let recolor = ui_text::get("maps-banks-recolor");
        icon_line(
            ctx,
            (inner.x + 2, inner.y + 119),
            SystemIcon::Warning,
            &recolor,
            (WARN, BACKGROUND),
        );
        let bar = Rect {
            y: inner.y + 134,
            height: 12,
            ..inner
        };
        let keys = [
            "maps-banks-key-pick",
            "maps-banks-key-use",
            "maps-banks-key-close",
        ];
        foot(ctx, bar, &keys, cell_w);
    }
}

#[cfg(test)]
mod tests;
