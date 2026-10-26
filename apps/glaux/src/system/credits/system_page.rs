use std::cell::Cell;

use glaux_i18n::Arg;
use glaux_input::InputEvent;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::machine::{GENERATION, NAME};
use glaux_machine::palette::PALETTE_LEN;
use glaux_machine::psg::SAMPLE_RATE;
use glaux_machine::sprite::{OBJ_TILE_CAPACITY, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH};
use glaux_machine::system_rom::SystemRom;

use super::catalog::Catalog;
use super::paint::{Painter, ROW};
use crate::licenses::Category;
use crate::system::screen::{DIM, TEXT};
use crate::ui_text;

const TOP: i32 = 30;
const ROOM: i32 = 228 - TOP;
const LABEL_X: i32 = 24;
const VALUE_X: i32 = 108;
const GAP: i32 = 4;
const SWATCH_COLUMNS: usize = 16;
const SWATCH_ROWS: usize = 2;
const SWATCH_PITCH_X: i32 = 12;
const SWATCH_PITCH_Y: i32 = 11;

enum Line {
    Fact(String, String),
    Gap,
    Palette,
}

impl Line {
    fn height(&self) -> i32 {
        match self {
            Self::Fact(..) => ROW,
            Self::Gap => GAP,
            Self::Palette => ROW + SWATCH_ROWS as i32 * SWATCH_PITCH_Y,
        }
    }
}

struct Component {
    label: &'static str,
    parts: &'static [(&'static str, Category)],
}

const COMPONENTS: [Component; 3] = [
    Component {
        label: "credits-system-graphics",
        parts: &[("raylib", Category::Library)],
    },
    Component {
        label: "credits-system-audio",
        parts: &[("miniaudio", Category::Native)],
    },
    Component {
        label: "credits-system-scripting",
        parts: &[("Luau", Category::Native), ("rhai", Category::Library)],
    },
];

fn fact(label: &str, value: String) -> Line {
    Line::Fact(ui_text::get(label), value)
}

fn lines(rom: &SystemRom, catalog: &Catalog) -> Vec<Line> {
    let number = |value: usize| Arg::Number(value as f64);
    let mut lines = vec![
        fact("credits-system-model", format!("{NAME} {GENERATION}")),
        fact(
            "credits-system-version",
            ui_text::get_with(
                "credits-system-version-value",
                &[
                    ("app", Arg::Text(env!("CARGO_PKG_VERSION"))),
                    ("rom", Arg::Text(&rom.version().to_string())),
                ],
            ),
        ),
        fact("credits-system-display", format!("{WIDTH}x{HEIGHT}")),
        fact(
            "credits-system-colors",
            ui_text::get_with(
                "credits-system-colors-value",
                &[("count", number(PALETTE_LEN))],
            ),
        ),
        fact(
            "credits-system-tiles",
            ui_text::get_with(
                "credits-system-tiles-value",
                &[
                    ("width", number(OBJ_TILE_WIDTH)),
                    ("height", number(OBJ_TILE_HEIGHT)),
                    ("count", number(OBJ_TILE_CAPACITY)),
                ],
            ),
        ),
    ];
    let metrics = rom.font().metrics();
    let cell = format!("{}x{}", metrics.cell_width(), metrics.cell_height());
    lines.push(fact(
        "credits-system-font",
        match catalog.find("Pixel Code", Category::Art) {
            Some(font) => ui_text::get_with(
                "credits-system-font-value",
                &[("name", Arg::Text(&font.name)), ("cell", Arg::Text(&cell))],
            ),
            None => cell,
        },
    ));
    lines.push(fact(
        "credits-system-sound",
        ui_text::get_with(
            "credits-system-sound-value",
            &[
                ("channels", number(glaux_audio::CHANNELS)),
                ("rate", number(SAMPLE_RATE as usize)),
            ],
        ),
    ));

    let software: Vec<Line> = COMPONENTS
        .iter()
        .filter_map(|component| {
            let parts: Vec<String> = component
                .parts
                .iter()
                .filter_map(|&(name, category)| catalog.find(name, category))
                .map(|entry| format!("{} {}", entry.name, entry.version))
                .collect();
            (!parts.is_empty()).then(|| fact(component.label, parts.join(", ")))
        })
        .collect();
    if !software.is_empty() {
        lines.push(Line::Gap);
        lines.extend(software);
    }
    lines.push(Line::Gap);
    lines.push(fact(
        "credits-system-notices",
        ui_text::get_with(
            "credits-system-notices-value",
            &[
                ("entries", number(catalog.entries().len())),
                ("texts", number(catalog.text_count())),
            ],
        ),
    ));
    lines.push(fact("credits-system-copyright", rom.copyright().to_owned()));
    lines.push(Line::Gap);
    lines.push(Line::Palette);
    lines
}

fn last_top(lines: &[Line]) -> usize {
    let mut used = 0;
    for (at, line) in lines.iter().enumerate().rev() {
        used += line.height();
        if used > ROOM {
            return at + 1;
        }
    }
    0
}

#[derive(Debug, Default)]
pub(super) struct SystemPage {
    top: usize,
    /// The furthest the page can scroll. It is learned when the page is
    /// drawn, because only then are the values, and so the lines, known.
    last_top: Cell<usize>,
}

impl SystemPage {
    pub(super) fn handle_input(&mut self, event: InputEvent) -> bool {
        let last = self.last_top.get();
        self.top = match event {
            InputEvent::Up { .. } => self.top.saturating_sub(1),
            InputEvent::Down { .. } => (self.top + 1).min(last),
            InputEvent::PageUp { .. } | InputEvent::Home { .. } => 0,
            InputEvent::PageDown { .. } | InputEvent::End { .. } => last,
            _ => return false,
        };
        true
    }

    pub(super) fn render(&self, p: &mut Painter<'_>, rom: &SystemRom, catalog: &Catalog) {
        let lines = lines(rom, catalog);
        let last = last_top(&lines);
        self.last_top.set(last);
        let top = self.top.min(last);
        p.heading(14, &ui_text::get("credits-system-title"));
        let mut y = TOP;
        for line in lines.iter().skip(top) {
            if y + line.height() > TOP + ROOM {
                break;
            }
            match line {
                Line::Fact(label, value) => {
                    p.text_clipped(LABEL_X, y, label, 13, DIM);
                    p.text_clipped(VALUE_X, y, value, 31, TEXT);
                }
                Line::Gap => {}
                Line::Palette => {
                    p.text(LABEL_X, y, &ui_text::get("credits-system-palette"), DIM);
                    if let Some(palette) = catalog.find("Zughy 32", Category::Art) {
                        p.text(VALUE_X, y, &palette.name, TEXT);
                    }
                    for index in 0..SWATCH_COLUMNS * SWATCH_ROWS {
                        let (column, row) = (index % SWATCH_COLUMNS, index / SWATCH_COLUMNS);
                        p.fill(
                            LABEL_X + column as i32 * SWATCH_PITCH_X,
                            y + ROW + row as i32 * SWATCH_PITCH_Y,
                            10,
                            9,
                            index as u8,
                        );
                    }
                }
            }
            y += line.height();
        }
        p.scrollbar(
            (316, TOP, ROOM),
            (top, lines.len().saturating_sub(last).max(1), lines.len()),
            (super::paint::BAR, DIM),
        );
        let hint = ui_text::get("credits-hint-system");
        p.status_bar(&hint, &ui_text::get("credits-tab-system"));
    }
}
