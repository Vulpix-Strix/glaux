//! Drawing the browser.

use glaux_i18n::Arg;
use glaux_input::Point;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{draw_text, fill_rect, pixel};

use super::layout::{Deco, Row, Run};
use super::search::{Scope, Search, SearchLayout};
use super::tabs::{CHEVRON_WIDTH, CLOSE_SIZE, LABEL_PAD, MARKER, StripHit};
use super::view::{Geometry, nav_label, search_label};
use super::{DocsBrowser, PanelKind, Source};
use crate::system::workspace::code::text_view::thumb;
use crate::system::workspace::dialog::{BODY, INK, draw_window};
use crate::system::workspace::text_field::FieldColors;
use crate::system::workspace::{BAR, ELLIPSIS, HOVER_SHADE, IDLE_SHADE, Rect, take_cells};
use crate::system::{ACCENT, BACKGROUND, DIM, ERROR, PALE, TEXT};
use crate::ui_text;

/// Thickness of the box around a code block or a table, in pixels from the
/// edge of the cells it frames.
const FRAME_INSET: i32 = 2;

/// Where the drawing goes, with the font that measures the text.
struct Canvas<'a> {
    framebuffer: &'a mut Framebuffer,
    font: &'a BitmapFont,
    cell_w: i32,
    cell_h: i32,
}

impl Canvas<'_> {
    fn fill(&mut self, rect: Rect, color: u8) {
        fill_rect(
            self.framebuffer,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            color,
        );
    }

    fn text(&mut self, text: &str, x: i32, y: i32, color: u8) {
        draw_text(self.framebuffer, self.font, text, x, y, color);
    }

    fn hline(&mut self, x: i32, y: i32, width: i32, color: u8) {
        fill_rect(self.framebuffer, x, y, width, 1, color);
    }

    fn vline(&mut self, x: i32, y: i32, height: i32, color: u8) {
        fill_rect(self.framebuffer, x, y, 1, height, color);
    }

    fn cells(&self, text: &str) -> i32 {
        i32::try_from(self.font.text_cells(text)).unwrap_or(i32::MAX)
    }

    /// Top of a text row centred in a bar that starts at `y`.
    fn bar_text_y(&self, y: i32, height: i32) -> i32 {
        y + (height - self.cell_h) / 2 + 1
    }
}

