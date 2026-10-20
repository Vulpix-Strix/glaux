use glaux_i18n::Arg;
use glaux_input::InputEvent;
use glaux_machine::framebuffer::WIDTH;

use super::catalog::{Catalog, LicenseFilter};
use super::paint::{BAR, Painter, ROW, clip, wrap};
use super::reader::Reader;
use super::spdx::Terms;
use crate::licenses::{Category, Entry};
use crate::system::{ACCENT, BACKGROUND, DIM, PALE, TEXT};
use crate::ui_text;

const LIST_TOP: i32 = 24;
pub(super) const LIST_ROWS: usize = 17;
const QUERY_MAX: usize = 40;
const CARD_VALUE_X: i32 = 54;
const CARD_WIDTH: i32 = 296;
const INK: u8 = BACKGROUND;
const PAPER_MUTE: u8 = BAR;
const PAPER_RULE: u8 = 14;
const HOVER_FACE: u8 = 19;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    License,
}

#[derive(Debug)]
enum Level {
    List,
    Card,
    Reading(Reader),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reaction {
    Handled,
    Pass,
}

#[derive(Debug)]
pub(super) struct Libraries {
    category: Option<Category>,
    sort: Sort,
    license: Option<LicenseFilter>,
    query: String,
    draft: Option<String>,
    visible: Vec<usize>,
    selected: usize,
    top: usize,
    level: Level,
}

impl Libraries {
    pub(super) fn new(catalog: &Catalog) -> Self {
        let mut page = Self {
            category: None,
            sort: Sort::Name,
            license: None,
            query: String::new(),
            draft: None,
            visible: Vec::new(),
            selected: 0,
            top: 0,
            level: Level::List,
        };
        page.refresh(catalog);
        page
    }

    #[cfg(test)]
    pub(super) fn shown(&self) -> &[usize] {
        &self.visible
    }

    #[cfg(test)]
    pub(super) fn selected(&self) -> usize {
        self.selected
    }

    pub(super) fn is_typing(&self) -> bool {
        self.draft.is_some()
    }

    pub(super) fn is_reading(&self) -> bool {
        matches!(self.level, Level::Reading(_))
    }

    /// Lists the components that mention `filter`, from the top, dropping
    /// the other filters so none of them hides the answer.
    pub(super) fn show_license(&mut self, filter: LicenseFilter, catalog: &Catalog) {
        self.license = Some(filter);
        self.category = None;
        self.query.clear();
        self.draft = None;
        self.level = Level::List;
        self.visible.clear();
        self.refresh(catalog);
    }

    fn effective_query(&self) -> &str {
        self.draft.as_deref().unwrap_or(&self.query)
    }

    fn refresh(&mut self, catalog: &Catalog) {
        let anchor = self.visible.get(self.selected).copied();
        let query = self.effective_query().to_lowercase();
        let entries = catalog.entries();
        let mut visible: Vec<usize> = (0..entries.len())
            .filter(|&at| {
                let entry = &entries[at];
                self.category.is_none_or(|wanted| entry.category == wanted)
                    && self
                        .license
                        .as_ref()
                        .is_none_or(|filter| catalog.matches(at, filter))
                    && (query.is_empty() || entry.name.to_lowercase().contains(&query))
            })
            .collect();
        let name_key = |at: usize| entries[at].name.to_lowercase();
        match self.sort {
            Sort::Name => visible.sort_by_cached_key(|&at| (name_key(at), at)),
            // Components without a license go last.
            Sort::License => visible.sort_by_cached_key(|&at| {
                let license = entries[at].license.to_lowercase();
                (license.is_empty(), license, name_key(at), at)
            }),
        }
        self.selected = anchor
            .and_then(|anchor| visible.iter().position(|&at| at == anchor))
            .unwrap_or(0);
        self.visible = visible;
        self.top = 0;
        self.keep_selection_in_view();
    }

    fn keep_selection_in_view(&mut self) {
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + LIST_ROWS {
            self.top = self.selected + 1 - LIST_ROWS;
        }
    }

    fn select(&mut self, at: usize) {
        self.selected = at.min(self.visible.len().saturating_sub(1));
        self.keep_selection_in_view();
    }

    fn move_selection(&mut self, event: InputEvent) -> bool {
        let last = self.visible.len().saturating_sub(1);
        let at = match event {
            InputEvent::Up { .. } => self.selected.saturating_sub(1),
            InputEvent::Down { .. } => (self.selected + 1).min(last),
            InputEvent::PageUp { .. } => self.selected.saturating_sub(LIST_ROWS),
            InputEvent::PageDown { .. } => (self.selected + LIST_ROWS).min(last),
            InputEvent::Home { .. } | InputEvent::DocumentStart { .. } => 0,
            InputEvent::End { .. } | InputEvent::DocumentEnd { .. } => last,
            _ => return false,
        };
        self.select(at);
        true
    }

