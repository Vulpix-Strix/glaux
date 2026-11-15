//! The slots of a background palette bank, each pointing at a master color.

use glaux_i18n::Arg;
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::bg::{BG_PALETTE_COLORS, BgPalettes};
use glaux_machine::palette::Palette;
use glaux_machine::system_rom::SystemIcon;
use glaux_machine::tilemap::BgBank;
use glaux_video::draw::draw_text;
use glaux_video::preview::ObjPreviewPalette;
use std::ops::RangeInclusive;

use super::canvas::{PlateState, plate};
use super::master_color::MasterEditor;
use super::panels::{Look, Panel, Step, button, fill, header, master_name, swatch, window};
use super::usage::Census;
use super::{MapEditor, MapOut};
use crate::system::screen::{ACCENT, DIM, PALE, TEXT};
use crate::system::sprite_document::SpriteDocument;
use crate::system::workspace::sprites::{SpriteSource, outline};
use crate::system::workspace::{BAR, Rect, RenderContext, SYSTEM_COLORS};
use crate::ui_text;

const SLOTS: usize = BG_PALETTE_COLORS;
const COLUMNS: i32 = 32;
const CELL_W: i32 = 6;
const GRID_H: i32 = 40;
const PAGE: usize = 128;

pub(super) fn area(top: i32) -> Rect {
    Rect {
        x: 26,
        y: top + 14,
        width: 244,
        height: 150,
    }
}

