//! Drawing the Map view.

use glaux_input::Point;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::system_rom::SystemCursor;
use glaux_video::draw::{draw_text, fill_rect};

use super::editing::map_size;
use super::frame::Hit as EditorHit;
use super::layout::{Action, Hit, Layout, first_visible};
use super::{MapEditor, Overlay};
use crate::cartridge::map::MapSummary;
use crate::cartridge::rom::RomUsage;
use crate::system::map_document::MapDocument;
use crate::system::screen::{ACCENT, BACKGROUND, DIM, ERROR, PALE, TEXT};
use crate::system::workspace::dialog::{INK, text_cells};
use crate::system::workspace::rom_tag::rom_tag;
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
        let hover = pointer
            .filter(|_| self.overlay.is_none())
            .map_or(Hit::Nothing, |at| self.hit(&layout, at, document));
        self.header(ctx, &layout, document, hover);
        let editing = document.filter(|_| !self.list_open);
        if self.list_open {
            self.list(ctx, &layout, document, hover);
        } else if let Some(document) = editing {
            self.render_editor(ctx, source, document, pointer);
        } else {
            self.body(ctx, &layout, hover);
        }
        let status = editing.map(|document| self.status_text(document, pointer));
        let left = info
            .notice
            .or_else(|| status.as_deref().map(|text| (text, PALE)));
        self.status_bar(ctx, &layout, left, info.rom);
        self.overlay(ctx, lit);
    }

    fn hit(&self, layout: &Layout, at: Point, document: Option<&MapDocument>) -> Hit {
        let list = self.list_open.then(|| {
            let first = first_visible(self.highlighted, layout.visible_rows());
            (self.maps().len(), first)
        });
        layout.hit(at, list, self.create_label_cells(document))
    }

    /// The pointer to show at `at`.
    pub(in crate::system::workspace) fn cursor(
        &self,
        at: Point,
        document: Option<&MapDocument>,
    ) -> SystemCursor {
        if self.overlay.is_some() {
            return SystemCursor::Arrow;
        }
        if document.is_some()
            && !self.list_open
            && !matches!(
                self.frame().hit(at, self.edit.strip_top),
                EditorHit::Nothing | EditorHit::Canvas
            )
        {
            return SystemCursor::Hand;
        }
        match self.hit(&self.layout(), at, document) {
            Hit::Nothing => SystemCursor::Arrow,
            _ => SystemCursor::Hand,
        }
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

    fn header(
        &self,
        ctx: &mut RenderContext<'_>,
        layout: &Layout,
        document: Option<&MapDocument>,
        hover: Hit,
    ) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let selector = layout.selector();
        let lit = matches!(hover, Hit::Selector) || self.list_open;
        fill(framebuffer, selector, if lit { HOVER_SHADE } else { BAR });
        let columns = usize::try_from(selector.width / self.metrics.cell_w - 4).unwrap_or(0);
        let (label, color) = match document {
            Some(document) => (document.meta().name.clone(), TEXT),
            None => (ui_text::get("maps-select"), DIM),
        };
        let text_y = selector.y + (selector.height - self.metrics.cell_h) / 2 + 1;
        draw_text(
            framebuffer,
            font,
            take_cells(font, &label, columns),
            selector.x + self.metrics.cell_w,
            text_y,
            color,
        );
        draw_text(
            framebuffer,
            font,
            if self.list_open { "^" } else { "v" },
            selector.right() - 2 * self.metrics.cell_w,
            text_y,
            PALE,
        );
        for action in Action::ALL {
            let rect = layout.button(action);
            let enabled = match action {
                Action::New => true,
                Action::Rename | Action::Delete => document.is_some(),
                Action::Save => document.is_some_and(MapDocument::is_modified),
            };
            let hovered = enabled && hover == Hit::Action(action);
            fill(framebuffer, rect, if hovered { HOVER_SHADE } else { BAR });
            let label = ui_text::get(action.message_id());
            let ink = if !enabled {
                DIM
            } else if action == Action::Save {
                ACCENT
            } else {
                TEXT
            };
            let x = rect.x + (rect.width - text_cells(&label) * self.metrics.cell_w) / 2;
            let y = rect.y + (rect.height - self.metrics.cell_h) / 2 + 1;
            draw_text(framebuffer, font, &label, x, y, ink);
        }
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

    fn list(
        &self,
        ctx: &mut RenderContext<'_>,
        layout: &Layout,
        document: Option<&MapDocument>,
        hover: Hit,
    ) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let maps = self.maps();
        let first = first_visible(self.highlighted, layout.visible_rows());
        let shown = maps.len().saturating_sub(first).min(layout.visible_rows());
        for slot in 0..shown {
            let index = first + slot;
            let rect = layout.row(slot);
            let active = index == self.highlighted || hover == Hit::Row(index);
            fill(
                framebuffer,
                rect,
                if active { HOVER_SHADE } else { BACKGROUND },
            );
            let columns = usize::try_from(rect.width / self.metrics.cell_w - 2).unwrap_or(0);
            let (line, color) = row_text(&maps[index], document);
            draw_text(
                framebuffer,
                font,
                take_cells(font, &line, columns),
                rect.x + self.metrics.cell_w,
                rect.y + 2,
                color,
            );
        }
    }

    /// The status bar: the notice on the left and, in the room it leaves,
    /// Cart ROM on the right.
    fn status_bar(
        &self,
        ctx: &mut RenderContext<'_>,
        layout: &Layout,
        notice: Option<(&str, u8)>,
        rom: Option<&RomUsage>,
    ) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let bar = layout.status();
        let cell_w = self.metrics.cell_w;
        fill(framebuffer, bar, BAR);
        let columns = usize::try_from(bar.width / cell_w - 2).unwrap_or(0);
        let mut taken = 0;
        if let Some((text, color)) = notice {
            let shown = take_cells(font, text, columns);
            taken = font.text_cells(shown);
            draw_text(framebuffer, font, shown, cell_w, bar.y + 2, color);
        }
        let free = columns.saturating_sub(taken + 1);
        if let Some(tag) = rom_tag(rom, font, free) {
            let width = i32::try_from(font.text_cells(&tag.text)).unwrap_or(0) * cell_w;
            draw_text(
                framebuffer,
                font,
                &tag.text,
                bar.right() - cell_w - width,
                bar.y + 2,
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

/// The text of a list row. A map that cannot be opened shows its id and says so.
fn row_text(summary: &MapSummary, document: Option<&MapDocument>) -> (String, u8) {
    let open = document.is_some_and(|document| document.id() == &summary.id);
    match &summary.meta {
        Ok(meta) => (
            format!(
                "{}  {}  {}x{}",
                meta.name, summary.id, meta.width, meta.height
            ),
            if open { ACCENT } else { TEXT },
        ),
        Err(_) => (
            format!("{}  ({})", summary.id, ui_text::get("maps-broken")),
            ERROR,
        ),
    }
}
