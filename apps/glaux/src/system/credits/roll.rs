use glaux_input::InputEvent;
use glaux_machine::machine::{GENERATION, NAME};

use super::catalog::Catalog;
use super::editorial::{Editorial, Item};
use super::paint::{IDLE, Painter, ROW};
use crate::licenses::Category;
use crate::system::screen::{DIM, PALE, TEXT};
use crate::ui_text;
use glaux_i18n::Arg;

const INTRO_TICKS: u32 = 180;
const LINE_TICKS: u32 = 60;
/// Ticks the roll waits after the reader scrolled it by hand.
const HOLD_TICKS: u32 = 300;
pub(super) const TOP: i32 = 16;
pub(super) const VISIBLE: usize = 17;
const MARGIN: i32 = 36;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RollLine {
    Heading(String),
    Pair(String, String),
    Text(String),
    Dim(String),
    Blank,
}

/// Builds the roll from the editorial sections that have items, then the
/// bundled art with its authors, and ends with a pointer to the legal notices.
pub(super) fn lines(editorial: &Editorial, catalog: &Catalog) -> Vec<RollLine> {
    let mut lines = Vec::new();
    for section in editorial.sections.iter().filter(|s| !s.items.is_empty()) {
        lines.push(RollLine::Heading(section.title.clone()));
        for item in &section.items {
            lines.push(match item {
                Item::Person { role, name } => RollLine::Pair(role.clone(), name.clone()),
                Item::Message(text) => RollLine::Text(text.clone()),
            });
        }
        lines.push(RollLine::Blank);
    }
    let mut art: Vec<&str> = catalog
        .entries()
        .iter()
        .filter(|entry| entry.category == Category::Art)
        .map(|entry| entry.name.as_str())
        .collect();
    art.sort_by_key(|name| name.to_lowercase());
    if !art.is_empty() {
        lines.push(RollLine::Heading(ui_text::get(
            "credits-creative-resources",
        )));
        for name in art {
            lines.push(match editorial.authors.get(name) {
                Some(author) => RollLine::Pair(name.to_owned(), author.clone()),
                None => RollLine::Text(name.to_owned()),
            });
        }
        lines.push(RollLine::Blank);
    }
    lines.push(RollLine::Dim(ui_text::get("credits-sources-and-licenses")));
    lines
}

#[derive(Debug)]
pub(super) struct Roll {
    lines: Vec<RollLine>,
    intro: u32,
    offset: usize,
    ticks: u32,
    hold: u32,
    paused: bool,
}

impl Roll {
    pub(super) fn new(lines: Vec<RollLine>) -> Self {
        Self {
            lines,
            intro: INTRO_TICKS,
            offset: 0,
            ticks: 0,
            hold: 0,
            paused: false,
        }
    }

    #[cfg(test)]
    pub(super) fn is_title_card(&self) -> bool {
        self.intro > 0
    }

    fn max_offset(&self) -> usize {
        self.lines.len().saturating_sub(VISIBLE)
    }

    fn percent(&self) -> usize {
        match self.max_offset() {
            0 => 100,
            max => self.offset * 100 / max,
        }
    }

    #[cfg(test)]
    pub(super) fn is_paused(&self) -> bool {
        self.paused
    }

    pub(super) fn tick(&mut self) {
        if self.intro > 0 {
            self.intro -= 1;
        } else if self.paused {
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.ticks += 1;
            if self.ticks >= LINE_TICKS {
                self.ticks = 0;
                self.offset = (self.offset + 1).min(self.max_offset());
            }
        }
    }

    pub(super) fn handle_input(&mut self, event: InputEvent) -> bool {
        let max = self.max_offset();
        let target = match event {
            InputEvent::Text(' ') => {
                if self.intro > 0 {
                    self.intro = 0;
                } else {
                    self.paused = !self.paused;
                }
                return true;
            }
            InputEvent::Up { .. } => self.offset.saturating_sub(1),
            InputEvent::Down { .. } => (self.offset + 1).min(max),
            InputEvent::PageUp { .. } => self.offset.saturating_sub(VISIBLE),
            InputEvent::PageDown { .. } => (self.offset + VISIBLE).min(max),
            InputEvent::Home { .. } | InputEvent::DocumentStart { .. } => 0,
            InputEvent::End { .. } | InputEvent::DocumentEnd { .. } => max,
            _ => return false,
        };
        self.intro = 0;
        self.offset = target;
        self.ticks = 0;
        self.hold = HOLD_TICKS;
        true
    }

