//! Drawing the Map view.

use glaux_input::Point;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::system_rom::SystemCursor;
use glaux_video::draw::{draw_text, fill_rect};

use super::editing::map_size;
use super::frame::{Frame, Hit as EditorHit};
use super::layout::{Hit, Layout};
use super::pattern::uses;
use super::{MapEditor, Overlay};
use crate::cartridge::rom::RomUsage;
use crate::system::map_document::MapDocument;
use crate::system::screen::{ACCENT, DIM, ERROR, PALE, TEXT};
use crate::system::workspace::context_menu::menu_hint;
use crate::system::workspace::dialog::{INK, text_cells};
use crate::system::workspace::rom_tag::{rom_tag, rom_tag_min_cells};
use crate::system::workspace::sprites::SpriteSource;
use crate::system::workspace::{BAR, HOVER_SHADE, Rect, take_cells};
use crate::system::workspace::{RenderContext, ViewInput};
use crate::ui_text;
use glaux_i18n::Arg;

fn fill(framebuffer: &mut Framebuffer, rect: Rect, color: u8) {
    fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, color);
}

impl MapEditor {
    /// `source` holds the background tiles and the palette they are drawn in.
    pub(in crate::system::workspace) fn render(
        &self,
        ctx: &mut RenderContext<'_>,
        source: Result<SpriteSource<'_>, &str>,
        document: Option<&MapDocument>,
        view: ViewInput<'_>,
        lit: bool,
    ) {
        let ViewInput { pointer, info } = view;
        let layout = self.layout();
        let frame = self.frame();
        let pointer = pointer.filter(|_| self.overlay.is_none());
        let hover = pointer.map_or(Hit::Nothing, |at| self.hit(&layout, at, document));
        let frame_hover = pointer.map_or(EditorHit::Nothing, |at| {
            self.frame_hit(&frame, at, document)
        });
        self.options_bar(ctx, &frame, document, document.is_some(), frame_hover);
        if let Some(document) = document {
            self.render_editor(ctx, source, document, pointer, frame_hover);
        } else {
            self.body(ctx, &layout, hover);
        }
        let left = document.map(|document| self.status_line(document, pointer));
        let hint = self.status_hint(document.is_some());
        self.status_bar(
            ctx,
            &frame,
            info.notice
                .or_else(|| left.as_deref().map(|text| (text, PALE))),
            &hint,
            info.rom,
        );
        self.overlay(ctx, lit);
    }

    /// What `at` falls on among the editor's parts. Without a map only the
    /// selector is live.
    fn frame_hit(&self, frame: &Frame, at: Point, document: Option<&MapDocument>) -> EditorHit {
        if document.is_none() {
            return if frame.selector.contains(at) {
                EditorHit::Selector
            } else {
                EditorHit::Nothing
            };
        }
        frame.hit(at, self.edit.strip_top)
    }

    fn hit(&self, layout: &Layout, at: Point, document: Option<&MapDocument>) -> Hit {
        layout.hit(at, self.create_label_cells(document))
    }

    /// The pointer to show at `at`.
    pub(in crate::system::workspace) fn cursor(
        &self,
        at: Point,
        document: Option<&MapDocument>,
    ) -> SystemCursor {
        if let Some(Overlay::Maps(menu)) = &self.overlay {
            let (cell_w, cell_h) = (self.metrics.cell_w, self.metrics.cell_h);
            let layout = menu.layout(&self.font, self.metrics.area(), cell_w, cell_h);
            return if menu.chooses_at(at, &layout) {
                SystemCursor::Hand
            } else {
                SystemCursor::Arrow
            };
        }
        if self.overlay.is_some() {
            return SystemCursor::Arrow;
        }
        let editor = self.frame_hit(&self.frame(), at, document);
        let over_editor = !matches!(editor, EditorHit::Nothing | EditorHit::Canvas);
        if over_editor || self.hit(&self.layout(), at, document) != Hit::Nothing {
            SystemCursor::Hand
        } else {
            SystemCursor::Arrow
        }
    }

