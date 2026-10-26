//! The tab bar: which views there are, where the bar draws them and what it
//! calls them.

use super::{
    DOCS_MARGIN, DOCS_PAD, ELLIPSIS, HINT_GAP, MIN_NAME_CELLS, NAME_GAP, NAME_PAD, PLATE_PAD,
    TAB_GAP,
};
use crate::rom::ICON_SIZE;
use crate::ui_text;
use glaux_i18n::Translator;
use glaux_input::Point;
use glaux_machine::font::{BitmapFont, FontMetrics};
use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::system_rom::SystemIcon;

/// One editor view of the workspace.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Code,
    Sprites,
    Map,
    Sfx,
    Music,
}

impl Tab {
    pub const ALL: [Self; 5] = [Self::Code, Self::Sprites, Self::Map, Self::Sfx, Self::Music];

    /// The id of the message holding the tab's name.
    pub fn message_id(self) -> &'static str {
        match self {
            Self::Code => "tab-code",
            Self::Sprites => "tab-sprites",
            Self::Map => "tab-map",
            Self::Sfx => "tab-sfx",
            Self::Music => "tab-music",
        }
    }

    pub(super) fn icon(self) -> SystemIcon {
        match self {
            Self::Code => SystemIcon::Code,
            Self::Sprites => SystemIcon::Sprites,
            Self::Map => SystemIcon::Map,
            Self::Sfx => SystemIcon::Sfx,
            Self::Music => SystemIcon::Music,
        }
    }

    /// What the view will edit, shown while it has no editor.
    pub(super) fn purpose(self) -> String {
        match self {
            Self::Code => ui_text::get("workspace-edit-the-cartridge-s-luau-and-rhai-scripts"),
            Self::Sprites => ui_text::get("workspace-draw-object-tiles-and-their-palettes"),
            Self::Map => ui_text::get("workspace-lay-out-tiles-into-maps"),
            Self::Sfx => ui_text::get("workspace-shape-sound-effects-for-the-psg"),
            Self::Music => ui_text::get("workspace-arrange-sound-effects-into-songs"),
        }
    }

    pub(super) fn index(self) -> usize {
        self as usize
    }

    /// The next tab, wrapping from the last to the first.
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    /// The previous tab, wrapping from the first to the last.
    pub fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// A screen rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub(super) fn contains(self, point: Point) -> bool {
        let (x, y) = (i32::from(point.x), i32::from(point.y));
        (self.x..self.x + self.width).contains(&x) && (self.y..self.y + self.height).contains(&y)
    }

    pub(super) fn right(self) -> i32 {
        self.x + self.width
    }

    pub(super) fn bottom(self) -> i32 {
        self.y + self.height
    }

    /// The rectangle shrunk by `by` pixels on every side, or grown when
    /// `by` is negative.
    pub(super) fn inset(self, by: i32) -> Self {
        Self {
            x: self.x + by,
            y: self.y + by,
            width: self.width - 2 * by,
            height: self.height - 2 * by,
        }
    }

    #[cfg(test)]
    pub(crate) fn contains_xy(self, x: i32, y: i32) -> bool {
        (self.x..self.right()).contains(&x) && (self.y..self.bottom()).contains(&y)
    }

    #[cfg(test)]
    pub(super) fn overlaps(self, other: Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

/// Where the workspace chrome sits for one font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Layout {
    pub(super) cell_w: i32,
    pub(super) cell_h: i32,
}

impl Layout {
    pub(super) const fn new(metrics: FontMetrics) -> Self {
        Self {
            cell_w: metrics.cell_width() as i32,
            cell_h: metrics.cell_height() as i32,
        }
    }

    /// Height of the tab bar: an icon, or one text row with a pixel above
    /// and below, whichever is taller.
    pub(super) const fn bar_height(self) -> i32 {
        let text = self.cell_h + 2;
        let icon = ICON_SIZE as i32;
        if text > icon { text } else { icon }
    }

    /// The cell size and top of the Code view, below the tab bar.
    pub(super) const fn code_metrics(self) -> super::code::Metrics {
        super::code::Metrics {
            cell_w: self.cell_w,
            cell_h: self.cell_h,
            top: self.bar_height(),
        }
    }

    /// The cell size and top of the documentation browser, below the tab
    /// bar.
    pub(super) const fn docs_metrics(self) -> super::docs::Metrics {
        super::docs::Metrics {
            cell_w: self.cell_w,
            cell_h: self.cell_h,
            top: self.bar_height(),
        }
    }

    /// The cell size and top of the sprite editor, below the tab bar.
    pub(super) const fn sprite_metrics(self) -> super::sprites::Metrics {
        super::sprites::Metrics {
            cell_w: self.cell_w,
            cell_h: self.cell_h,
            top: self.bar_height(),
        }
    }