impl DocsBrowser {
    /// Draws the browser over the view area. `pointer` highlights what is
    /// under it, and `notice` replaces the status bar's text.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        source: Source<'_>,
        pointer: Option<Point>,
        notice: Option<(&str, u8)>,
    ) {
        let metrics = self.geometry.metrics;
        let mut canvas = Canvas {
            framebuffer,
            font,
            cell_w: metrics.cell_w,
            cell_h: metrics.cell_h,
        };
        // Nothing under an open window reacts to the pointer.
        let covered = self.has_overlay();
        let page_pointer = pointer.filter(|_| !covered);
        self.strip(&mut canvas, source, page_pointer);
        self.options(&mut canvas, page_pointer);
        self.panel(&mut canvas, page_pointer);
        self.page(&mut canvas, page_pointer);
        self.status(&mut canvas, page_pointer, notice);
        if let Some(menu) = self.menu {
            self.menu_window(&mut canvas, source, menu.focus, pointer);
        }
        if let Some(search) = &self.search {
            self.search_window(&mut canvas, search, pointer);
        }
    }

    fn strip(&self, canvas: &mut Canvas<'_>, source: Source<'_>, pointer: Option<Point>) {
        let geometry = &self.geometry;
        let strip = geometry.strip;
        canvas.fill(strip, BACKGROUND);
        canvas.hline(strip.x, strip.bottom() - 1, strip.width, BAR);
        let labels = self.labels(source);
        if labels.is_empty() {
            return;
        }
        let hit = pointer.map_or(StripHit::None, |at| {
            geometry.strip_hit(canvas.font, &labels, self.strip_scroll, at)
        });
        let tabs = geometry.strip_tabs(canvas.font, &labels, self.strip_scroll);
        for tab in &tabs {
            let active = tab.index == self.active;
            let hovered = matches!(hit, StripHit::Tab(i) | StripHit::Close(i) if i == tab.index);
            canvas.fill(tab.rect, BAR);
            let ink = match (active, hovered) {
                (true, _) => TEXT,
                (false, true) => PALE,
                (false, false) => DIM,
            };
            let y = tab.rect.y + 1;
            let marker_ink = if active { ACCENT } else { IDLE_SHADE };
            canvas.text(MARKER, tab.rect.x + LABEL_PAD, y, marker_ink);
            canvas.text(
                &labels[tab.index],
                tab.rect.x + LABEL_PAD + 2 * canvas.cell_w,
                y,
                ink,
            );
            let close_ink = match (hit, active || hovered) {
                (StripHit::Close(i), _) if i == tab.index => ERROR,
                (_, true) => DIM,
                (_, false) => IDLE_SHADE,
            };
            close_glyph(canvas, tab.close, close_ink);
            if active {
                canvas.hline(tab.rect.x, tab.rect.bottom() - 1, tab.rect.width, ACCENT);
            }
        }
        let more = geometry.more_button();
        let open = self.menu.is_some();
        let hovered = hit == StripHit::More;
        if open || hovered {
            canvas.fill(more, BAR);
        }
        let ink = if open || hovered { PALE } else { DIM };
        let chevron_x = more.right() - 2 - CHEVRON_WIDTH;
        let hidden = labels.len() - tabs.len();
        if hidden > 0 {
            let count = format!("+{hidden}");
            let x = chevron_x - 2 - canvas.cells(&count) * canvas.cell_w;
            canvas.text(&count, x, more.y + 1, ink);
        }
        for row in 0..3 {
            let width = CHEVRON_WIDTH - 2 * row;
            let y = more.y + (more.height - 3) / 2 + row;
            canvas.hline(chevron_x + row, y, width, ink);
        }
    }

    fn options(&self, canvas: &mut Canvas<'_>, pointer: Option<Point>) {
        let bar = self.geometry.options;
        let items = self.options_bar();
        canvas.hline(bar.x, bar.bottom() - 1, bar.width, BAR);
        let y = canvas.bar_text_y(bar.y, bar.height - 1);
        let hover = |rect: Rect| pointer.is_some_and(|at| rect.contains(at));
        let pad = Geometry::button_pad();
        let button = |canvas: &mut Canvas<'_>, rect: Rect, label: &str, lit: bool, live: bool| {
            let ground = match (live && hover(rect), lit) {
                (true, _) => Some(HOVER_SHADE),
                (false, true) => Some(BAR),
                (false, false) => None,
            };
            if let Some(ground) = ground {
                canvas.fill(rect, ground);
            }
            let ink = if !live {
                IDLE_SHADE
            } else if lit {
                TEXT
            } else {
                PALE
            };
            canvas.text(label, rect.x + pad, y, ink);
        };
        button(canvas, items.back, "<", true, self.can_go(true));
        button(canvas, items.forward, ">", true, self.can_go(false));
        button(canvas, items.filter, &self.filter.label(), true, true);
        button(canvas, items.nav, &nav_label(), !self.nav_collapsed, true);
        button(
            canvas,
            items.search,
            &search_label(),
            self.search.is_some(),
            true,
        );

        let Some(shown) = &self.shown else {
            return;
        };
        let columns = usize::try_from(items.crumb.width / canvas.cell_w.max(1)).unwrap_or(0);
        let title_cells = canvas.font.text_cells(&shown.title);
        let crumb_room = columns.saturating_sub(title_cells);
        let crumb = shorten_start(canvas.font, &shown.crumb, crumb_room);
        canvas.text(&crumb, items.crumb.x, y, DIM);
        let title_x = items.crumb.x + canvas.cells(&crumb) * canvas.cell_w;
        let title_room = columns.saturating_sub(canvas.font.text_cells(&crumb));
        let title = take_cells(canvas.font, &shown.title, title_room);
        canvas.text(title, title_x, y, TEXT);
    }

    fn panel(&self, canvas: &mut Canvas<'_>, pointer: Option<Point>) {
        let Some(rows) = self.panel_rows() else {
            return;
        };
        let Some(nav) = self.geometry.nav(self.nav_collapsed) else {
            return;
        };
        canvas.vline(nav.right() - 1, nav.y, nav.height, BAR);
        let text_x = nav.x + self.geometry.nav_text_x();
        let columns = self.geometry.nav_cols();
        for row in rows {
            let hovered = pointer.is_some_and(|at| row.rect.contains(at));
            let y = row.rect.y + 1;
            match row.kind {
                PanelKind::Title(title) => canvas.text(&title, text_x, y, DIM),
                PanelKind::Outline {
                    row: target,
                    active,
                } => {
                    let title = self.outline_title(target);
                    let label = clip(canvas.font, &title, columns - 2);
                    if active {
                        canvas.fill(row.rect, BAR);
                        canvas.text(">", text_x, y, ACCENT);
                        canvas.text(&label, text_x + 2 * canvas.cell_w, y, TEXT);
                    } else {
                        if hovered {
                            canvas.fill(row.rect, HOVER_SHADE);
                        }
                        let ink = if hovered { TEXT } else { PALE };
                        canvas.text(&label, text_x + 2 * canvas.cell_w, y, ink);
                    }
                }
                PanelKind::Related(index) => {
                    let Some((_, title)) = self.shown.as_ref().and_then(|s| s.related.get(index))
                    else {
                        continue;
                    };
                    let label = clip(canvas.font, title, columns);
                    let ink = if hovered { TEXT } else { ACCENT };
                    canvas.text(&label, text_x, y, ink);
                    let width = canvas.cells(&label) * canvas.cell_w;
                    canvas.hline(text_x, row.rect.y + canvas.cell_h - 2, width, ink);
                }
            }
        }
    }

    /// The title of the heading whose text is on `row`.
    pub(super) fn outline_title(&self, row: usize) -> String {
        self.shown
            .as_ref()
            .and_then(|shown| shown.layout.headings.iter().find(|h| h.row == row))
            .map(|heading| heading.title.clone())
            .unwrap_or_default()
    }

    fn page(&self, canvas: &mut Canvas<'_>, pointer: Option<Point>) {
        let Some(shown) = &self.shown else {
            return;
        };
        let text = self.geometry.text(self.nav_collapsed);
        let layout = &shown.layout;
        let scroll = self.scroll();
        let visible = layout.visible(scroll, text.height);
        let focus = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.current().focus);
        let hovered = pointer.and_then(|at| self.link_at(at));
        let width = text.width;
        let mut y = text.y;
        for row in &layout.rows[visible.clone()] {
            let height = row.height(layout.cell_h);
            draw_row(canvas, row, text.x, y, width, focus, hovered);
            y += height;
        }
        let track = self.geometry.track();
        if let Some((start, length)) = thumb(scroll, visible.len(), layout.rows.len(), track.height)
        {
            canvas.fill(track, BAR);
            canvas.fill(
                Rect {
                    y: track.y + start,
                    height: length,
                    ..track
                },
                DIM,
            );
        }
    }

    fn status(&self, canvas: &mut Canvas<'_>, pointer: Option<Point>, notice: Option<(&str, u8)>) {
        let bar = self.geometry.status;
        canvas.fill(bar, BAR);
        let hint = self.hint();
        let (text, ink) = match (notice, pointer.and_then(|at| self.hovered_target(at))) {
            (Some((text, ink)), _) => (text, ink),
            (None, Some(target)) => (target, PALE),
            (None, None) => (hint.as_str(), DIM),
        };
        let columns = usize::try_from((bar.width - 8) / canvas.cell_w.max(1)).unwrap_or(0);
        let y = canvas.bar_text_y(bar.y, bar.height);
        let shown = take_cells(canvas.font, text, columns);
        canvas.text(shown, 4, y, ink);
    }

    /// The list of every tab, `focus` highlighted.
    fn menu_window(
        &self,
        canvas: &mut Canvas<'_>,
        source: Source<'_>,
        focus: usize,
        pointer: Option<Point>,
    ) {
        let labels = self.labels(source);
        let (window, rows) = self.geometry.menu_rows(labels.len(), focus);
        canvas.fill(window, DIM);
        canvas.fill(window.inset(1), BAR);
        for (index, rect) in rows {
            let lit = index == focus || pointer.is_some_and(|at| rect.contains(at));
            if lit {
                canvas.fill(rect, HOVER_SHADE);
            }
            if index == self.active {
                canvas.text(">", rect.x + 2, rect.y + 1, ACCENT);
            }
            let ink = if lit { TEXT } else { PALE };
            canvas.text(
                &labels[index],
                rect.x + 2 + 2 * canvas.cell_w,
                rect.y + 1,
                ink,
            );
        }
    }

    fn search_window(&self, canvas: &mut Canvas<'_>, search: &Search, pointer: Option<Point>) {
        let layout: SearchLayout = self.search_layout(self.search_area());
        draw_window(
            canvas.framebuffer,
            canvas.font,
            layout.window,
            layout.title_bar,
            &ui_text::get("docs-search-docs"),
            canvas.cell_w,
        );
        canvas.fill(layout.field, INK);
        search.field().render(
            canvas.framebuffer,
            canvas.font,
            layout.field.inset(1),
            FieldColors {
                ground: BODY,
                ink: INK,
                selection: INK,
                selected_ink: BODY,
            },
            true,
            canvas.cell_w,
        );
        for (rect, scope) in layout.scopes.iter().zip(Scope::ALL) {
            let on = scope == search.scope();
            let (ground, ink) = if on { (INK, BODY) } else { (BODY, INK) };
            canvas.fill(*rect, ground);
            canvas.text(&scope.label(), rect.x + 3, rect.y, ink);
        }
        let found = search.results().len();
        let count = ui_text::get_with(
            "docs-search-page-count",
            &[("count", Arg::Number(found as f64))],
        );
        let count_x = layout.count.right() - canvas.cells(&count) * canvas.cell_w;
        canvas.text(&count, count_x.max(layout.count.x), layout.count.y, INK);

        let columns = usize::try_from(layout.rows[0].width / canvas.cell_w.max(1)).unwrap_or(0);
        for (slot, rect) in layout.rows.iter().enumerate() {
            let Some(found) = search.results().get(search.first() + slot) else {
                break;
            };
            let focused = search.first() + slot == search.focus();
            let hovered = pointer.is_some_and(|at| rect.contains(at));
            let (ground, ink) = if focused || hovered {
                (INK, BODY)
            } else {
                (BODY, INK)
            };
            canvas.fill(*rect, ground);
            let mut mark = [0; 4];
            canvas.text(found.mark.encode_utf8(&mut mark), rect.x + 2, rect.y, ink);
            let tag_cells = canvas.font.text_cells(&found.tag);
            let tag_x = rect.right() - 2 - i32::try_from(tag_cells).unwrap_or(0) * canvas.cell_w;
            canvas.text(&found.tag, tag_x, rect.y, ink);
            let room = columns.saturating_sub(tag_cells + 5);
            let mut title = clip(canvas.font, &found.title, room);
            if found.is_fallback {
                title.push('*');
            }
            canvas.text(&title, rect.x + 2 + 2 * canvas.cell_w, rect.y, ink);
        }
        if search.results().is_empty() {
            canvas.text(
                &ui_text::get("docs-no-page-matches"),
                layout.rows[0].x + 2,
                layout.rows[0].y,
                INK,
            );
        }
    }
}

