//! The editor workspace: a tab bar over one view per cartridge resource.
//!
//! The workspace works on the loaded cartridge without starting it. The
//! system lends it the cartridge for each tick and frame; the workspace
//! keeps only its own UI state, so the selected tab survives trips to the
//! shell.

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::{BitmapFont, FontMetrics};
use glaux_machine::framebuffer::{Framebuffer, WIDTH};
use glaux_video::draw::{clear, draw_text, draw_text_scaled, fill_rect};

use super::{ACCENT, BACKGROUND, DIM, PALE, TEXT};
use crate::cartridge::LoadedCartridge;

/// Fill of the tab bar, as a system palette index.
const BAR: u8 = 12;
/// Space between a tab's label and its edges.
const TAB_PAD: i32 = 3;
/// Gap between adjacent tabs, and before the first one.
const TAB_GAP: i32 = 1;
/// Minimum space between the last tab and the cartridge name.
const NAME_GAP: i32 = 8;
/// Fewest cells worth showing of a truncated cartridge name.
const MIN_NAME_CELLS: usize = 4;
/// Marks a cartridge name cut short to fit the bar.
const ELLIPSIS: &str = "..";

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

    pub fn label(self) -> &'static str {
        match self {
            Self::Code => "Code",
            Self::Sprites => "Sprites",
            Self::Map => "Map",
            Self::Sfx => "SFX",
            Self::Music => "Music",
        }
    }

    /// What the view will edit, shown while it has no editor.
    fn purpose(self) -> &'static str {
        match self {
            Self::Code => "Edit the cartridge's Luau scripts.",
            Self::Sprites => "Draw object tiles and their palettes.",
            Self::Map => "Lay out tiles into maps.",
            Self::Sfx => "Shape sound effects for the PSG.",
            Self::Music => "Arrange sound effects into songs.",
        }
    }

    fn index(self) -> usize {
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
    fn contains(self, point: Point) -> bool {
        let (x, y) = (i32::from(point.x), i32::from(point.y));
        (self.x..self.x + self.width).contains(&x) && (self.y..self.y + self.height).contains(&y)
    }
}

/// Where the workspace chrome sits for one font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    cell_w: i32,
    cell_h: i32,
}

impl Layout {
    fn new(metrics: FontMetrics) -> Self {
        Self {
            cell_w: i32::from(metrics.cell_width()),
            cell_h: i32::from(metrics.cell_height()),
        }
    }

    /// Height of the tab bar: one text row and a pixel above and below.
    fn bar_height(self) -> i32 {
        self.cell_h + 2
    }

    /// Each tab's clickable plate, left to right. Rendering and hit-testing
    /// both use these.
    fn tabs(self) -> [(Tab, Rect); 5] {
        let mut x = TAB_GAP;
        Tab::ALL.map(|tab| {
            let width = text_width(tab.label(), self.cell_w) + 2 * TAB_PAD;
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

    fn tab_at(self, point: Point) -> Option<Tab> {
        self.tabs()
            .into_iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(tab, _)| tab)
    }

    /// Cells available to the cartridge name, right of the last tab.
    fn name_cells(self) -> usize {
        let [.., (_, last)] = self.tabs();
        let start = last.x + last.width + NAME_GAP;
        let space = WIDTH as i32 - TAB_GAP - TAB_PAD - start;
        usize::try_from(space / self.cell_w.max(1)).unwrap_or(0)
    }
}

fn text_width(text: &str, cell_w: i32) -> i32 {
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX) * cell_w
}

/// `name` as it fits in `cells` character cells: whole, cut short with
/// [`ELLIPSIS`], or `None` when too little of it would show.
fn fit_name(name: &str, cells: usize) -> Option<String> {
    let len = name.chars().count();
    if len <= cells {
        return Some(name.to_owned());
    }
    let kept = cells.checked_sub(ELLIPSIS.len())?;
    (kept >= MIN_NAME_CELLS).then(|| name.chars().take(kept).chain(ELLIPSIS.chars()).collect())
}

/// The workspace's UI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    layout: Layout,
    tab: Tab,
    /// The logical pixel under the pointer, while it is over the screen.
    hover: Option<Point>,
}

impl Workspace {
    /// A workspace on the Code tab, laid out for a font with `metrics`.
    pub fn new(metrics: FontMetrics) -> Self {
        Self {
            layout: Layout::new(metrics),
            tab: Tab::default(),
            hover: None,
        }
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// Forgets pointer state, keeping the selected tab. Called whenever the
    /// workspace is entered or left, so no hover or press outlives a visit.
    pub fn reset_pointer(&mut self) {
        self.hover = None;
    }

    /// Applies `event`. Ctrl+Tab and Ctrl+Shift+Tab cycle the tabs, and a
    /// primary press on a tab selects it. Plain Tab and text are left for
    /// the editors.
    pub fn handle_input(&mut self, event: InputEvent) {
        match event {
            InputEvent::NextView => self.tab = self.tab.next(),
            InputEvent::PreviousView => self.tab = self.tab.previous(),
            InputEvent::PointerMove(position) => self.hover = position,
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
            } => {
                self.hover = Some(at);
                if let Some(tab) = self.layout.tab_at(at) {
                    self.tab = tab;
                }
            }
            _ => {}
        }
    }

