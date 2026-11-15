//! The quick tile editor, which draws on a background tile over the map.

use glaux_i18n::Arg;
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::system_rom::SystemIcon;
use glaux_machine::tilemap::BgBank;
use glaux_video::draw::{draw_bg_tile_scaled, draw_ink_image_centered, draw_text, fill_rect};

use super::canvas::{PlateState, plate};
use super::editing::TileAction;
use super::panels::{
    Look, Panel, Step, button, fill, focus_ring, header, icon_line, swatch, window,
};
use super::tools::Tool;
use super::usage::{Census, Uses};
use super::{MapEditor, MapOut};
use crate::system::screen::{ACCENT, BACKGROUND, DIM, OK, PALE, TEXT, WARN};
use crate::system::sprite_document::{SpriteDocument, TileSet};
use crate::system::workspace::sprites::stroke::line;
use crate::system::workspace::sprites::{
    SpriteSource, ToolSlot, checker, format_hex, outline, use_tool,
};
use crate::system::workspace::{BAR, Rect, RenderContext};
use crate::ui_text;

const SCALE: i32 = 9;
const SIDE: i32 = 8;
const TOOLS: [ToolSlot; 4] = [
    ToolSlot::Pencil,
    ToolSlot::Eraser,
    ToolSlot::Fill,
    ToolSlot::Eyedropper,
];
/// The slots the brush's bank chip shows, which tell the banks apart.
pub(super) const CHIP_SLOTS: [u8; 4] = [3, 4, 5, 6];