    pub(super) fn hint(&self) -> String {
        ui_text::get(if self.intro > 0 {
            "credits-hint-intro"
        } else if self.paused {
            "credits-hint-paused"
        } else {
            "credits-hint-playing"
        })
    }

    pub(super) fn status(&self) -> String {
        if self.intro > 0 {
            return ui_text::get_with(
                "credits-status-intro",
                &[("seconds", Arg::Number(f64::from(self.intro.div_ceil(60))))],
            );
        }
        let id = if self.paused {
            "credits-status-paused"
        } else {
            "credits-status-playing"
        };
        ui_text::get_with(id, &[("percent", Arg::Number(self.percent() as f64))])
    }

    pub(super) fn render(
        &self,
        p: &mut Painter<'_>,
        logo: &glaux_machine::system_rom::IndexedImage,
        copyright: &str,
    ) {
        if self.intro > 0 {
            self.render_title_card(p, logo, copyright);
            return;
        }
        let width = glaux_machine::framebuffer::WIDTH as i32;
        for (row, line) in self
            .lines
            .iter()
            .skip(self.offset)
            .take(VISIBLE)
            .enumerate()
        {
            let y = TOP + row as i32 * ROW;
            match line {
                RollLine::Heading(title) => p.heading(y, title),
                RollLine::Pair(label, value) => {
                    let cell = p.cell_w();
                    p.text(MARGIN, y, label, DIM);
                    let right = width - MARGIN;
                    p.text_right(right, y, value, TEXT);
                    // The dotted leader fills what is left between the two.
                    let from = MARGIN + p.width(label) + cell;
                    let to = right - p.width(value) - cell;
                    if to > from {
                        let dots = ((to - from) / cell) as usize;
                        p.text(from, y, &".".repeat(dots), IDLE);
                    }
                }
                RollLine::Text(text) => p.text_center(y, text, TEXT),
                RollLine::Dim(text) => p.text_center(y, text, DIM),
                RollLine::Blank => {}
            }
        }
        p.scrollbar(
            (316, 12, 216),
            (
                self.offset,
                self.max_offset().max(1),
                self.max_offset() + VISIBLE,
            ),
            (super::paint::BAR, DIM),
        );
        if self.paused {
            let label = ui_text::get("credits-paused-tag");
            let w = p.width(&label) + p.cell_w() * 2;
            let x = width - w - 8;
            p.fill(x, 14, w, ROW, super::paint::BAR);
            p.text(x + p.cell_w(), 14, &label, TEXT);
        }
    }

    fn render_title_card(
        &self,
        p: &mut Painter<'_>,
        logo: &glaux_machine::system_rom::IndexedImage,
        copyright: &str,
    ) {
        use glaux_video::draw::{draw_text_scaled, fill_rect};
        let width = glaux_machine::framebuffer::WIDTH as i32;
        p.checker(0, 14, width, 8);
        p.checker(0, 212, width, 8);
        let longest = logo.width().max(logo.height()).max(1);
        let scale = (32 / longest).max(1) as i32;
        let (w, h) = (logo.width() as i32 * scale, logo.height() as i32 * scale);
        let (left, top) = ((width - w) / 2, 50 + (32 - h) / 2);
        for y in 0..logo.height() {
            for x in 0..logo.width() {
                if let Some(color) = logo.get(x, y) {
                    fill_rect(
                        p.fb,
                        left + x as i32 * scale,
                        top + y as i32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        let name_width = p.width(NAME) * 2;
        draw_text_scaled(p.fb, p.font, NAME, (width - name_width) / 2, 90, 2, TEXT);
        let tagline =
            ui_text::get_with("credits-tagline", &[("generation", Arg::Text(GENERATION))]);
        p.text_center(126, &tagline, PALE);
        p.text_center(138, copyright, DIM);
        let (x, y, w) = (110, 162, 99);
        fill_rect(p.fb, x, y, w, 6, super::paint::BAR);
        let filled = w * (INTRO_TICKS - self.intro) as i32 / INTRO_TICKS as i32;
        fill_rect(p.fb, x, y, filled, 6, crate::system::screen::ACCENT);
    }
}

#[cfg(test)]
mod tests;
