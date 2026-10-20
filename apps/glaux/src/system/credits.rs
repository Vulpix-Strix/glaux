mod catalog;
mod editorial;
mod libraries;
mod licenses_page;
mod paint;
mod reader;
mod roll;
mod spdx;
mod system_page;

use glaux_input::InputEvent;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::system_rom::SystemRom;

use crate::licenses::{self, Licenses};
use crate::rom::CREDITS_RON;
use crate::ui_text;
use catalog::Catalog;
use editorial::Editorial;
use libraries::{Libraries, Reaction};
use licenses_page::{Action, LicensesPage};
use paint::{Painter, ROW};
use roll::Roll;
use system_page::SystemPage;

use super::BACKGROUND;

const KEYS: [(&str, &str); 12] = [
    ("Up/Down", "credits-key-move"),
    ("PgUp/PgDn", "credits-key-page"),
    ("Home/End", "credits-key-ends"),
    ("Enter", "credits-key-open"),
    ("Esc", "credits-key-back"),
    ("/", "credits-key-find"),
    ("C", "credits-key-category"),
    ("S", "credits-key-sort"),
    ("R", "credits-key-reflow"),
    ("Tab", "credits-key-text"),
    ("F1-F4", "credits-key-pages"),
    ("Left/Right", "credits-key-neighbors"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Credits,
    System,
    Libraries,
    Licenses,
}

impl Page {
    const ALL: [Page; 4] = [Self::Credits, Self::System, Self::Libraries, Self::Licenses];

    fn index(self) -> usize {
        Self::ALL.iter().position(|&page| page == self).unwrap_or(0)
    }

    fn label_id(self) -> &'static str {
        match self {
            Self::Credits => "credits-tab-credits",
            Self::System => "credits-tab-system",
            Self::Libraries => "credits-tab-libraries",
            Self::Licenses => "credits-tab-licenses",
        }
    }
}

#[derive(Debug)]
pub struct CreditsView {
    page: Page,
    catalog: Catalog,
    roll: Roll,
    system: SystemPage,
    libraries: Libraries,
    licenses: LicensesPage,
    keys_open: bool,
}

impl CreditsView {
    /// Opens the view on its first page. The license inventory is embedded
    /// in the binary, so a failure to read it is a build mistake. It is
    /// logged and the pages show what is left.
    pub fn open() -> Self {
        let licenses = licenses::third_party().unwrap_or_else(|error| {
            tracing::error!(%error, "the license inventory is unreadable");
            Licenses {
                texts: Vec::new(),
                entries: Vec::new(),
            }
        });
        let catalog = Catalog::new(licenses);
        let editorial = Editorial::parse(CREDITS_RON);
        Self {
            page: Page::Credits,
            roll: Roll::new(roll::lines(&editorial, &catalog)),
            system: SystemPage::default(),
            libraries: Libraries::new(&catalog),
            licenses: LicensesPage::default(),
            catalog,
            keys_open: false,
        }
    }

    #[cfg(test)]
    pub fn page(&self) -> Page {
        self.page
    }

    pub fn tick(&mut self) {
        if self.page == Page::Credits {
            self.roll.tick();
        }
    }

    pub fn handle_input(&mut self, event: InputEvent) -> bool {
        if self.keys_open {
            if matches!(
                event,
                InputEvent::Break | InputEvent::Enter | InputEvent::Text('?')
            ) {
                self.keys_open = false;
            }
            return false;
        }
        let typing = self.page == Page::Libraries && self.libraries.is_typing();
        let handled = match self.page {
            Page::Credits => self.roll.handle_input(event),
            Page::System => self.system.handle_input(event),
            Page::Libraries => {
                self.libraries.handle_input(event, &self.catalog) == Reaction::Handled
            }
            Page::Licenses => match self.licenses.handle_input(event, &self.catalog) {
                Action::Handled => true,
                Action::Pass => false,
                Action::Show(filter) => {
                    self.libraries.show_license(filter, &self.catalog);
                    self.page = Page::Libraries;
                    true
                }
            },
        };
        if handled || typing {
            return false;
        }
        match event {
            InputEvent::Docs => self.page = Page::Credits,
            InputEvent::Function(number @ 2..=4) => {
                self.page = Page::ALL[usize::from(number) - 1];
            }
            InputEvent::Left { .. } => self.step_page(false),
            InputEvent::Right { .. } => self.step_page(true),
            InputEvent::Text('?') => self.keys_open = true,
            InputEvent::Break => return true,
            _ => {}
        }
        false
    }

    fn step_page(&mut self, forward: bool) {
        let at = self.page.index();
        let at = if forward {
            (at + 1).min(3)
        } else {
            at.saturating_sub(1)
        };
        self.page = Page::ALL[at];
    }

    pub fn render(&self, framebuffer: &mut Framebuffer, rom: &SystemRom) {
        let mut p = Painter::new(framebuffer, rom.font());
        p.clear(BACKGROUND);
        let reading = self.page == Page::Libraries && self.libraries.is_reading();
        match self.page {
            Page::Credits => self.roll.render(&mut p, rom.logo(), rom.copyright()),
            Page::System => self.system.render(&mut p, rom, &self.catalog),
            Page::Libraries => self.libraries.render(&mut p, &self.catalog),
            Page::Licenses => self.licenses.render(&mut p, &self.catalog),
        }
        if self.page == Page::Credits {
            p.status_bar(&self.roll.hint(), &self.roll.status());
        }
        if !reading {
            let tabs: Vec<String> = Page::ALL
                .iter()
                .enumerate()
                .map(|(at, page)| format!("F{} {}", at + 1, ui_text::get(page.label_id())))
                .collect();
            let position = format!("{}/{}", self.page.index() + 1, Page::ALL.len());
            p.tab_bar(&tabs, self.page.index(), &position);
        }
        if self.keys_open {
            render_keys(&mut p);
        }
    }
}

fn render_keys(p: &mut Painter<'_>) {
    let (w, h) = (240, ROW + 6 + KEYS.len() as i32 * ROW + 6);
    let (x, y) = ((320 - w) / 2, 28);
    p.window((x, y, w, h), &ui_text::get("credits-keys-title"));
    for (row, (key, description)) in KEYS.iter().enumerate() {
        let line = y + ROW + 6 + row as i32 * ROW;
        p.text(x + 6, line, key, paint::BAR);
        p.text(x + 72, line, &ui_text::get(description), BACKGROUND);
    }
}

#[cfg(test)]
mod tests;
