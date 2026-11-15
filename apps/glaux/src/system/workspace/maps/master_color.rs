//! The RGB of a master color, which every palette slot pointing at it shows.

use glaux_i18n::Arg;
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, OBJ_PALETTE_COLORS, ObjBank};
use glaux_machine::palette::Rgb;
use glaux_machine::sprite::OBJ_TILE_CAPACITY;
use glaux_machine::system_rom::SystemIcon;
use glaux_machine::tilemap::{BG_PALETTE_BANKS, BgBank};
use glaux_video::draw::{draw_text, fill_rect};

use super::bank_slots::area;
use super::panels::{Look, Panel, Step, button, fill, header, icon_line, master_name, window};
use super::usage::Census;
use super::{MapEditor, MapOut};
use crate::system::screen::{ACCENT, BACKGROUND, DIM, PALE, TEXT, WARN};
use crate::system::sprite_document::{ColorTarget, PaletteBank, SpriteDocument};
use crate::system::workspace::sprites::{
    SCRATCH_BASE, SpriteSource, format_hex, outline, parse_hex,
};
use crate::system::workspace::{BAR, Rect, RenderContext};
use crate::ui_text;

const CHANNELS: usize = 3;
const TRACK_W: i32 = 128;
const STEPS: usize = 16;
const STEP_W: i32 = TRACK_W / STEPS as i32;
const KNOB_W: i32 = 3;
/// Scratch entries: each channel's ramp, then the color before the edit and
/// the color now.
const WAS_SLOT: usize = SCRATCH_BASE + CHANNELS * STEPS;
const NOW_SLOT: usize = WAS_SLOT + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Channel(usize),
    Hex,
}

/// A master color while its RGB is being changed. The color is written to
/// the document as it changes, so the map shows it, and cancelling writes
/// the first color back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MasterEditor {
    index: u8,
    was: Rgb,
    now: Rgb,
    focus: Focus,
    typing: Option<String>,
    drag: Option<usize>,
    bg: Vec<(BgBank, u8)>,
    obj: Vec<(ObjBank, u8)>,
    /// Object tiles that show the color through their own palette.
    sprites: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MasterHit {
    Track(usize),
    Hex,
    Cancel,
    Apply,
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MasterLayout {
    window: Rect,
    pub(super) tracks: [Rect; CHANNELS],
    hex: Rect,
    was: Rect,
    now: Rect,
    pub(super) cancel: Rect,
    pub(super) apply: Rect,
}

impl MasterLayout {
    pub(super) fn new(top: i32) -> Self {
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
            tracks: std::array::from_fn(|channel| at(16, 54 + channel as i32 * 12, TRACK_W, 6)),
            hex: at(26, 90, 46, 12),
            was: at(180, 54, 24, 20),
            now: at(206, 54, 24, 20),
            cancel: at(4, 133, 46, 12),
            apply: at(54, 133, 82, 12),
        }
    }

    fn hit(&self, at: Point) -> MasterHit {
        let reach = |track: &Rect| track.inset(-2).contains(at);
        if let Some(channel) = self.tracks.iter().position(reach) {
            return MasterHit::Track(channel);
        }
        for (rect, hit) in [
            (self.hex, MasterHit::Hex),
            (self.cancel, MasterHit::Cancel),
            (self.apply, MasterHit::Apply),
        ] {
            if rect.contains(at) {
                return hit;
            }
        }
        MasterHit::Nothing
    }

    fn value_at(&self, channel: usize, at: Point) -> u8 {
        let span = TRACK_W - 1;
        let offset = (i32::from(at.x) - self.tracks[channel].x).clamp(0, span);
        // Within 0..=255.
        ((offset * 255 + span / 2) / span) as u8
    }
}

fn channels(color: Rgb) -> [u8; CHANNELS] {
    [color.r, color.g, color.b]
}

fn with_channel(color: Rgb, channel: usize, value: u8) -> Rgb {
    let mut parts = channels(color);
    parts[channel] = value;
    Rgb {
        r: parts[0],
        g: parts[1],
        b: parts[2],
    }
}

fn hex(color: Rgb) -> String {
    format_hex(color).trim_start_matches('#').to_lowercase()
}