    fn status_line(&self, document: &MapDocument, pointer: Option<Point>) -> String {
        match (&self.overlay, &self.edit.picker, &self.edit.attrs) {
            (Some(Overlay::TileMenu(tile, _)), ..) => {
                let layer = self.edit.layer;
                ui_text::get_with(
                    "maps-status-tile-used",
                    &[
                        ("tile", Arg::Text(&format!("{tile:04}"))),
                        ("count", Arg::Number(uses(document.layer(layer), *tile) as f64)),
                        ("layer", Arg::Text(&layer.to_string())),
                    ],
                )
            }
            (Some(Overlay::LayerMenu(layer, _)), ..) => {
                let name = document.meta().layer_names[layer.index()].as_deref();
                ui_text::get_with(
                    "maps-status-layer",
                    &[
                        ("layer", Arg::Text(&layer.to_string())),
                        ("name", Arg::Text(name.unwrap_or(""))),
                    ],
                )
                .trim_end()
                .to_owned()
            }
            (None, Some(picker), _) => self.picker_status(picker),
            (None, None, Some(_)) => self
                .attr_status()
                .unwrap_or_else(|| self.status_text(document, pointer)),
            _ => self.status_text(document, pointer),
        }
    }

    fn status_hint(&self, editing: bool) -> String {
        let id = match (&self.overlay, &self.edit.picker, &self.edit.attrs) {
            (Some(Overlay::Maps(_)), ..) => return menu_hint(),
            (Some(Overlay::LayerMenu(..)), ..) => "maps-hint-layer-menu",
            (None, Some(_), _) => "maps-hint-picker",
            (None, None, Some(_)) => "maps-hint-attr",
            _ if editing => "maps-hint-drawer",
            _ => return String::new(),
        };
        ui_text::get(id)
    }

    pub(super) fn status_text(&self, document: &MapDocument, pointer: Option<Point>) -> String {
        let frame = self.frame();
        let size = map_size(document);
        let view = self.viewport(&frame, document);
        let under = pointer
            .filter(|at| frame.canvas.contains(*at))
            .map(|at| view.cell_at(frame.canvas, (i32::from(at.x), i32::from(at.y))));
        if let Some(cell) = under.filter(|cell| super::canvas::in_map(*cell, size)) {
            let layer = self.edit.layer;
            // `in_map` holds, so both coordinates are not negative.
            let tile = document
                .layer(layer)
                .get(cell.0 as usize, cell.1 as usize)
                .map_or(0, |entry| entry.tile());
            return ui_text::get_with(
                "maps-status-cell",
                &[
                    ("x", Arg::Number(cell.0.into())),
                    ("y", Arg::Number(cell.1.into())),
                    ("layer", Arg::Text(&layer.to_string())),
                    ("tile", Arg::Text(&format!("{tile:04}"))),
                ],
            );
        }
        if let Some(selection) = self.edit.selection {
            return ui_text::get_with(
                "maps-status-selection",
                &[
                    ("width", Arg::Number(selection.width() as f64)),
                    ("height", Arg::Number(selection.height() as f64)),
                    ("x", Arg::Number(selection.left as f64)),
                    ("y", Arg::Number(selection.top as f64)),
                ],
            );
        }
        ui_text::get_with(
            "maps-info-size",
            &[
                ("width", Arg::Text(&size.0.to_string())),
                ("height", Arg::Text(&size.1.to_string())),
            ],
        )
    }

    fn body(&self, ctx: &mut RenderContext<'_>, layout: &Layout, hover: Hit) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let body = layout.body();
        let cell_h = self.metrics.cell_h;
        let y = body.y + cell_h;
        match &self.summaries {
            Some(Err(message)) => draw_text(framebuffer, font, message, body.x, y, ERROR),
            Some(Ok(maps)) if maps.is_empty() => {
                let text = ui_text::get("maps-no-maps");
                draw_text(framebuffer, font, &text, body.x, y, TEXT);
                let label = ui_text::get("maps-create-map");
                let button = layout.create_map(text_cells(&label));
                let hovered = hover == Hit::CreateMap;
                fill(framebuffer, button, if hovered { HOVER_SHADE } else { BAR });
                let x = button.x + (button.width - text_cells(&label) * self.metrics.cell_w) / 2;
                let text_y = button.y + (button.height - cell_h) / 2 + 1;
                draw_text(framebuffer, font, &label, x, text_y, ACCENT);
            }
            Some(Ok(_)) => {
                draw_text(
                    framebuffer,
                    font,
                    &ui_text::get("maps-select"),
                    body.x,
                    y,
                    DIM,
                );
            }
            None => {}
        }
    }