    /// Draws the tab bar and the selected view for `cartridge`.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        cartridge: &LoadedCartridge,
    ) {
        let layout = self.layout;
        clear(framebuffer, BACKGROUND);
        fill_rect(framebuffer, 0, 0, WIDTH as i32, layout.bar_height(), BAR);

        let hovered = self.hover.and_then(|at| layout.tab_at(at));
        for (tab, rect) in layout.tabs() {
            let label = if tab == self.tab {
                // The active plate takes the body's color so it reads as
                // the front of the view below it.
                fill_rect(
                    framebuffer,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    BACKGROUND,
                );
                fill_rect(framebuffer, rect.x, rect.y, rect.width, 1, ACCENT);
                TEXT
            } else if hovered == Some(tab) {
                PALE
            } else {
                DIM
            };
            draw_text(
                framebuffer,
                font,
                tab.label(),
                rect.x + TAB_PAD,
                rect.y + 1,
                label,
            );
        }

        let name = &cartridge.manifest().cartridge.name;
        if let Some(name) = fit_name(name, layout.name_cells()) {
            let x = WIDTH as i32 - TAB_GAP - TAB_PAD - text_width(&name, layout.cell_w);
            draw_text(framebuffer, font, &name, x, 1, PALE);
        }

        self.render_placeholder(framebuffer, font);
    }

    /// The selected view's stand-in until its editor exists.
    fn render_placeholder(&self, framebuffer: &mut Framebuffer, font: &BitmapFont) {
        let Layout { cell_w, cell_h } = self.layout;
        let x = cell_w;
        let mut y = self.layout.bar_height() + cell_h;
        draw_text_scaled(framebuffer, font, self.tab.label(), x, y, 2, ACCENT);
        y += 3 * cell_h;
        for (text, color) in [
            (self.tab.purpose(), TEXT),
            ("Not available yet.", DIM),
            ("", DIM),
            ("Esc shell   Ctrl+Tab switch tab", DIM),
        ] {
            draw_text(framebuffer, font, text, x, y, color);
            y += cell_h;
        }
    }
}

#[cfg(test)]
pub(super) mod tests_support {
    use super::*;

    /// Where `tab` sits in the bar for a font with `metrics`.
    pub fn tab_rect(tab: Tab, metrics: FontMetrics) -> Rect {
        Layout::new(metrics).tabs()[tab.index()].1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn layout() -> Layout {
        Layout::new(FontMetrics::SYSTEM)
    }

    #[test]
    fn tabs_cycle_both_ways_and_wrap() {
        let mut tab = Tab::Code;
        let mut seen = vec![tab];
        for _ in 0..Tab::ALL.len() {
            tab = tab.next();
            seen.push(tab);
        }
        assert_eq!(
            seen,
            [
                Tab::Code,
                Tab::Sprites,
                Tab::Map,
                Tab::Sfx,
                Tab::Music,
                Tab::Code
            ]
        );
        assert_eq!(Tab::Code.previous(), Tab::Music);
        for tab in Tab::ALL {
            assert_eq!(tab.next().previous(), tab);
        }
    }

    #[test]
    fn tabs_sit_in_the_bar_left_to_right_without_overlapping() {
        let tabs = layout().tabs();
        for pair in tabs.windows(2) {
            let ((_, a), (_, b)) = (pair[0], pair[1]);
            assert!(a.x + a.width < b.x, "{a:?} {b:?}");
        }
        for (tab, rect) in tabs {
            assert_eq!((rect.y, rect.height), (0, layout().bar_height()), "{tab:?}");
            let centre = Point::new((rect.x + rect.width / 2) as u16, 5);
            assert_eq!(layout().tab_at(centre), Some(tab));
        }
    }

    #[test]
    fn gaps_and_the_view_below_the_bar_hit_no_tab() {
        let tabs = layout().tabs();
        let (_, first) = tabs[0];
        let gap = Point::new((first.x + first.width) as u16, 2);
        assert_eq!(layout().tab_at(gap), None);
        let below = Point::new((first.x + 2) as u16, layout().bar_height() as u16);
        assert_eq!(layout().tab_at(below), None);
        assert_eq!(layout().tab_at(Point::new(318, 2)), None);
    }

    #[test]
    fn the_bar_stays_compact() {
        assert_eq!(layout().bar_height(), 12);
        let [.., (_, last)] = layout().tabs();
        assert!(last.x + last.width < WIDTH as i32 / 2 + 20);
        assert!(layout().name_cells() >= 16);
    }

    #[test]
    fn long_names_are_cut_to_fit_and_tiny_space_hides_them() {
        assert_eq!(fit_name("Pong", 10).as_deref(), Some("Pong"));
        assert_eq!(fit_name("Glaux Owl", 9).as_deref(), Some("Glaux Owl"));
        assert_eq!(
            fit_name("Glaux Owl Adventure", 9).as_deref(),
            Some("Glaux O..")
        );
        assert_eq!(fit_name("Glaux Owl Adventure", 5), None);
        assert_eq!(fit_name("ÁÉÍÓÚáéíóú", 8).as_deref(), Some("ÁÉÍÓÚá.."));
    }
}