impl MasterEditor {
    pub(super) fn open(index: u8, sprites: &SpriteDocument) -> Self {
        let color = sprites.video().palette.get(index);
        let obj: Vec<_> = (0..OBJ_PALETTE_BANKS as u8)
            .filter_map(ObjBank::new)
            .flat_map(|bank| (1..OBJ_PALETTE_COLORS as u8).map(move |local| (bank, local)))
            .filter(|&(bank, local)| sprites.obj().palettes().resolve(bank, local) == Some(index))
            .collect();
        let bg = (0..BG_PALETTE_BANKS as u8)
            .filter_map(BgBank::new)
            .flat_map(|bank| (1..OBJ_PALETTE_COLORS as u8).map(move |local| (bank, local)))
            .filter(|&(bank, local)| sprites.bg().palettes().resolve(bank, local) == Some(index))
            .collect();
        let shown = obj.first().and_then(|&(bank, local)| {
            let target = ColorTarget {
                bank: PaletteBank::Obj(bank),
                local,
            };
            // No tile is left out of the count.
            sprites.color_uses(target, OBJ_TILE_CAPACITY)
        });
        Self {
            index,
            was: color,
            now: color,
            focus: Focus::Hex,
            typing: None,
            drag: None,
            bg,
            obj,
            sprites: shown.map_or(0, |uses| uses.tiles),
        }
    }

    pub(super) fn cancel(&self, sprites: &mut SpriteDocument) {
        sprites.set_master_color(self.index, self.was);
    }

    fn set(&mut self, sprites: &mut SpriteDocument, color: Rgb) {
        self.now = color;
        sprites.set_master_color(self.index, color);
    }

    pub(super) fn scratch_colors(&self) -> Vec<Rgb> {
        let ramp = |channel: usize| {
            (0..STEPS).map(move |step| {
                // Within 0..=255.
                let value = ((2 * step + 1) * 255 / (2 * STEPS)) as u8;
                with_channel(self.now, channel, value)
            })
        };
        (0..CHANNELS)
            .flat_map(ramp)
            .chain([self.was, self.now])
            .collect()
    }

    fn kinds(&self) -> String {
        match (self.bg.is_empty(), self.obj.is_empty()) {
            (false, false) => "BG + OBJ".to_owned(),
            (false, true) => "BG".to_owned(),
            (true, false) => "OBJ".to_owned(),
            (true, true) => ui_text::get("maps-master-unused"),
        }
    }

    pub(super) fn status(&self) -> String {
        ui_text::get_with(
            "maps-status-master",
            &[
                ("master", Arg::Text(&format!("{:03}", self.index))),
                ("kinds", Arg::Text(&self.kinds())),
            ],
        )
    }
}

