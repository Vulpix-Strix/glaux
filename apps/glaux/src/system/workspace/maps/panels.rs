//! The windows that edit tiles and palettes from the Map editor.

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::palette::Rgb;
use glaux_machine::system_rom::SystemIcon;
use glaux_video::draw::{draw_ink_image_centered, draw_text, fill_rect};
use std::ops::RangeInclusive;

use super::bank_slots::SlotEditor;
use super::frame::{Frame, Hit};
use super::master_color::MasterEditor;
use super::tile_editor::TileEditor;
use super::usage::Census;
use super::{MapEditor, MapOut};
use crate::system::screen::{ACCENT, BACKGROUND, DIM, PALE, TEXT};
use crate::system::sprite_document::SpriteDocument;
use crate::system::workspace::dialog::text_cells;
use crate::system::workspace::sprites::{SpriteSource, checker, outline};
use crate::system::workspace::{BAR, HOVER_SHADE, IDLE_SHADE, Rect, RenderContext};
use crate::ui_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Panel {
    Tile(TileEditor),
    Banks,
    Slots(SlotEditor),
    Master(MasterEditor),
}

impl Panel {
    /// Whether the window holds a change the user has not applied yet. It
    /// then takes every input, as a dialog does.
    fn is_provisional(&self) -> bool {
        matches!(self, Self::Slots(_) | Self::Master(_))
    }
}

pub(super) enum Step {
    Stay,
    Close,
    CloseAll,
    Open(Panel),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Look {
    Plain,
    Accent,
    Off,
}

pub(super) fn fill(framebuffer: &mut Framebuffer, rect: Rect, color: u8) {
    fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, color);
}

pub(super) fn window(framebuffer: &mut Framebuffer, rect: Rect) {
    let shadow = Rect {
        x: rect.x + 2,
        y: rect.y + 2,
        ..rect
    };
    fill(framebuffer, shadow, BAR);
    fill(framebuffer, rect, DIM);
    fill(framebuffer, rect.inset(1), BACKGROUND);
}

pub(super) fn header(
    ctx: &mut RenderContext<'_>,
    window: Rect,
    left: &[(&str, u8)],
    right: (&str, u8),
    cell_w: i32,
) {
    let inner = window.inset(1);
    let mut x = inner.x + 4;
    for (text, ink) in left {
        draw_text(ctx.framebuffer, ctx.font, text, x, inner.y, *ink);
        x += text_cells(text) * cell_w + 6;
    }
    let x = inner.right() - 4 - text_cells(right.0) * cell_w;
    draw_text(ctx.framebuffer, ctx.font, right.0, x, inner.y, right.1);
    fill_rect(ctx.framebuffer, inner.x, inner.y + 12, inner.width, 1, BAR);
}

pub(super) fn button(
    ctx: &mut RenderContext<'_>,
    rect: Rect,
    label: &str,
    look: Look,
    hovered: bool,
    cell_w: i32,
) {
    let (face, edge, ink) = match (look, hovered) {
        (Look::Off, _) => (BAR, BAR, IDLE_SHADE),
        (Look::Plain, false) => (BAR, DIM, PALE),
        (Look::Plain, true) => (HOVER_SHADE, DIM, TEXT),
        (Look::Accent, false) => (BAR, ACCENT, ACCENT),
        (Look::Accent, true) => (ACCENT, ACCENT, BACKGROUND),
    };
    fill(ctx.framebuffer, rect, edge);
    fill(ctx.framebuffer, rect.inset(1), face);
    let x = rect.x + (rect.width - text_cells(label) * cell_w) / 2;
    draw_text(ctx.framebuffer, ctx.font, label, x, rect.y, ink);
}

pub(super) fn focus_ring(framebuffer: &mut Framebuffer, rect: Rect) {
    outline(framebuffer, rect.inset(-2), ACCENT);
}

pub(super) fn swatch(framebuffer: &mut Framebuffer, rect: Rect, color: Option<u8>, edge: u8) {
    fill(framebuffer, rect, edge);
    match color {
        Some(color) => fill(framebuffer, rect.inset(1), color),
        None => checker(framebuffer, rect.inset(1), 2),
    }
}

pub(super) fn icon_line(
    ctx: &mut RenderContext<'_>,
    (x, y): (i32, i32),
    icon: SystemIcon,
    text: &str,
    (ink, ground): (u8, u8),
) {
    draw_ink_image_centered(
        ctx.framebuffer,
        ctx.ui.icon(icon),
        (x, y, 14, 12),
        &[ink, ink, ground],
    );
    draw_text(ctx.framebuffer, ctx.font, text, x + 16, y, ink);
}

/// A bar of hints, each a key and what it does, as `Esc close`.
pub(super) fn foot(ctx: &mut RenderContext<'_>, rect: Rect, ids: &[&str], cell_w: i32) {
    fill(ctx.framebuffer, rect, BAR);
    let mut x = rect.x + 4;
    for id in ids {
        let text = ui_text::get(id);
        let (key, rest) = text.split_once(' ').unwrap_or((&text, ""));
        draw_text(ctx.framebuffer, ctx.font, key, x, rect.y, PALE);
        let key_end = x + (text_cells(key) + 1) * cell_w;
        draw_text(ctx.framebuffer, ctx.font, rest, key_end, rect.y, DIM);
        x = key_end + text_cells(rest) * cell_w + 8;
    }
}

pub(super) fn master_name(master: u8) -> String {
    format!("M{master:03}")
}

fn primary_press(event: InputEvent) -> Option<Point> {
    match event {
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            at,
            ..
        } => Some(at),
        _ => None,
    }
}

impl MapEditor {
    pub(super) fn has_panel(&self) -> bool {
        !self.edit.panels.is_empty()
    }