    pub(super) fn handle_input(&mut self, event: InputEvent, catalog: &Catalog) -> Reaction {
        if self.draft.is_some() {
            self.type_query(event, catalog);
            return Reaction::Handled;
        }
        match &mut self.level {
            Level::Reading(_)
                if matches!(
                    event,
                    InputEvent::Function(_) | InputEvent::Docs | InputEvent::Text('?')
                ) =>
            {
                Reaction::Pass
            }
            Level::Reading(reader) => {
                if event == InputEvent::Break {
                    self.level = Level::Card;
                } else {
                    reader.handle_input(event, catalog);
                }
                Reaction::Handled
            }
            Level::Card => self.card_input(event, catalog),
            Level::List => self.list_input(event, catalog),
        }
    }

    fn type_query(&mut self, event: InputEvent, catalog: &Catalog) {
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        match event {
            InputEvent::Text(ch) if !ch.is_control() => {
                if draft.chars().count() < QUERY_MAX {
                    draft.push(ch);
                }
            }
            InputEvent::Backspace => {
                draft.pop();
            }
            InputEvent::Enter => {
                self.query = self.draft.take().unwrap_or_default();
            }
            InputEvent::Break => self.draft = None,
            _ => {
                self.move_selection(event);
                return;
            }
        }
        self.refresh(catalog);
    }

    fn list_input(&mut self, event: InputEvent, catalog: &Catalog) -> Reaction {
        if self.move_selection(event) {
            return Reaction::Handled;
        }
        match event {
            InputEvent::Enter if !self.visible.is_empty() => self.level = Level::Card,
            InputEvent::Text('/') => self.draft = Some(self.query.clone()),
            InputEvent::Text('c' | 'C') => {
                self.category = match self.category {
                    None => Some(Category::Library),
                    Some(Category::Library) => Some(Category::Native),
                    Some(Category::Native) => Some(Category::Art),
                    Some(Category::Art) => None,
                };
                self.refresh(catalog);
            }
            InputEvent::Text('s' | 'S') => {
                self.sort = match self.sort {
                    Sort::Name => Sort::License,
                    Sort::License => Sort::Name,
                };
                self.refresh(catalog);
            }
            // Escape peels off the search, then the license filter, and
            // only then leaves the page.
            InputEvent::Break if !self.query.is_empty() => {
                self.query.clear();
                self.refresh(catalog);
            }
            InputEvent::Break if self.license.is_some() => {
                self.license = None;
                self.refresh(catalog);
            }
            _ => return Reaction::Pass,
        }
        Reaction::Handled
    }

    fn card_input(&mut self, event: InputEvent, catalog: &Catalog) -> Reaction {
        match event {
            InputEvent::Left { .. } => self.select(self.selected.saturating_sub(1)),
            InputEvent::Right { .. } => self.select(self.selected + 1),
            InputEvent::Enter => {
                if let Some(&entry) = self.visible.get(self.selected)
                    && let Some(reader) = Reader::open(catalog, entry, 0)
                {
                    self.level = Level::Reading(reader);
                }
            }
            InputEvent::Break => self.level = Level::List,
            // Other keys neither move the card nor leave the page.
            InputEvent::Function(_) | InputEvent::Docs | InputEvent::Text('?') => {
                return Reaction::Pass;
            }
            _ => {}
        }
        Reaction::Handled
    }

    pub(super) fn render(&self, p: &mut Painter<'_>, catalog: &Catalog) {
        if let Level::Reading(reader) = &self.level {
            reader.render(p, catalog);
            return;
        }
        self.render_list(p, catalog);
        if matches!(self.level, Level::Card)
            && let Some(&entry) = self.visible.get(self.selected)
        {
            self.render_card(p, catalog, entry);
        }
    }

