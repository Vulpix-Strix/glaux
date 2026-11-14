//! Drawing the Map view.

use glaux_input::Point;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::system_rom::SystemCursor;
use glaux_video::draw::{draw_text, draw_text_scaled, fill_rect};

use super::layout::{Action, Hit, Layout, first_visible};
use super::{MapEditor, Overlay};
use crate::cartridge::map::{MapMeta, MapSummary};
use crate::cartridge::rom::RomUsage;
use crate::system::map_document::MapDocument;
use crate::system::screen::{ACCENT, BACKGROUND, DIM, ERROR, PALE, TEXT};
use crate::system::workspace::dialog::{INK, text_cells};
use crate::system::workspace::rom_tag::rom_tag;
use crate::system::workspace::{BAR, HOVER_SHADE, Rect, take_cells};
use crate::system::workspace::{RenderContext, ViewInput};
use crate::ui_text;
use glaux_i18n::Arg;
use glaux_machine::tilemap::BgLayer;

fn fill(framebuffer: &mut Framebuffer, rect: Rect, color: u8) {
    fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, color);
}

impl MapEditor {
    /// Draws the view below the tab bar for the open `document`. `pointer`
    /// highlights what it hovers, `notice` takes the status bar and `lit`
    /// is the phase of the text carets.
    pub(in crate::system::workspace) fn render(
        &self,
        ctx: &mut RenderContext<'_>,
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
        // The open list takes the place of the body.
        if self.list_open {
            self.list(ctx, &layout, document, hover);
        } else {
            self.body(ctx, &layout, document, hover);
        }
        self.status_bar(ctx, &layout, info.notice, info.rom);
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
        match self.hit(&self.layout(), at, document) {
            Hit::Nothing => SystemCursor::Arrow,
            _ => SystemCursor::Hand,
        }
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

    fn body(
        &self,
        ctx: &mut RenderContext<'_>,
        layout: &Layout,
        document: Option<&MapDocument>,
        hover: Hit,
    ) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let body = layout.body();
        let cell_h = self.metrics.cell_h;
        let y = body.y + cell_h;
        if let Some(document) = document {
            self.info(ctx, body, y, document);
            return;
        }
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

    /// The open map's name, size and layers.
    fn info(&self, ctx: &mut RenderContext<'_>, body: Rect, top: i32, document: &MapDocument) {
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let cell_h = self.metrics.cell_h;
        let meta: &MapMeta = document.meta();
        let mut y = top;
        draw_text_scaled(framebuffer, font, &meta.name, body.x, y, 2, ACCENT);
        y += 3 * cell_h;
        let size = ui_text::get_with(
            "maps-info-size",
            &[
                ("width", Arg::Text(&meta.width.to_string())),
                ("height", Arg::Text(&meta.height.to_string())),
            ],
        );
        draw_text(framebuffer, font, &size, body.x, y, TEXT);
        y += 2 * cell_h;
        for layer in BgLayer::ALL {
            let line = match &meta.layer_names[layer.index()] {
                Some(name) => ui_text::get_with(
                    "maps-info-layer",
                    &[
                        ("layer", Arg::Text(&layer.to_string())),
                        ("name", Arg::Text(name)),
                    ],
                ),
                None => ui_text::get_with(
                    "maps-info-layer-unnamed",
                    &[("layer", Arg::Text(&layer.to_string()))],
                ),
            };
            draw_text(framebuffer, font, &line, body.x, y, PALE);
            y += cell_h;
        }
        if document.is_modified() {
            y += cell_h;
            draw_text(
                framebuffer,
                font,
                &ui_text::get("maps-info-unsaved"),
                body.x,
                y,
                ACCENT,
            );
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