/// A diagonal cross filling `rect`.
fn close_glyph(canvas: &mut Canvas<'_>, rect: Rect, color: u8) {
    for i in 0..CLOSE_SIZE {
        pixel(canvas.framebuffer, rect.x + i, rect.y + i, color);
        pixel(
            canvas.framebuffer,
            rect.x + CLOSE_SIZE - 1 - i,
            rect.y + i,
            color,
        );
    }
}

/// `text` kept to `cells` of `font`'s cells, marking a cut with `..`.
fn clip(font: &BitmapFont, text: &str, cells: usize) -> String {
    if font.text_cells(text) <= cells {
        return text.to_owned();
    }
    let room = cells.saturating_sub(font.text_cells(ELLIPSIS));
    format!("{}{ELLIPSIS}", take_cells(font, text, room))
}

/// `text` kept to `cells` of `font`'s cells by cutting its start.
fn shorten_start(font: &BitmapFont, text: &str, cells: usize) -> String {
    if font.text_cells(text) <= cells {
        return text.to_owned();
    }
    let room = cells.saturating_sub(font.text_cells(ELLIPSIS));
    let mut used = 0;
    let mut start = text.len();
    for (at, ch) in text.char_indices().rev() {
        used += font.cells(ch);
        if used > room {
            break;
        }
        start = at;
    }
    format!("{ELLIPSIS}{}", &text[start..])
}