/// Changes are written to the document inside one open edit, so the map
/// shows them. Leaving writes the first values back, which leaves no undo
/// step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SlotEditor {
    pub(super) bank: BgBank,
    local: u8,
    first: [u8; SLOTS],
    pub(super) page: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotHit {
    Slot(u8),
    Master(u8),
    PageUp,
    PageDown,
    Revert,
    Apply,
    EditRgb,
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SlotLayout {
    pub(super) window: Rect,
    pub(super) slots: [Rect; SLOTS],
    pub(super) grid: Rect,
    shown: RangeInclusive<u8>,
    page_up: Rect,
    page_down: Rect,
    pub(super) revert: Rect,
    pub(super) apply: Rect,
    pub(super) edit_rgb: Rect,
}

impl SlotLayout {
    pub(super) fn new(top: i32, shown: RangeInclusive<u8>) -> Self {
        let window = area(top);
        let (x, y) = (window.x + 1, window.y + 1);
        let at = |dx: i32, dy: i32, width: i32, height: i32| Rect {
            x: x + dx,
            y: y + dy,
            width,
            height,
        };
        Self {
            window,
            slots: std::array::from_fn(|local| {
                at(
                    4 + (local % 8) as i32 * 13,
                    39 + (local / 8) as i32 * 11,
                    12,
                    10,
                )
            }),
            grid: at(4, 74, COLUMNS * CELL_W, GRID_H),
            shown,
            page_up: at(200, 88, 12, 12),
            page_down: at(214, 88, 12, 12),
            revert: at(4, 133, 46, 12),
            apply: at(54, 133, 88, 12),
            edit_rgb: at(150, 133, 88, 12),
        }
    }

    fn paged(&self) -> bool {
        self.shown.len() <= PAGE
    }

    fn cell_h(&self) -> i32 {
        GRID_H / (self.shown.len() as i32 / COLUMNS)
    }

    pub(super) fn cell(&self, master: u8) -> Option<Rect> {
        let index = i32::from(master.checked_sub(*self.shown.start())?);
        self.shown.contains(&master).then(|| Rect {
            x: self.grid.x + index % COLUMNS * CELL_W,
            y: self.grid.y + index / COLUMNS * self.cell_h(),
            width: CELL_W,
            height: self.cell_h(),
        })
    }

    fn master_at(&self, at: Point) -> Option<u8> {
        self.grid.contains(at).then(|| {
            let column = (i32::from(at.x) - self.grid.x) / CELL_W;
            let row = (i32::from(at.y) - self.grid.y) / self.cell_h();
            // Inside the grid, so it is one of the masters shown.
            *self.shown.start() + (row * COLUMNS + column) as u8
        })
    }

    fn hit(&self, at: Point) -> SlotHit {
        if let Some(local) = self.slots.iter().position(|rect| rect.contains(at)) {
            return SlotHit::Slot(local as u8);
        }
        if let Some(master) = self.master_at(at) {
            return SlotHit::Master(master);
        }
        for (rect, hit, shown) in [
            (self.page_up, SlotHit::PageUp, self.paged()),
            (self.page_down, SlotHit::PageDown, self.paged()),
            (self.revert, SlotHit::Revert, true),
            (self.apply, SlotHit::Apply, true),
            (self.edit_rgb, SlotHit::EditRgb, true),
        ] {
            if shown && rect.contains(at) {
                return hit;
            }
        }
        SlotHit::Nothing
    }
}

fn masters(palettes: &BgPalettes, bank: BgBank) -> [u8; SLOTS] {
    std::array::from_fn(|local| palettes.resolve(bank, local as u8).unwrap_or(0))
}

impl SlotEditor {
    pub(super) fn open(bank: BgBank, sprites: &mut SpriteDocument) -> Self {
        sprites.begin_edit();
        Self {
            bank,
            local: 1,
            first: masters(sprites.bg().palettes(), bank),
            page: 0,
        }
    }

    fn master(&self, palettes: &BgPalettes) -> u8 {
        palettes.resolve(self.bank, self.local).unwrap_or(0)
    }

    pub(super) fn is_changed(&self, palettes: &BgPalettes) -> bool {
        masters(palettes, self.bank)[1..] != self.first[1..]
    }

    pub(super) fn changes(&self, palettes: &BgPalettes) -> Vec<(u8, u8)> {
        let now = masters(palettes, self.bank);
        (1..SLOTS)
            .filter(|&local| now[local] != self.first[local])
            .map(|local| (local as u8, now[local]))
            .collect()
    }

    pub(super) fn revert(&self, sprites: &mut SpriteDocument) {
        for local in 1..SLOTS {
            sprites.set_bg_color(self.bank, local as u8, self.first[local]);
        }
    }

    fn point_at(&mut self, sprites: &mut SpriteDocument, master: u8) {
        sprites.set_bg_color(self.bank, self.local, master);
        self.page = u8::from(usize::from(master) >= PAGE);
    }

    /// Undoes or redoes a step of the document. The open edit holds nothing
    /// then, so it is closed and begun again around the step.
    fn step_history(&mut self, sprites: &mut SpriteDocument, redo: bool) -> Option<MapOut> {
        sprites.end_edit();
        let out = MapEditor::graphics_undo(sprites, redo);
        *self = Self {
            local: self.local,
            page: self.page,
            ..Self::open(self.bank, sprites)
        };
        out
    }

    pub(super) fn status(&self, source: &SpriteSource<'_>) -> String {
        ui_text::get_with(
            "maps-status-slot",
            &[
                ("bank", Arg::Text(&bank_name(self.bank))),
                ("slot", Arg::Text(&format!("{:02}", self.local))),
                (
                    "master",
                    Arg::Text(&master_name(self.master(source.bg.palettes()))),
                ),
            ],
        )
    }
}

fn bank_name(bank: BgBank) -> String {
    format!("BGP{}", bank.get())
}

impl MapEditor {
    /// The masters the grid shows: all 256 when the frame palette can hold
    /// each one's exact color beside the system's, and otherwise the half
    /// `page` names.
    pub(super) fn master_range(
        &self,
        master: &Palette,
        bg: &BgPalettes,
        page: u8,
    ) -> RangeInclusive<u8> {
        let whole =
            ObjPreviewPalette::for_background_and(&self.system, SYSTEM_COLORS, master, bg, 0..=255);
        match (whole.is_ok(), page) {
            (true, _) => 0..=255,
            (false, 0) => 0..=127,
            (false, _) => 128..=255,
        }
    }

    fn slot_layout(&self, master: &Palette, bg: &BgPalettes, page: u8) -> SlotLayout {
        SlotLayout::new(self.metrics.top, self.master_range(master, bg, page))
    }

    pub(super) fn slots_input(
        &mut self,
        editor: &mut SlotEditor,
        event: InputEvent,
        sprites: &mut SpriteDocument,
    ) -> (Step, Option<MapOut>) {
        let video = sprites.video();
        let layout = self.slot_layout(&video.palette, video.bg.palettes(), editor.page);
        let changed = editor.is_changed(sprites.bg().palettes());
        let current = editor.master(sprites.bg().palettes());
        let apply = |sprites: &mut SpriteDocument| {
            sprites.end_edit();
            (Step::Close, None)
        };
        let step = |editor: &mut SlotEditor, sprites: &mut SpriteDocument, by: i32| {
            let master = (i32::from(current) + by).clamp(0, 255) as u8;
            editor.point_at(sprites, master);
        };
        match event {
            InputEvent::Break => {
                editor.revert(sprites);
                sprites.end_edit();
                return (Step::Close, None);
            }
            InputEvent::Enter => return apply(sprites),
            InputEvent::Undo if changed => editor.revert(sprites),
            InputEvent::Undo => return (Step::Stay, editor.step_history(sprites, false)),
            InputEvent::Redo if !changed => {
                return (Step::Stay, editor.step_history(sprites, true));
            }
            InputEvent::Tab { reverse } => {
                let by = if reverse { SLOTS as u8 - 2 } else { 1 };
                editor.local = (editor.local - 1 + by) % (SLOTS as u8 - 1) + 1;
            }
            InputEvent::Left { .. } => step(editor, sprites, -1),
            InputEvent::Right { .. } => step(editor, sprites, 1),
            InputEvent::Up { .. } => step(editor, sprites, -COLUMNS),
            InputEvent::Down { .. } => step(editor, sprites, COLUMNS),
            InputEvent::PageUp { .. } => editor.page = 0,
            InputEvent::PageDown { .. } => editor.page = 1,
            InputEvent::Wheel { delta } => editor.page = u8::from(delta < 0),
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => match layout.hit(at) {
                SlotHit::Slot(local) if local != 0 => editor.local = local,
                SlotHit::Master(master) => editor.point_at(sprites, master),
                SlotHit::PageUp => editor.page = 0,
                SlotHit::PageDown => editor.page = 1,
                SlotHit::Revert => editor.revert(sprites),
                SlotHit::Apply if changed => return apply(sprites),
                SlotHit::EditRgb => {
                    let master = MasterEditor::open(current, sprites);
                    return (Step::Open(Panel::Master(master)), None);
                }
                SlotHit::Slot(_) | SlotHit::Apply | SlotHit::Nothing => {}
            },
            _ => {}
        }
        (Step::Stay, None)
    }

    pub(super) fn render_slots(
        &self,
        ctx: &mut RenderContext<'_>,
        editor: &SlotEditor,
        source: &SpriteSource<'_>,
        census: &Census<'_>,
        pointer: Option<Point>,
    ) {
        let palettes = source.bg.palettes();
        let layout = self.slot_layout(source.master, palettes, editor.page);
        let cell_w = self.metrics.cell_w;
        let hover = pointer.map_or(SlotHit::Nothing, |at| layout.hit(at));
        let slots = source.preview.slots();
        let name = bank_name(editor.bank);
        let inner = layout.window.inset(1);
        let text = |ctx: &mut RenderContext<'_>, text: &str, dx: i32, dy: i32, ink: u8| {
            draw_text(
                ctx.framebuffer,
                ctx.font,
                text,
                inner.x + dx,
                inner.y + dy,
                ink,
            );
        };
        window(ctx.framebuffer, layout.window);
        let title = ui_text::get("maps-slots-title");
        let live = ui_text::get("maps-slots-live");
        header(
            ctx,
            layout.window,
            &[(&title, PALE), (&name, ACCENT)],
            (&live, DIM),
            cell_w,
        );
        let band = Rect {
            y: inner.y + 12,
            height: 13,
            ..inner
        };
        fill(ctx.framebuffer, band, BAR);
        let scope = ui_text::get_with(
            "maps-slots-scope",
            &[
                ("bank", Arg::Text(&name)),
                ("cells", Arg::Number(census.bank(editor.bank).cells as f64)),
            ],
        );
        text(ctx, &scope, 4, 13, PALE);

        text(ctx, &ui_text::get("maps-slots-count"), 4, 26, DIM);
        for (local, rect) in layout.slots.into_iter().enumerate() {
            let local = local as u8;
            let edge = if local == editor.local {
                ACCENT
            } else if hover == SlotHit::Slot(local) && local != 0 {
                PALE
            } else {
                BAR
            };
            let color = palettes.resolve(editor.bank, local);
            swatch(
                ctx.framebuffer,
                rect,
                color.map(|master| slots[usize::from(master)]),
                edge,
            );
        }
        let uses = ui_text::get_with(
            "maps-slots-uses",
            &[("slot", Arg::Text(&format!("{:02}", editor.local)))],
        );
        text(ctx, &uses, 112, 26, PALE);
        let first = editor.first[usize::from(editor.local)];
        let now = editor.master(palettes);
        let chip = |ctx: &mut RenderContext<'_>, dx: i32, master: u8, edge: u8, ink: u8| {
            let rect = Rect {
                x: inner.x + dx,
                y: inner.y + 39,
                width: 12,
                height: 10,
            };
            swatch(
                ctx.framebuffer,
                rect,
                Some(slots[usize::from(master)]),
                edge,
            );
            draw_text(
                ctx.framebuffer,
                ctx.font,
                &master_name(master),
                rect.right() + 2,
                rect.y - 2,
                ink,
            );
        };
        if first == now {
            chip(ctx, 112, now, ACCENT, TEXT);
        } else {
            chip(ctx, 112, first, BAR, DIM);
            text(ctx, "->", 162, 37, DIM);
            chip(ctx, 180, now, ACCENT, TEXT);
        }
        text(ctx, &ui_text::get("maps-slots-clear"), 112, 49, DIM);

        text(ctx, &ui_text::get("maps-slots-masters"), 4, 61, DIM);
        text(ctx, &ui_text::get("maps-slots-masters-all"), 92, 61, PALE);
        for master in layout.shown.clone() {
            if let Some(cell) = layout.cell(master) {
                fill(ctx.framebuffer, cell, slots[usize::from(master)]);
            }
        }
        if let SlotHit::Master(master) = hover
            && let Some(cell) = layout.cell(master)
        {
            outline(ctx.framebuffer, cell.inset(-1), PALE);
        }
        if let Some(cell) = layout.cell(now) {
            outline(ctx.framebuffer, cell.inset(-1), TEXT);
        }
        let (low, high) = (*layout.shown.start(), *layout.shown.end());
        text(ctx, &format!("{low:03}"), 200, 72, DIM);
        text(ctx, &format!("{high:03}"), 200, 102, DIM);
        if layout.paged() {
            for (rect, icon, hit, page) in [
                (layout.page_up, SystemIcon::Expand, SlotHit::PageUp, 0),
                (layout.page_down, SystemIcon::Collapse, SlotHit::PageDown, 1),
            ] {
                let state = if editor.page == page {
                    PlateState::Off
                } else if hover == hit {
                    PlateState::Hover
                } else {
                    PlateState::Idle
                };
                plate(ctx, rect, icon, state);
            }
        }
        text(ctx, &ui_text::get("maps-slots-hint"), 4, 115, DIM);

        let footer = Rect {
            y: inner.y + 130,
            height: 18,
            ..inner
        };
        fill(ctx.framebuffer, footer, BAR);
        let changed = editor.is_changed(palettes);
        let look = |look: Look| if changed { look } else { Look::Off };
        let revert = ui_text::get("maps-slots-revert");
        button(
            ctx,
            layout.revert,
            &revert,
            look(Look::Plain),
            hover == SlotHit::Revert,
            cell_w,
        );
        let apply = ui_text::get_with("maps-slots-apply", &[("bank", Arg::Text(&name))]);
        button(
            ctx,
            layout.apply,
            &apply,
            look(Look::Accent),
            hover == SlotHit::Apply,
            cell_w,
        );
        let edit = ui_text::get_with(
            "maps-slots-edit-rgb",
            &[("master", Arg::Text(&master_name(now)))],
        );
        button(
            ctx,
            layout.edit_rgb,
            &edit,
            Look::Plain,
            hover == SlotHit::EditRgb,
            cell_w,
        );
    }
}

#[cfg(test)]
mod tests;