    fn render_list(&self, p: &mut Painter<'_>, catalog: &Catalog) {
        let width = WIDTH as i32;
        let cell = p.cell_w();
        p.fill(0, ROW, width, ROW, BACKGROUND);
        p.fill(0, ROW * 2 - 1, width, 1, BAR);
        p.text(4, ROW, &ui_text::get("credits-show"), DIM);
        let mut x = 30;
        for (category, id) in [
            (None, "credits-filter-all"),
            (Some(Category::Library), "credits-filter-lib"),
            (Some(Category::Native), "credits-filter-native"),
            (Some(Category::Art), "credits-filter-art"),
        ] {
            let label = ui_text::get_with(
                id,
                &[("count", Arg::Number(catalog.count(category) as f64))],
            );
            let w = p.width(&label);
            if self.category == category {
                p.fill(x, ROW * 2 - 2, w, 1, ACCENT);
                p.text(x, ROW, &label, TEXT);
            } else {
                p.text(x, ROW, &label, PALE);
            }
            x += w + cell + 2;
        }
        let sort = ui_text::get(match self.sort {
            Sort::Name => "credits-sort-name",
            Sort::License => "credits-sort-license",
        });
        p.text_right(width - cell, ROW, &sort, DIM);

        for (row, &at) in self
            .visible
            .iter()
            .enumerate()
            .skip(self.top)
            .take(LIST_ROWS)
        {
            let entry = &catalog.entries()[at];
            let y = LIST_TOP + (row - self.top) as i32 * ROW;
            let selected = row == self.selected;
            if selected {
                p.fill(0, y, width - 4, ROW, PALE);
            }
            let (name_color, version_color, license_color) = if selected {
                (INK, INK, INK)
            } else {
                (TEXT, DIM, PALE)
            };
            let tag = match entry.category {
                Category::Library => None,
                Category::Native => Some("N"),
                Category::Art => Some("A"),
            };
            if let Some(tag) = tag {
                p.fill(4, y + 1, 10, ROW - 2, BAR);
                p.text(4 + (10 - cell) / 2, y, tag, PALE);
            }
            self.draw_name(p, entry, 18, y, name_color, selected, ACCENT);
            let version = if entry.version.is_empty() {
                "-"
            } else {
                &entry.version
            };
            p.text_clipped(140, y, version, 8, version_color);
            let license = license_label(entry);
            p.text_clipped(194, y, &license, 19, license_color);
        }
        if self.visible.is_empty() {
            p.text(18, LIST_TOP, &ui_text::get("credits-no-match"), DIM);
        } else if self.visible.len() < catalog.entries().len() && self.visible.len() < LIST_ROWS {
            let y = LIST_TOP + self.visible.len() as i32 * ROW + 6;
            let line = ui_text::get_with(
                "credits-match-count",
                &[
                    ("shown", Arg::Number(self.visible.len() as f64)),
                    ("total", Arg::Number(catalog.entries().len() as f64)),
                ],
            );
            p.text(18, y, &line, DIM);
        }
        p.scrollbar(
            (316, LIST_TOP, ROW * LIST_ROWS as i32),
            (self.top, LIST_ROWS, self.visible.len()),
            (BAR, DIM),
        );

        if let Some(draft) = &self.draft {
            self.render_find_bar(p, draft);
            return;
        }
        let position = if self.visible.is_empty() {
            "0/0".to_owned()
        } else {
            format!("{}/{}", self.selected + 1, self.visible.len())
        };
        let status = match &self.license {
            Some(LicenseFilter::Id(id)) => format!("{id} {position}"),
            Some(LicenseFilter::Undeclared) => {
                format!("{} {position}", ui_text::get("credits-not-declared"))
            }
            None => position,
        };
        let hint = if self.query.is_empty() {
            ui_text::get("credits-hint-libraries")
        } else {
            ui_text::get_with(
                "credits-hint-find-kept",
                &[("query", Arg::Text(&self.query))],
            )
        };
        p.status_bar(&hint, &status);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_name(
        &self,
        p: &mut Painter<'_>,
        entry: &Entry,
        x: i32,
        y: i32,
        color: u8,
        selected: bool,
        accent: u8,
    ) {
        let name = clip(p.font, &entry.name, 20);
        let query = self.effective_query().to_lowercase();
        let found = (!query.is_empty())
            .then(|| name.to_lowercase().find(&query))
            .flatten()
            .filter(|&at| name.is_char_boundary(at) && name.is_char_boundary(at + query.len()));
        match found {
            Some(at) => {
                let (before, rest) = name.split_at(at);
                let (hit, after) = rest.split_at(query.len());
                let hit_x = x + p.width(before);
                p.text(x, y, before, color);
                let w = p.width(hit);
                let frame = if selected { INK } else { accent };
                p.fill(hit_x - 1, y, w + 2, 1, frame);
                p.fill(hit_x - 1, y + ROW - 1, w + 2, 1, frame);
                p.fill(hit_x - 1, y, 1, ROW, frame);
                p.fill(hit_x + w, y, 1, ROW, frame);
                p.text(hit_x, y, hit, color);
                p.text(hit_x + w, y, after, color);
            }
            None => p.text(x, y, &name, color),
        }
    }

    fn render_find_bar(&self, p: &mut Painter<'_>, draft: &str) {
        let y = 240 - ROW;
        let width = WIDTH as i32;
        let cell = p.cell_w();
        p.fill(0, y, width, ROW, BAR);
        let prompt = format!("{} ", ui_text::get("credits-find-prompt"));
        p.text(cell, y, &prompt, PALE);
        let x = cell + p.width(&prompt);
        p.text(x, y, draft, TEXT);
        p.fill(x + p.width(draft), y + 1, 1, ROW - 2, TEXT);
        let hint = ui_text::get_with(
            "credits-find-hint",
            &[("count", Arg::Number(self.visible.len() as f64))],
        );
        p.text_right(width - cell, y, &hint, TEXT);
    }

    fn render_card(&self, p: &mut Painter<'_>, catalog: &Catalog, at: usize) {
        let font = p.font;
        let entry = &catalog.entries()[at];
        let cell = p.cell_w();
        let value_cells = ((CARD_WIDTH - CARD_VALUE_X - 6) / cell).max(1) as usize;
        let mut rows: Vec<(String, Vec<String>)> = Vec::new();
        if !entry.version.is_empty() {
            rows.push((
                ui_text::get("credits-card-version"),
                vec![entry.version.clone()],
            ));
        }
        let declared = !entry.license.trim().is_empty();
        let license = if declared {
            wrap(font, &entry.license, value_cells)
        } else {
            vec![format!("(!) {}", ui_text::get("credits-not-declared"))]
        };
        rows.push((ui_text::get("credits-card-license"), license));
        if let Some(terms) = catalog.terms(at) {
            rows.push((ui_text::get("credits-card-terms"), vec![terms_label(terms)]));
        }
        if let Some(repository) = &entry.repository {
            rows.push((
                ui_text::get("credits-card-source"),
                wrap(font, repository, value_cells),
            ));
        }
        let texts = catalog.texts_of(at).len();
        rows.push((
            ui_text::get("credits-card-texts"),
            vec![ui_text::get_with(
                "credits-card-texts-count",
                &[("count", Arg::Number(texts as f64))],
            )],
        ));
        let note = (!declared).then(|| {
            ui_text::get(if entry.version.is_empty() {
                "credits-card-note-nothing"
            } else {
                "credits-card-note-license"
            })
        });

        let lines: i32 = rows.iter().map(|(_, lines)| lines.len() as i32).sum();
        let note_height = if note.is_some() { 4 + ROW } else { 0 };
        let body = 6 + ROW + 4 + lines * ROW + note_height + 6 + 1 + 5 + ROW + 6;
        let height = ROW + body;
        let left = (WIDTH as i32 - CARD_WIDTH) / 2;
        let top = ((240 - height) / 2 - 6).max(LIST_TOP);
        let kind = ui_text::get(match entry.category {
            Category::Library => "credits-kind-library",
            Category::Native => "credits-kind-native",
            Category::Art => "credits-kind-art",
        });
        let title = ui_text::get_with(
            "credits-card-title",
            &[
                ("kind", Arg::Text(&kind)),
                ("position", Arg::Number((self.selected + 1) as f64)),
                ("total", Arg::Number(self.visible.len() as f64)),
            ],
        );
        p.window((left, top, CARD_WIDTH, height), &title);
        let mut y = top + ROW + 6;
        p.text_clipped(
            left + 6,
            y,
            &entry.name,
            ((CARD_WIDTH - 12) / cell) as usize,
            INK,
        );
        y += ROW + 4;
        for (label, lines) in &rows {
            p.text(left + 6, y, label, PAPER_MUTE);
            for line in lines {
                p.text(left + CARD_VALUE_X, y, line, INK);
                y += ROW;
            }
        }
        if let Some(note) = note {
            y += 4;
            p.text_clipped(
                left + 6,
                y,
                &note,
                ((CARD_WIDTH - 12) / cell) as usize,
                PAPER_MUTE,
            );
            y += ROW;
        }
        y += 6;
        p.fill(left + 6, y, CARD_WIDTH - 12, 1, PAPER_RULE);
        y += 5;
        let back = ui_text::get("credits-button-back");
        let read = ui_text::get(if texts == 1 {
            "credits-button-read-text"
        } else {
            "credits-button-read-texts"
        });
        let mut right = left + CARD_WIDTH - 6;
        for (label, focused) in [(&back, false), (&read, true)] {
            let w = p.width(label) + cell * 2;
            right -= w;
            p.fill(right, y, w, ROW, if focused { HOVER_FACE } else { BAR });
            p.text(right + cell, y, label, TEXT);
            right -= 4;
        }
    }
}

fn license_label(entry: &Entry) -> String {
    if entry.license.trim().is_empty() {
        ui_text::get("credits-not-declared")
    } else {
        entry.license.clone()
    }
}

fn terms_label(terms: Terms) -> String {
    match terms {
        Terms::Single => ui_text::get("credits-terms-single"),
        Terms::All(count) => {
            ui_text::get_with("credits-terms-all", &[("count", Arg::Number(count as f64))])
        }
        Terms::Choice(count) => ui_text::get_with(
            "credits-terms-choice",
            &[("count", Arg::Number(count as f64))],
        ),
    }
}

#[cfg(test)]
mod tests;