    /// Each tab's clickable plate, left to right. Rendering and hit-testing
    /// both use these.
    pub(super) fn tabs(self) -> [(Tab, Rect); 5] {
        let width = ICON_SIZE as i32 + 2 * PLATE_PAD;
        let mut x = TAB_GAP;
        Tab::ALL.map(|tab| {
            let rect = Rect {
                x,
                y: 0,
                width,
                height: self.bar_height(),
            };
            x += width + TAB_GAP;
            (tab, rect)
        })
    }

    pub(super) fn tab_at(self, point: Point) -> Option<Tab> {
        self.tabs()
            .into_iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(tab, _)| tab)
    }

    /// Where the hint naming a tab starts, right of the last tab.
    pub(super) fn hint_x(self) -> i32 {
        let [.., (_, last)] = self.tabs();
        last.right() + HINT_GAP
    }

    /// The screen below the tab bar, where the views and dialogs go.
    pub(super) fn view_area(self) -> Rect {
        let top = self.bar_height();
        Rect {
            x: 0,
            y: top,
            width: WIDTH as i32,
            height: HEIGHT as i32 - top,
        }
    }

    /// Text row origin within the bar.
    pub(super) fn text_y(self) -> i32 {
        (self.bar_height() - self.cell_h) / 2 + 1
    }

    /// The Docs button at the bar's right end, showing `label`.
    pub(super) fn docs_button(self, label: &str, font: &BitmapFont) -> Rect {
        let width = text_width(font, label) + 2 * DOCS_PAD;
        Rect {
            x: WIDTH as i32 - DOCS_MARGIN - width,
            y: 0,
            width,
            height: self.bar_height(),
        }
    }

    /// Where the cartridge name ends: left of the Docs button.
    pub(super) fn name_right(self, labels: &TabLabels, font: &BitmapFont) -> i32 {
        self.docs_button(labels.docs(), font).x - NAME_PAD
    }

    /// Cells available to the cartridge name, right of the longest hint.
    pub(super) fn name_cells(self, labels: &TabLabels, font: &BitmapFont) -> usize {
        let longest = i32::try_from(labels.longest_cells()).unwrap_or(0) * self.cell_w;
        let start = self.hint_x() + longest + NAME_GAP;
        let space = self.name_right(labels, font) - start;
        usize::try_from(space / self.cell_w.max(1)).unwrap_or(0)
    }
}

/// The tabs' names in the interface language, resolved once because the
/// language does not change while the workspace exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabLabels {
    tabs: [String; Tab::ALL.len()],
    /// The Docs button's label, and the hint naming it.
    docs: String,
    docs_hint: String,
}

impl TabLabels {
    pub fn new(text: &Translator) -> Self {
        Self {
            tabs: Tab::ALL.map(|tab| text.get(tab.message_id()).into_owned()),
            docs: text.get("docs-button").into_owned(),
            docs_hint: text.get("docs-button-hint").into_owned(),
        }
    }

    pub fn get(&self, tab: Tab) -> &str {
        &self.tabs[tab.index()]
    }

    /// The Docs button's label.
    pub(super) fn docs(&self) -> &str {
        &self.docs
    }

    /// The hint shown while the pointer is over the Docs button.
    pub(super) fn docs_hint(&self) -> &str {
        &self.docs_hint
    }

    /// The width in cells of the longest name the hint area shows.
    pub(super) fn longest_cells(&self) -> usize {
        self.tabs
            .iter()
            .chain([&self.docs_hint])
            .map(|label| label.chars().count())
            .max()
            .unwrap_or(0)
    }
}

/// Width in pixels of `text` drawn on one line with `font`.
pub(super) fn text_width(font: &BitmapFont, text: &str) -> i32 {
    let cells = i32::try_from(font.text_cells(text)).unwrap_or(i32::MAX);
    cells.saturating_mul(i32::from(font.metrics().cell_width()))
}

/// The longest start of `text` that takes at most `cells` of `font`'s
/// cells.
pub(crate) fn take_cells<'a>(font: &BitmapFont, text: &'a str, cells: usize) -> &'a str {
    let mut used = 0;
    for (at, ch) in text.char_indices() {
        used += font.cells(ch);
        if used > cells {
            return &text[..at];
        }
    }
    text
}

/// `name` as it fits in `cells` of `font`'s cells: whole, cut short with
/// [`ELLIPSIS`], or `None` when too little of it would show.
pub(super) fn fit_name(font: &BitmapFont, name: &str, cells: usize) -> Option<String> {
    if font.text_cells(name) <= cells {
        return Some(name.to_owned());
    }
    let room = cells.checked_sub(font.text_cells(ELLIPSIS))?;
    let kept = take_cells(font, name, room);
    (font.text_cells(kept) >= MIN_NAME_CELLS).then(|| format!("{kept}{ELLIPSIS}"))
}