    /// The status bar: what the view reports on the left, then a hint when
    /// there is room, and Cart ROM on the right. Cart ROM always keeps room
    /// for its shortest form and takes a longer one with what is left.
    fn status_bar(
        &self,
        ctx: &mut RenderContext<'_>,
        frame: &Frame,
        left: Option<(&str, u8)>,
        hint: &str,
        rom: Option<&RomUsage>,
    ) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let bar = frame.status;
        let cell_w = self.metrics.cell_w;
        fill(framebuffer, bar, BAR);
        let margin = 4;
        let columns = usize::try_from((bar.width - 2 * margin) / cell_w).unwrap_or(0);
        let reserved = rom_tag_min_cells(rom, font);
        let gap = usize::from(reserved > 0);
        let room = columns.saturating_sub(reserved + gap);
        let mut taken = 0;
        if let Some((text, color)) = left {
            let shown = take_cells(font, text, room);
            taken = font.text_cells(shown);
            draw_text(framebuffer, font, shown, bar.x + margin, bar.y, color);
        }
        if !hint.is_empty() && taken + 2 + font.text_cells(hint) <= room {
            let x = bar.x + margin + (taken + 2) as i32 * cell_w;
            draw_text(framebuffer, font, hint, x, bar.y, DIM);
            taken += 2 + font.text_cells(hint);
        }
        let free = columns.saturating_sub(taken + gap);
        if let Some(tag) = rom_tag(rom, font, free) {
            let width = i32::try_from(font.text_cells(&tag.text)).unwrap_or(0) * cell_w;
            draw_text(
                framebuffer,
                font,
                &tag.text,
                bar.right() - margin - width,
                bar.y,
                tag.color,
            );
        }
    }

    fn overlay(&self, ctx: &mut RenderContext<'_>, lit: bool) {
        let area = self.metrics.area();
        let (cell_w, cell_h) = (self.metrics.cell_w, self.metrics.cell_h);
        match &self.overlay {
            Some(Overlay::Create(form)) => {
                let layout = form.layout(area, cell_w, cell_h);
                form.render(ctx, &layout, lit);
            }
            Some(Overlay::RenameLayer(_, prompt)) => {
                let layout = prompt.layout(area, cell_w, cell_h);
                prompt.render(ctx, &layout, ("", INK), lit);
            }
            Some(Overlay::LayerMenu(_, menu)) => {
                let layout = menu.layout(ctx.font, area, cell_w, cell_h);
                menu.render(ctx, &layout);
            }
            Some(Overlay::TileMenu(_, menu)) => {
                let layout = menu.layout(ctx.font, area, cell_w, cell_h);
                menu.render(ctx, &layout);
            }
            Some(Overlay::Confirm(_, dialog)) => {
                let layout = dialog.layout(ctx.font, area, cell_w, cell_h);
                dialog.render(ctx, &layout);
            }
            Some(Overlay::Maps(menu)) => {
                let layout = menu.layout(ctx.font, area, cell_w, cell_h);
                menu.render(ctx, &layout);
            }
            Some(Overlay::Rename(prompt)) => {
                let layout = prompt.layout(area, cell_w, cell_h);
                let message = if prompt.text().trim().is_empty() {
                    (
                        crate::system::ui_errors::map::map(
                            &crate::cartridge::map::MapError::BlankName,
                        ),
                        ERROR,
                    )
                } else {
                    (String::new(), INK)
                };
                prompt.render(ctx, &layout, (&message.0, message.1), lit);
            }
            None => {}
        }
    }
}
