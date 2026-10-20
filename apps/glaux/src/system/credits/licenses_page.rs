use glaux_i18n::Arg;
use glaux_input::InputEvent;
use glaux_machine::framebuffer::WIDTH;

use super::catalog::{Catalog, LicenseFilter};
use super::paint::{BAR, Painter, ROW};
use crate::system::{BACKGROUND, DIM, PALE, TEXT, WARN};
use crate::ui_text;

const LIST_TOP: i32 = 24;
const LIST_ROWS: usize = 16;
const BAR_WIDTH: usize = 62;
const BAR_LEFT: i32 = 252;

#[derive(Debug, Default)]
pub(super) struct LicensesPage {
    selected: usize,
    top: usize,
}

pub(super) enum Action {
    Handled,
    Pass,
    Show(LicenseFilter),
}

impl LicensesPage {
    /// Every license gets a row, and one more stands for the components
    /// that declare none.
    fn rows(catalog: &Catalog) -> usize {
        catalog.summary().len() + usize::from(catalog.undeclared() > 0)
    }

    pub(super) fn handle_input(&mut self, event: InputEvent, catalog: &Catalog) -> Action {
        let rows = Self::rows(catalog);
        let last = rows.saturating_sub(1);
        let at = match event {
            InputEvent::Up { .. } => self.selected.saturating_sub(1),
            InputEvent::Down { .. } => (self.selected + 1).min(last),
            InputEvent::PageUp { .. } => self.selected.saturating_sub(LIST_ROWS),
            InputEvent::PageDown { .. } => (self.selected + LIST_ROWS).min(last),
            InputEvent::Home { .. } | InputEvent::DocumentStart { .. } => 0,
            InputEvent::End { .. } | InputEvent::DocumentEnd { .. } => last,
            InputEvent::Enter if rows > 0 => {
                return Action::Show(match catalog.summary().get(self.selected) {
                    Some(license) => LicenseFilter::Id(license.id.clone()),
                    None => LicenseFilter::Undeclared,
                });
            }
            _ => return Action::Pass,
        };
        self.selected = at;
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + LIST_ROWS {
            self.top = self.selected + 1 - LIST_ROWS;
        }
        Action::Handled
    }

    pub(super) fn render(&self, p: &mut Painter<'_>, catalog: &Catalog) {
        let width = WIDTH as i32;
        p.fill(0, ROW, width, ROW, BACKGROUND);
        p.fill(0, ROW * 2 - 1, width, 1, BAR);
        p.text(16, ROW, &ui_text::get("credits-licenses-license"), PALE);
        p.text(200, ROW, &ui_text::get("credits-licenses-required"), PALE);
        p.text(
            230,
            ROW,
            &ui_text::get("credits-licenses-alternative"),
            PALE,
        );

        let rows = Self::rows(catalog);
        let biggest = catalog
            .summary()
            .iter()
            .map(|license| license.required + license.offered)
            .max()
            .unwrap_or(1)
            .max(1);
        let scale = |count: usize| -> i32 {
            if count == 0 {
                0
            } else {
                (count * BAR_WIDTH).div_ceil(biggest).max(2) as i32
            }
        };
        for row in self.top..rows.min(self.top + LIST_ROWS) {
            let y = LIST_TOP + (row - self.top) as i32 * ROW;
            let selected = row == self.selected;
            let (text, soft) = if selected {
                (BACKGROUND, BACKGROUND)
            } else {
                (TEXT, PALE)
            };
            if selected {
                p.fill(0, y, width, ROW, PALE);
            }
            match catalog.summary().get(row) {
                Some(license) => {
                    p.text_clipped(16, y, &license.id, 30, text);
                    for (right, count) in [(218, license.required), (248, license.offered)] {
                        p.text_right(right, y, &count.to_string(), soft);
                    }
                    let solid = scale(license.required);
                    let hollow =
                        scale(license.offered).max(if license.offered > 0 { 3 } else { 0 });
                    p.fill(BAR_LEFT, y + 3, solid, 6, soft);
                    if hollow > 0 {
                        let x = BAR_LEFT + solid;
                        p.fill(x, y + 3, hollow, 6, soft);
                        p.fill(
                            x + 1,
                            y + 4,
                            hollow - 2,
                            4,
                            if selected { PALE } else { BACKGROUND },
                        );
                    }
                }
                None => {
                    let color = if selected { BACKGROUND } else { WARN };
                    p.text_clipped(16, y, &ui_text::get("credits-not-declared"), 30, color);
                    let note = ui_text::get_with(
                        "credits-licenses-entries",
                        &[("count", Arg::Number(catalog.undeclared() as f64))],
                    );
                    p.text(BAR_LEFT, y, &note, text);
                }
            }
        }
        p.scrollbar(
            (316, LIST_TOP, ROW * LIST_ROWS as i32),
            (self.top, LIST_ROWS, rows),
            (BAR, DIM),
        );

        let y = 240 - ROW * 2;
        p.fill(0, y, width, ROW, BACKGROUND);
        p.fill(0, y, width, 1, BAR);
        let sentence = match catalog.summary().get(self.selected) {
            Some(license) => {
                let id = if license.required == 0 {
                    "credits-licenses-offered-only"
                } else if license.offered == 0 {
                    "credits-licenses-required-only"
                } else {
                    "credits-licenses-both"
                };
                ui_text::get_with(
                    id,
                    &[
                        ("license", Arg::Text(&license.id)),
                        ("required", Arg::Number(license.required as f64)),
                        ("offered", Arg::Number(license.offered as f64)),
                    ],
                )
            }
            None if rows > 0 => ui_text::get_with(
                "credits-licenses-undeclared",
                &[("count", Arg::Number(catalog.undeclared() as f64))],
            ),
            None => String::new(),
        };
        p.text_clipped(4, y + 1, &sentence, 51, TEXT);
        let status = format!("{}/{}", (self.selected + 1).min(rows), rows);
        p.status_bar(&ui_text::get("credits-hint-licenses"), &status);
    }
}