/// Draws `row` with its top at `y` and its cells starting at `x`, across
/// `width` pixels.
fn draw_row(
    canvas: &mut Canvas<'_>,
    row: &Row,
    x: i32,
    y: i32,
    width: i32,
    focus: Option<usize>,
    hovered: Option<usize>,
) {
    let height = row.height(canvas.cell_h);
    draw_deco(canvas, &row.deco, x, y, width, height);
    let text_y = y + i32::from(row.top);
    for run in &row.runs {
        draw_run(canvas, run, x, text_y, focus, hovered);
    }
}

fn draw_run(
    canvas: &mut Canvas<'_>,
    run: &Run,
    x: i32,
    y: i32,
    focus: Option<usize>,
    hovered: Option<usize>,
) {
    let left = x + run.col as i32 * canvas.cell_w;
    let width = run.cells as i32 * canvas.cell_w;
    let focused = run.style.link.is_some() && run.style.link == focus;
    let hot = run.style.link.is_some() && run.style.link == hovered;
    let bg = if focused {
        Some(HOVER_SHADE)
    } else {
        run.style.bg
    };
    if let Some(bg) = bg {
        // A pixel shorter at both ends, so bands on neighbouring lines stay
        // apart.
        canvas.fill(
            Rect {
                x: left,
                y: y + 1,
                width,
                height: canvas.cell_h - 2,
            },
            bg,
        );
    }
    let ink = if focused || hot { TEXT } else { run.style.ink };
    canvas.text(&run.text, left, y, ink);
    if run.style.underline {
        canvas.hline(left, y + canvas.cell_h - 2, width, ink);
    }
}