pub(super) fn area(top: i32) -> Rect {
    Rect {
        x: 40,
        y: top + 18,
        width: 200,
        height: 124,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TileEditor {
    pub(super) tile: u16,
    pub(super) created: bool,
    tool: ToolSlot,
    color: u8,
    stroke: Option<(i32, i32)>,
    hover: Option<Point>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TileHit {
    Canvas,
    Swatch(u8),
    Tool(ToolSlot),
    Bank,
    Close,
    Place,
    Sprites,
    Done,
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TileEditorLayout {
    pub(super) window: Rect,
    bank: Rect,
    close: Rect,
    tools: [Rect; 4],
    pub(super) canvas: Rect,
    pub(super) swatches: [Rect; 16],
    small: Rect,
    large: Rect,
    foot: Rect,
    pub(super) place: Option<Rect>,
    pub(super) sprites: Rect,
    pub(super) done: Option<Rect>,
}

impl TileEditorLayout {
    pub(super) fn new(top: i32, created: bool) -> Self {
        let window = area(top);
        let (x, y) = (window.x + 1, window.y + 1);
        let at = |dx: i32, dy: i32, width: i32, height: i32| Rect {
            x: x + dx,
            y: y + dy,
            width,
            height,
        };
        let (place, sprites, done) = if created {
            (Some(at(4, 109, 82, 12)), at(92, 109, 100, 12), None)
        } else {
            (None, at(4, 109, 100, 12), Some(at(108, 109, 40, 12)))
        };
        Self {
            window,
            bank: at(141, 0, 44, 12),
            close: at(186, 0, 12, 12),
            tools: std::array::from_fn(|slot| at(4, 16 + slot as i32 * 19, 18, 18)),
            canvas: at(27, 17, SIDE * SCALE, SIDE * SCALE),
            swatches: std::array::from_fn(|local| {
                at(
                    106 + (local % 4) as i32 * 13,
                    16 + (local / 4) as i32 * 11,
                    12,
                    10,
                )
            }),
            small: at(162, 16, 8, 8),
            large: at(175, 16, 16, 16),
            foot: at(0, 108, 198, 14),
            place,
            sprites,
            done,
        }
    }

    /// The tile pixel under `at`, which may lie off the tile.
    fn pixel(&self, at: Point) -> (i32, i32) {
        (
            (i32::from(at.x) - self.canvas.x).div_euclid(SCALE),
            (i32::from(at.y) - self.canvas.y).div_euclid(SCALE),
        )
    }

    fn hit(&self, at: Point) -> TileHit {
        if self.canvas.contains(at) {
            return TileHit::Canvas;
        }
        if let Some(local) = self.swatches.iter().position(|rect| rect.contains(at)) {
            return TileHit::Swatch(local as u8);
        }
        if let Some(slot) = self.tools.iter().position(|rect| rect.contains(at)) {
            return TileHit::Tool(TOOLS[slot]);
        }
        for (rect, hit) in [
            (Some(self.bank), TileHit::Bank),
            (Some(self.close), TileHit::Close),
            (self.place, TileHit::Place),
            (Some(self.sprites), TileHit::Sprites),
            (self.done, TileHit::Done),
        ] {
            if rect.is_some_and(|rect| rect.contains(at)) {
                return hit;
            }
        }
        TileHit::Nothing
    }
}

impl TileEditor {
    pub(super) fn new(tile: u16, created: bool) -> Self {
        Self {
            tile,
            created,
            tool: ToolSlot::Pencil,
            color: 1,
            stroke: None,
            hover: None,
        }
    }

    /// Forgets the drag in progress. Its edit is closed by the caller.
    pub(super) fn drop_stroke(&mut self) {
        self.stroke = None;
    }

    fn end_stroke(&mut self, sprites: &mut SpriteDocument) {
        self.stroke = None;
        sprites.end_edit();
    }

    fn apply(&mut self, sprites: &mut SpriteDocument, (x, y): (i32, i32)) {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return;
        };
        let target = (TileSet::Bg, usize::from(self.tile));
        if let Some(picked) = use_tool(sprites, target, self.tool, self.color, (x, y)) {
            self.color = picked;
        }
    }

    fn drag(&mut self, sprites: &mut SpriteDocument, to: (i32, i32)) {
        let Some(last) = self.stroke else {
            return;
        };
        for pixel in line(last, to).skip(1) {
            self.apply(sprites, pixel);
        }
        self.stroke = Some(to);
    }
}

pub(super) fn uses_text(id: &str, uses: Uses) -> String {
    ui_text::get_with(
        id,
        &[
            ("uses", Arg::Number(uses.cells as f64)),
            ("maps", Arg::Number(uses.maps as f64)),
        ],
    )
}

impl MapEditor {
    pub(super) fn tile_input(
        &mut self,
        editor: &mut TileEditor,
        event: InputEvent,
        sprites: &mut SpriteDocument,
    ) -> (Step, Option<MapOut>) {
        let layout = TileEditorLayout::new(self.metrics.top, editor.created);
        match event {
            InputEvent::Break | InputEvent::Text('d' | 'D') => {
                editor.end_stroke(sprites);
                return (Step::Close, None);
            }
            InputEvent::Enter => {
                editor.end_stroke(sprites);
                return self.finish_tile(editor);
            }
            InputEvent::Text(key) => {
                if let Some(tool) = ToolSlot::from_key(key) {
                    editor.end_stroke(sprites);
                    editor.tool = tool;
                }
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                // A press while a drag is open means its release was lost.
                editor.end_stroke(sprites);
                match layout.hit(at) {
                    TileHit::Canvas => {
                        if matches!(editor.tool, ToolSlot::Pencil | ToolSlot::Eraser) {
                            sprites.begin_edit();
                        }
                        let pixel = layout.pixel(at);
                        editor.apply(sprites, pixel);
                        if editor.tool.drags() {
                            editor.stroke = Some(pixel);
                        }
                    }
                    TileHit::Swatch(local) if local != 0 => editor.color = local,
                    TileHit::Tool(tool) => editor.tool = tool,
                    TileHit::Bank => return (Step::Open(Panel::Banks), None),
                    TileHit::Close | TileHit::Done => return (Step::Close, None),
                    TileHit::Place => return self.finish_tile(editor),
                    TileHit::Sprites => {
                        let out =
                            self.run_tile_action(editor.tile, TileAction::EditInSprites, None);
                        return (Step::CloseAll, out);
                    }
                    TileHit::Swatch(_) | TileHit::Nothing => {}
                }
            }
            InputEvent::PointerMove(at) => {
                editor.hover = at;
                if let Some(at) = at {
                    editor.drag(sprites, layout.pixel(at));
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                at,
            } => {
                editor.drag(sprites, layout.pixel(at));
                editor.end_stroke(sprites);
            }
            InputEvent::PointerCancel => editor.end_stroke(sprites),
            _ => {}
        }
        (Step::Stay, None)
    }

    fn finish_tile(&mut self, editor: &TileEditor) -> (Step, Option<MapOut>) {
        if editor.created {
            self.edit.tool = Tool::Pencil;
        }
        (Step::Close, None)
    }

    pub(super) fn tile_status(&self, editor: &TileEditor) -> String {
        let layout = TileEditorLayout::new(self.metrics.top, editor.created);
        let id = if editor.created {
            "maps-status-new"
        } else {
            "maps-status-edit"
        };
        let mut parts = vec![ui_text::get_with(
            id,
            &[("tile", Arg::Text(&format!("{:04}", editor.tile)))],
        )];
        if let Some(at) = editor.hover.filter(|at| layout.canvas.contains(*at)) {
            let (x, y) = layout.pixel(at);
            parts.push(ui_text::get_with(
                "maps-status-pixel",
                &[("x", Arg::Number(x.into())), ("y", Arg::Number(y.into()))],
            ));
        }
        parts.push(color_text(editor.color));
        parts.join("  ")
    }

    pub(super) fn render_tile_editor(
        &self,
        ctx: &mut RenderContext<'_>,
        editor: &TileEditor,
        source: &SpriteSource<'_>,
        census: &Census<'_>,
        pointer: Option<Point>,
    ) {
        let layout = TileEditorLayout::new(self.metrics.top, editor.created);
        let cell_w = self.metrics.cell_w;
        let hover = pointer.map_or(TileHit::Nothing, |at| layout.hit(at));
        let bank = self.edit.brush.bank();
        let tile = usize::from(editor.tile);
        let slots = source.preview.slots();
        window(ctx.framebuffer, layout.window);
        let title = ui_text::get(if editor.created {
            "maps-tile-new-title"
        } else {
            "maps-tile-edit-title"
        });
        let number = format!("{:04}", editor.tile);
        header(
            ctx,
            layout.window,
            &[(&title, PALE), (&number, TEXT)],
            ("", DIM),
            cell_w,
        );
        let ink = if hover == TileHit::Bank { TEXT } else { PALE };
        bank_chip(
            ctx,
            source,
            (layout.bank.x + 3, layout.bank.y),
            bank,
            ink,
            cell_w,
        );
        let ink = if hover == TileHit::Close { TEXT } else { DIM };
        let close = layout.close;
        draw_ink_image_centered(
            ctx.framebuffer,
            ctx.ui.icon(SystemIcon::Close),
            (close.x, close.y, close.width, close.height),
            &[ink, ink, BACKGROUND],
        );

        for (tool, rect) in TOOLS.into_iter().zip(layout.tools) {
            let icon = match tool {
                ToolSlot::Pencil => SystemIcon::Sprites,
                ToolSlot::Eraser => SystemIcon::Eraser,
                ToolSlot::Fill => SystemIcon::Fill,
                ToolSlot::Eyedropper => SystemIcon::Eyedropper,
            };
            let state = if tool == editor.tool {
                PlateState::Active
            } else if hover == TileHit::Tool(tool) {
                PlateState::Hover
            } else {
                PlateState::Idle
            };
            plate(ctx, rect, icon, state);
        }

        let canvas = layout.canvas;
        outline(ctx.framebuffer, canvas.inset(-1), DIM);
        checker(ctx.framebuffer, canvas, SCALE);
        draw_bg_tile_scaled(
            ctx.framebuffer,
            source.bg,
            tile,
            bank,
            (canvas.x, canvas.y),
            SCALE,
            slots,
        );
        for step in 1..SIDE {
            let offset = step * SCALE;
            fill_rect(
                ctx.framebuffer,
                canvas.x + offset,
                canvas.y,
                1,
                canvas.height,
                BACKGROUND,
            );
            fill_rect(
                ctx.framebuffer,
                canvas.x,
                canvas.y + offset,
                canvas.width,
                1,
                BACKGROUND,
            );
        }
        if let Some(at) = pointer.filter(|_| hover == TileHit::Canvas) {
            let (x, y) = layout.pixel(at);
            let pixel = Rect {
                x: canvas.x + x * SCALE,
                y: canvas.y + y * SCALE,
                width: SCALE,
                height: SCALE,
            };
            outline(ctx.framebuffer, pixel, ACCENT);
        }

        for (local, rect) in layout.swatches.into_iter().enumerate() {
            let local = local as u8;
            let edge = if local == editor.color {
                ACCENT
            } else if hover == TileHit::Swatch(local) && local != 0 {
                PALE
            } else {
                BAR
            };
            let color = source.bg.palettes().resolve(bank, local);
            swatch(
                ctx.framebuffer,
                rect,
                color.map(|master| slots[usize::from(master)]),
                edge,
            );
        }
        for (rect, scale, label) in [(layout.small, 1, "1x"), (layout.large, 2, "2x")] {
            checker(ctx.framebuffer, rect, 2 * scale);
            draw_bg_tile_scaled(
                ctx.framebuffer,
                source.bg,
                tile,
                bank,
                (rect.x, rect.y),
                scale,
                slots,
            );
            let x = rect.x + (rect.width - 2 * cell_w) / 2;
            draw_text(
                ctx.framebuffer,
                ctx.font,
                label,
                x,
                layout.large.bottom(),
                DIM,
            );
        }
        let text_x = layout.swatches[0].x;
        let below = layout.swatches[15].bottom();
        draw_text(
            ctx.framebuffer,
            ctx.font,
            &color_text(editor.color),
            text_x,
            below + 2,
            PALE,
        );
        if let Some(master) = source.bg.palettes().resolve(bank, editor.color) {
            let hex = format_hex(source.master.get(master));
            draw_text(ctx.framebuffer, ctx.font, &hex, text_x, below + 13, DIM);
        }

        let note = (layout.window.x + 3, layout.foot.y - 14);
        let uses = census.tile(editor.tile);
        if editor.created {
            let text = ui_text::get_with("maps-tile-created", &[("tile", Arg::Text(&number))]);
            icon_line(ctx, note, SystemIcon::Check, &text, (OK, BACKGROUND));
        } else if uses.cells > 0 {
            let text = uses_text("maps-tile-uses", uses);
            icon_line(ctx, note, SystemIcon::Warning, &text, (WARN, BACKGROUND));
        } else {
            let text = ui_text::get("maps-tile-unused");
            draw_text(ctx.framebuffer, ctx.font, &text, note.0 + 2, note.1, DIM);
        }

        fill(ctx.framebuffer, layout.foot, BAR);
        let sprites = ui_text::get("maps-tile-menu-edit");
        button(
            ctx,
            layout.sprites,
            &sprites,
            Look::Plain,
            hover == TileHit::Sprites,
            cell_w,
        );
        if let Some(rect) = layout.place {
            let label = ui_text::get("maps-tile-place");
            button(
                ctx,
                rect,
                &label,
                Look::Accent,
                hover == TileHit::Place,
                cell_w,
            );
            focus_ring(ctx.framebuffer, rect);
        }
        if let Some(rect) = layout.done {
            let label = ui_text::get("maps-tile-done");
            button(
                ctx,
                rect,
                &label,
                Look::Plain,
                hover == TileHit::Done,
                cell_w,
            );
            draw_text(
                ctx.framebuffer,
                ctx.font,
                "D",
                rect.right() + 6,
                rect.y,
                DIM,
            );
        }
    }
}

fn color_text(color: u8) -> String {
    ui_text::get_with(
        "maps-tile-color",
        &[("color", Arg::Text(&format!("{color:02}")))],
    )
}

/// Returns where the chip ends.
pub(super) fn bank_chip(
    ctx: &mut RenderContext<'_>,
    source: &SpriteSource<'_>,
    (x, y): (i32, i32),
    bank: BgBank,
    ink: u8,
    cell_w: i32,
) -> i32 {
    let name = format!("BGP{}", bank.get());
    draw_text(ctx.framebuffer, ctx.font, &name, x, y, ink);
    let mut x = x + 4 * cell_w + 3;
    for local in CHIP_SLOTS {
        if let Some(master) = source.bg.palettes().resolve(bank, local) {
            let color = source.preview.slots()[usize::from(master)];
            fill_rect(ctx.framebuffer, x, y + 3, 2, 6, color);
        }
        x += 2;
    }
    x
}

#[cfg(test)]
pub(super) fn pixel_middle(top: i32, x: i32, y: i32) -> Point {
    let canvas = TileEditorLayout::new(top, false).canvas;
    let middle = |pixel: i32| pixel * SCALE + SCALE / 2;
    Point::new((canvas.x + middle(x)) as u16, (canvas.y + middle(y)) as u16)
}

#[cfg(test)]
mod tests;