impl MapEditor {
    pub(super) fn master_input(
        &mut self,
        editor: &mut MasterEditor,
        event: InputEvent,
        sprites: &mut SpriteDocument,
    ) -> (Step, Option<MapOut>) {
        let layout = MasterLayout::new(self.metrics.top);
        let nudge = |editor: &mut MasterEditor, sprites: &mut SpriteDocument, by: i32| {
            if let Focus::Channel(channel) = editor.focus {
                let value = (i32::from(channels(editor.now)[channel]) + by).clamp(0, 255) as u8;
                editor.set(sprites, with_channel(editor.now, channel, value));
            }
        };
        match event {
            InputEvent::Break => {
                editor.cancel(sprites);
                return (Step::Close, None);
            }
            InputEvent::Enter => return (self.apply_master(sprites), None),
            InputEvent::Tab { reverse } => {
                let order = [
                    Focus::Channel(0),
                    Focus::Channel(1),
                    Focus::Channel(2),
                    Focus::Hex,
                ];
                let at = order
                    .iter()
                    .position(|&focus| focus == editor.focus)
                    .unwrap_or(0);
                let by = if reverse { order.len() - 1 } else { 1 };
                editor.focus = order[(at + by) % order.len()];
                editor.typing = None;
            }
            InputEvent::Left { select } => nudge(editor, sprites, if select { -8 } else { -1 }),
            InputEvent::Right { select } => nudge(editor, sprites, if select { 8 } else { 1 }),
            InputEvent::Text(key) if editor.focus == Focus::Hex && key.is_ascii_hexdigit() => {
                let typing = editor.typing.get_or_insert_with(String::new);
                if typing.len() < 6 {
                    typing.push(key.to_ascii_lowercase());
                }
                if let Some(color) = parse_hex(typing) {
                    editor.set(sprites, color);
                    editor.typing = None;
                }
            }
            InputEvent::Backspace if editor.focus == Focus::Hex => {
                let mut text = editor.typing.take().unwrap_or_else(|| hex(editor.now));
                text.pop();
                editor.typing = Some(text);
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => match layout.hit(at) {
                MasterHit::Track(channel) => {
                    editor.focus = Focus::Channel(channel);
                    editor.typing = None;
                    editor.drag = Some(channel);
                    editor.set(
                        sprites,
                        with_channel(editor.now, channel, layout.value_at(channel, at)),
                    );
                }
                MasterHit::Hex => editor.focus = Focus::Hex,
                MasterHit::Cancel => {
                    editor.cancel(sprites);
                    return (Step::Close, None);
                }
                MasterHit::Apply => return (self.apply_master(sprites), None),
                MasterHit::Nothing => {}
            },
            InputEvent::PointerMove(Some(at)) => {
                if let Some(channel) = editor.drag {
                    editor.set(
                        sprites,
                        with_channel(editor.now, channel, layout.value_at(channel, at)),
                    );
                }
            }
            InputEvent::PointerUp { .. } | InputEvent::PointerCancel => editor.drag = None,
            _ => {}
        }
        (Step::Stay, None)
    }

    /// Makes the new color one undo step of its own. The slots window
    /// under this one keeps its changes pending in the same open edit, so
    /// they are taken out while the edit closes and written again after.
    fn apply_master(&mut self, sprites: &mut SpriteDocument) -> Step {
        let slots = self.edit.panels.iter().rev().find_map(|panel| match panel {
            Panel::Slots(editor) => Some(*editor),
            _ => None,
        });
        let pending = slots.map(|editor| editor.changes(sprites.bg().palettes()));
        if let Some(editor) = slots {
            editor.revert(sprites);
        }
        sprites.end_edit();
        sprites.begin_edit();
        if let (Some(editor), Some(pending)) = (slots, pending) {
            for (local, master) in pending {
                sprites.set_bg_color(editor.bank, local, master);
            }
        }
        Step::Close
    }

    pub(super) fn render_master(
        &self,
        ctx: &mut RenderContext<'_>,
        editor: &MasterEditor,
        _source: &SpriteSource<'_>,
        census: &Census<'_>,
        pointer: Option<Point>,
    ) {
        let layout = MasterLayout::new(self.metrics.top);
        let cell_w = self.metrics.cell_w;
        let hover = pointer.map_or(MasterHit::Nothing, |at| layout.hit(at));
        let inner = layout.window.inset(1);
        let name = master_name(editor.index);
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
        let title = ui_text::get("maps-master-title");
        let cart = ui_text::get("maps-master-cart");
        header(
            ctx,
            layout.window,
            &[(&title, PALE), (&name, WARN)],
            (&cart, DIM),
            cell_w,
        );

        let band = Rect {
            y: inner.y + 12,
            height: 35,
            ..inner
        };
        fill(ctx.framebuffer, band, BAR);
        let everywhere = ui_text::get("maps-master-everywhere");
        icon_line(
            ctx,
            (inner.x + 2, inner.y + 13),
            SystemIcon::Warning,
            &everywhere,
            (WARN, BAR),
        );
        let bg: Vec<String> = editor
            .bg
            .iter()
            .map(|(bank, local)| format!("BGP{} {local:02}", bank.get()))
            .collect();
        let obj: Vec<String> = editor
            .obj
            .iter()
            .map(|(bank, local)| format!("OBP{} {local:02}", bank.get()))
            .collect();
        let banks: Vec<BgBank> = editor.bg.iter().map(|&(bank, _)| bank).collect();
        let used = ui_text::get_with(
            "maps-master-used",
            &[
                ("maps", Arg::Number(census.banks(&banks).maps as f64)),
                ("sprites", Arg::Number(editor.sprites as f64)),
            ],
        );
        let columns = usize::try_from((inner.width - 19) / cell_w).unwrap_or(0);
        text(ctx, &listed("BG ", &bg, columns), 15, 23, PALE);
        let room = columns.saturating_sub(used.chars().count() + 2);
        text(ctx, &listed("OBJ", &obj, room), 15, 33, PALE);
        let x = inner.width - 4 - used.chars().count() as i32 * cell_w;
        text(ctx, &used, x, 33, DIM);

        for (channel, label) in ["R", "G", "B"].into_iter().enumerate() {
            let track = layout.tracks[channel];
            let focused = editor.focus == Focus::Channel(channel);
            let lit = focused || hover == MasterHit::Track(channel);
            let y = track.y - 4;
            draw_text(
                ctx.framebuffer,
                ctx.font,
                label,
                inner.x + 4,
                y,
                if lit { TEXT } else { DIM },
            );
            for step in 0..STEPS {
                let x = track.x + step as i32 * STEP_W;
                // Scratch entries end at the palette's end, so fit u8.
                let slot = (SCRATCH_BASE + channel * STEPS + step) as u8;
                fill_rect(ctx.framebuffer, x, track.y, STEP_W, track.height, slot);
            }
            outline(ctx.framebuffer, track, BAR);
            let value = channels(editor.now)[channel];
            let knob = Rect {
                x: track.x + i32::from(value) * (TRACK_W - KNOB_W) / 255,
                y: track.y - 2,
                width: KNOB_W,
                height: 10,
            };
            fill(ctx.framebuffer, knob.inset(-1), BACKGROUND);
            fill(ctx.framebuffer, knob, if focused { ACCENT } else { TEXT });
            let number = format!("{value:>3}");
            draw_text(
                ctx.framebuffer,
                ctx.font,
                &number,
                track.right() + 6,
                y,
                TEXT,
            );
        }

        text(ctx, &ui_text::get("maps-master-hex"), 4, 89, DIM);
        let field = layout.hex;
        let focused = editor.focus == Focus::Hex;
        fill(ctx.framebuffer, field, if focused { ACCENT } else { DIM });
        fill(ctx.framebuffer, field.inset(1), BACKGROUND);
        let shown = match &editor.typing {
            Some(typing) => format!("{typing}_"),
            None => hex(editor.now),
        };
        draw_text(
            ctx.framebuffer,
            ctx.font,
            &shown,
            field.x + 4,
            field.y,
            TEXT,
        );

        for (rect, slot, edge, id, ink) in [
            (layout.was, WAS_SLOT, BAR, "maps-master-was", DIM),
            (layout.now, NOW_SLOT, ACCENT, "maps-master-now", PALE),
        ] {
            fill(ctx.framebuffer, rect, edge);
            // Scratch entries end at the palette's end, so fit u8.
            fill(ctx.framebuffer, rect.inset(1), slot as u8);
            draw_text(
                ctx.framebuffer,
                ctx.font,
                &ui_text::get(id),
                rect.x,
                rect.bottom(),
                ink,
            );
        }
        text(ctx, &ui_text::get("maps-master-note-1"), 82, 89, DIM);
        text(ctx, &ui_text::get("maps-master-note-2"), 4, 102, DIM);
        let note = ui_text::get_with("maps-master-note-3", &[("master", Arg::Text(&name))]);
        text(ctx, &note, 4, 113, DIM);

        let footer = Rect {
            y: inner.y + 130,
            height: 18,
            ..inner
        };
        fill(ctx.framebuffer, footer, BAR);
        let cancel = ui_text::get("maps-master-cancel");
        button(
            ctx,
            layout.cancel,
            &cancel,
            Look::Plain,
            hover == MasterHit::Cancel,
            cell_w,
        );
        let apply = ui_text::get_with("maps-master-apply", &[("master", Arg::Text(&name))]);
        button(
            ctx,
            layout.apply,
            &apply,
            Look::Accent,
            hover == MasterHit::Apply,
            cell_w,
        );
        let key = ui_text::get("maps-master-undo-key");
        let x = layout.apply.right() + 8;
        draw_text(ctx.framebuffer, ctx.font, &key, x, layout.apply.y, PALE);
        let undo = ui_text::get("maps-master-undo");
        let x = x + (key.chars().count() as i32 + 1) * cell_w;
        draw_text(ctx.framebuffer, ctx.font, &undo, x, layout.apply.y, DIM);
    }
}

/// `kind` and as many whole `entries` as fit in `columns`, with `..` for
/// the ones left out.
fn listed(kind: &str, entries: &[String], columns: usize) -> String {
    let mut line = kind.to_owned();
    for (position, entry) in entries.iter().enumerate() {
        let more = if position + 1 < entries.len() { 4 } else { 0 };
        if line.chars().count() + 2 + entry.chars().count() + more > columns {
            line.push_str("  ..");
            break;
        }
        line.push_str("  ");
        line.push_str(entry);
    }
    line
}

#[cfg(test)]
mod tests;