    pub(super) fn has_provisional_panel(&self) -> bool {
        self.edit.panels.last().is_some_and(Panel::is_provisional)
    }

    pub(super) fn shows_tile_editor(&self) -> Option<&TileEditor> {
        self.edit.panels.iter().find_map(|panel| match panel {
            Panel::Tile(editor) => Some(editor),
            _ => None,
        })
    }

    pub(super) fn shows_palettes(&self) -> bool {
        self.edit
            .panels
            .iter()
            .any(|panel| !matches!(panel, Panel::Tile(_)))
    }

    /// The master colors the frame palette has to hold besides the
    /// background tiles' own, while the window on top lets them be picked.
    pub(in crate::system::workspace) fn shown_masters(
        &self,
        document: &SpriteDocument,
    ) -> Option<RangeInclusive<u8>> {
        match self.edit.panels.last()? {
            Panel::Slots(editor) => {
                let video = document.video();
                Some(self.master_range(&video.palette, video.bg.palettes(), editor.page))
            }
            _ => None,
        }
    }

    pub(in crate::system::workspace) fn scratch_colors(&self) -> Option<Vec<Rgb>> {
        match self.edit.panels.last()? {
            Panel::Master(editor) => Some(editor.scratch_colors()),
            _ => None,
        }
    }

    fn panel_window(&self, panel: &Panel) -> Rect {
        let top = self.metrics.top;
        match panel {
            Panel::Tile(_) => super::tile_editor::area(top),
            Panel::Banks => super::bank_picker::area(top),
            Panel::Slots(_) | Panel::Master(_) => super::bank_slots::area(top),
        }
    }

    pub(super) fn top_panel_window(&self) -> Option<Rect> {
        self.edit
            .panels
            .last()
            .map(|panel| self.panel_window(panel))
    }

    pub(super) fn graphics_undo(sprites: &mut SpriteDocument, redo: bool) -> Option<MapOut> {
        let (done, id) = if redo {
            (sprites.redo(), "sprites-nothing-to-redo")
        } else {
            (sprites.undo(), "sprites-nothing-to-undo")
        };
        (!done).then(|| MapOut::Notice(ui_text::get(id)))
    }

    /// `None` leaves the event to the editor under the windows.
    pub(super) fn panel_input(
        &mut self,
        event: InputEvent,
        frame: &Frame,
        sprites: Option<&mut SpriteDocument>,
    ) -> Option<Option<MapOut>> {
        if self.edit.panels.is_empty() {
            return None;
        }
        let Some(sprites) = sprites else {
            self.edit.panels.clear();
            return None;
        };
        if let InputEvent::PointerMove(at) = event {
            self.edit.pointer = at;
        }
        let mut panel = self.edit.panels.pop()?;
        if !panel.is_provisional() {
            let outside =
                primary_press(event).filter(|at| !self.panel_window(&panel).contains(*at));
            let history = match (event, outside.map(|at| frame.hit(at, self.edit.strip_top))) {
                (InputEvent::Undo, _) | (_, Some(Hit::Undo)) => Some(false),
                (InputEvent::Redo, _) | (_, Some(Hit::Redo)) => Some(true),
                _ => None,
            };
            if history.is_some() || outside.is_some() || event == InputEvent::Save {
                // An open drag ends first so that it counts as one edit.
                sprites.end_edit();
                if let Panel::Tile(editor) = &mut panel {
                    editor.drop_stroke();
                }
            }
            if let Some(redo) = history {
                self.edit.panels.push(panel);
                return Some(Self::graphics_undo(sprites, redo));
            }
            if outside.is_some() {
                self.edit.panels.clear();
                return Some(None);
            }
            if event == InputEvent::Save {
                self.edit.panels.push(panel);
                return None;
            }
        }
        let (step, out) = match &mut panel {
            Panel::Tile(editor) => self.tile_input(editor, event, sprites),
            Panel::Banks => self.banks_input(event, sprites),
            Panel::Slots(editor) => self.slots_input(editor, event, sprites),
            Panel::Master(editor) => self.master_input(editor, event, sprites),
        };
        match step {
            Step::Stay => self.edit.panels.push(panel),
            Step::Close => {}
            Step::CloseAll => self.edit.panels.clear(),
            Step::Open(next) => {
                self.edit.panels.push(panel);
                self.edit.panels.push(next);
            }
        }
        Some(out)
    }

    pub(super) fn render_panel(
        &self,
        ctx: &mut RenderContext<'_>,
        source: &SpriteSource<'_>,
        census: &Census<'_>,
        pointer: Option<Point>,
    ) {
        match self.edit.panels.last() {
            Some(Panel::Tile(editor)) => {
                self.render_tile_editor(ctx, editor, source, census, pointer)
            }
            Some(Panel::Banks) => self.render_banks(ctx, source, census, pointer),
            Some(Panel::Slots(editor)) => self.render_slots(ctx, editor, source, census, pointer),
            Some(Panel::Master(editor)) => self.render_master(ctx, editor, source, census, pointer),
            None => {}
        }
    }

    pub(super) fn panel_status(&self, source: Option<&SpriteSource<'_>>) -> Option<String> {
        Some(match self.edit.panels.last()? {
            Panel::Tile(editor) => self.tile_status(editor),
            Panel::Banks => format!("BGP{}", self.edit.brush.bank().get()),
            Panel::Slots(editor) => editor.status(source?),
            Panel::Master(editor) => editor.status(),
        })
    }

    pub(super) fn panel_hint(&self) -> Option<&'static str> {
        Some(match self.edit.panels.last()? {
            Panel::Banks => "maps-hint-banks",
            Panel::Tile(_) | Panel::Slots(_) | Panel::Master(_) => "",
        })
    }
}