/// The lines around a row's text: bands, boxes, rules.
fn draw_deco(canvas: &mut Canvas<'_>, deco: &Deco, x: i32, y: i32, width: i32, height: i32) {
    let cell_w = canvas.cell_w;
    if let Some(table) = &deco.table {
        let table_width = table.width as i32 * cell_w;
        let (left, right) = (x + FRAME_INSET, x + table_width - FRAME_INSET - 1);
        if deco.fill.is_some() {
            canvas.fill(
                Rect {
                    x: left,
                    y,
                    width: right - left + 1,
                    height,
                },
                BAR,
            );
        }
        canvas.vline(left, y, height, DIM);
        canvas.vline(right, y, height, DIM);
        for &col in &table.dividers {
            let at = x + col as i32 * cell_w - cell_w / 2;
            canvas.vline(at, y, height, if table.header { DIM } else { BAR });
        }
        if table.rule_above {
            let ink = if table.outer_top { DIM } else { BAR };
            canvas.hline(left, y, right - left + 1, ink);
        }
        if table.last {
            canvas.hline(left, y + height - 1, right - left + 1, DIM);
        }
    } else if let Some(fill) = deco.fill {
        canvas.fill(
            Rect {
                x,
                y,
                width,
                height,
            },
            fill,
        );
    }
    if let Some(frame) = &deco.frame {
        let (left, right) = (x + FRAME_INSET, x + width - FRAME_INSET - 1);
        if frame.boxed {
            canvas.vline(left, y, height, frame.ink);
            canvas.vline(right, y, height, frame.ink);
            if frame.first {
                canvas.hline(left, y, right - left + 1, frame.ink);
            }
            if frame.last {
                canvas.hline(left, y + height - 1, right - left + 1, frame.ink);
            }
        } else {
            canvas.vline(x + 1, y, height, frame.ink);
        }
    }
    if let Some(ink) = deco.underline {
        canvas.hline(x, y + height - 1, width, ink);
    }
    if deco.rule {
        canvas.hline(x, y + height / 2, width, DIM);
    }
}

#[cfg(test)]
mod tests;
