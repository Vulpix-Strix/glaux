//! The editor workspace: a tab bar over one view per cartridge resource.
//!
//! The workspace works on the loaded cartridge without starting it. The
//! system lends it the cartridge and the documents the editors change for
//! each tick and frame; the workspace keeps only its own UI state, so the
//! selected tab and each editor's view survive tab switches and trips to the
//! shell.

mod sprites;

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::{BitmapFont, FontMetrics};
use glaux_machine::framebuffer::{Framebuffer, WIDTH};
use glaux_machine::ink_image::INK_SLOTS;
use glaux_machine::palette::Palette;
use glaux_machine::system_rom::{Cursor, SystemIcon, SystemUi};
use glaux_video::draw::{clear, draw_ink_image, draw_text, draw_text_scaled, fill_rect};
use glaux_video::preview::ObjPreviewPalette;

use super::sprite_document::SpriteDocument;
use super::{ACCENT, BACKGROUND, DIM, PALE, TEXT};
use crate::cartridge::LoadedCartridge;
use crate::rom::ICON_SIZE;
use sprites::SpriteEditor;
pub use sprites::SpriteSource;

/// Entries of the workspace's frame palette that keep the system palette's
/// colors. Every UI color index is below it; the cartridge's object colors
/// are placed after it.
const SYSTEM_COLORS: usize = 32;

/// Fill of the tab bar, as a system palette index.
const BAR: u8 = 12;
/// Icon outlines and the pointer's outline.
const OUTLINE: u8 = 11;
/// Icon shading on an idle tab.
const IDLE_SHADE: u8 = 13;
/// Icon shading while a tab is hovered.
const HOVER_SHADE: u8 = 19;
/// Space between a tab's icon and the sides of its plate.
const PLATE_PAD: i32 = 1;
/// Gap between adjacent tabs, and before the first one.
const TAB_GAP: i32 = 1;
/// Space between the last tab and the tab name hint.
const HINT_GAP: i32 = 4;
/// Minimum space between the hint and the cartridge name.
const NAME_GAP: i32 = 8;
/// Space between the cartridge name and the right edge.
const NAME_PAD: i32 = 4;
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

    fn icon(self) -> SystemIcon {
        match self {
            Self::Code => SystemIcon::Code,
            Self::Sprites => SystemIcon::Sprites,
            Self::Map => SystemIcon::Map,
            Self::Sfx => SystemIcon::Sfx,
            Self::Music => SystemIcon::Music,
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

    fn right(self) -> i32 {
        self.x + self.width
    }

    fn bottom(self) -> i32 {
        self.y + self.height
    }

    /// The rectangle shrunk by `by` pixels on every side, or grown when
    /// `by` is negative.
    fn inset(self, by: i32) -> Self {
        Self {
            x: self.x + by,
            y: self.y + by,
            width: self.width - 2 * by,
            height: self.height - 2 * by,
        }
    }

    #[cfg(test)]
    fn contains_xy(self, x: i32, y: i32) -> bool {
        (self.x..self.right()).contains(&x) && (self.y..self.bottom()).contains(&y)
    }

    #[cfg(test)]
    fn overlaps(self, other: Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
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

    /// Height of the tab bar: an icon, or one text row with a pixel above
    /// and below, whichever is taller.
    fn bar_height(self) -> i32 {
        (self.cell_h + 2).max(ICON_SIZE as i32)
    }

    /// Each tab's clickable plate, left to right. Rendering and hit-testing
    /// both use these.
    fn tabs(self) -> [(Tab, Rect); 5] {
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

    fn tab_at(self, point: Point) -> Option<Tab> {
        self.tabs()
            .into_iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(tab, _)| tab)
    }

    /// Where the hint naming a tab starts, right of the last tab.
    fn hint_x(self) -> i32 {
        let [.., (_, last)] = self.tabs();
        last.right() + HINT_GAP
    }

    /// Text row origin within the bar.
    fn text_y(self) -> i32 {
        (self.bar_height() - self.cell_h) / 2 + 1
    }

    /// Cells available to the cartridge name, right of the longest hint.
    fn name_cells(self) -> usize {
        let longest = Tab::ALL
            .iter()
            .map(|tab| text_width(tab.label(), self.cell_w))
            .max()
            .unwrap_or(0);
        let start = self.hint_x() + longest + NAME_GAP;
        let space = WIDTH as i32 - NAME_PAD - start;
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

/// Draws `cursor` with its hotspot on `pointer`, if any. Drawn over the
/// finished frame, it covers everything below it.
pub fn draw_cursor(framebuffer: &mut Framebuffer, cursor: &Cursor, pointer: Option<Point>) {
    let Some(at) = pointer else {
        return;
    };
    let (hot_x, hot_y) = cursor.hotspot;
    draw_ink_image(
        framebuffer,
        &cursor.image,
        i32::from(at.x) - i32::from(hot_x),
        i32::from(at.y) - i32::from(hot_y),
        &[OUTLINE, TEXT, TEXT],
    );
}

/// How the workspace shows the cartridge's colors, prepared when it opens:
/// where the frame palette holds each master color the object palettes use,
/// or why the cartridge's video state can't be shown.
#[derive(Debug)]
// Boxed: a frame palette would make every system state as large as this
// one.
pub struct WorkspaceVideo(Result<Box<ObjPreviewPalette>, String>);

impl WorkspaceVideo {
    /// The video state could not be read, for the reason in `message`.
    pub fn unavailable(message: String) -> Self {
        tracing::warn!(%message, "cartridge video unavailable in the workspace");
        Self(Err(message))
    }

    /// Whether the editors can show the cartridge's video state.
    pub fn is_ready(&self) -> bool {
        self.0.is_ok()
    }

    /// The palette the workspace's frames are presented with, if the
    /// cartridge's video assets loaded.
    pub fn frame_palette(&self) -> Option<&Palette> {
        self.0.as_ref().ok().map(|preview| preview.palette())
    }

    /// What the sprite editor shows of `document`, or why it shows nothing.
    pub fn source<'a>(
        &'a self,
        document: Option<&'a SpriteDocument>,
    ) -> Result<SpriteSource<'a>, &'a str> {
        let preview = self.0.as_deref().map_err(String::as_str)?;
        // The system only prepares a preview from a document it keeps.
        let document = document.ok_or("The cartridge's sprites are not loaded.")?;
        Ok(SpriteSource {
            obj: document.obj(),
            master: &document.video().palette,
            preview,
            modified: document.is_modified(),
        })
    }
}

/// The workspace's UI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    layout: Layout,
    tab: Tab,
    sprites: SpriteEditor,
    /// The system palette, whose first [`SYSTEM_COLORS`] entries the UI is
    /// drawn with.
    system_palette: Palette,
}

impl Workspace {
    /// A workspace on the Code tab, laid out for a font with `metrics`
    /// and drawn in `system_palette`.
    pub fn new(metrics: FontMetrics, system_palette: Palette) -> Self {
        let layout = Layout::new(metrics);
        Self {
            layout,
            tab: Tab::default(),
            sprites: SpriteEditor::new(sprites::Metrics {
                cell_w: layout.cell_w,
                cell_h: layout.cell_h,
                top: layout.bar_height(),
            }),
            system_palette,
        }
    }

    /// Prepares to show `document` in the workspace's frame palette. The
    /// object palettes are read-only, so this holds for as long as the
    /// workspace stays open.
    pub fn prepare_video(&self, document: &SpriteDocument) -> WorkspaceVideo {
        let video = document.video();
        match ObjPreviewPalette::new(
            &self.system_palette,
            SYSTEM_COLORS,
            &video.palette,
            video.obj.palettes(),
        ) {
            Ok(preview) => WorkspaceVideo(Ok(Box::new(preview))),
            Err(err) => WorkspaceVideo::unavailable(err.to_string()),
        }
    }

    #[cfg(test)]
    pub fn sprites(&self) -> &SpriteEditor {
        &self.sprites
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// Applies `event`, letting the sprite editor change `sprites`.
    /// Ctrl+Tab and Ctrl+Shift+Tab cycle the tabs, and a primary press on a
    /// tab selects it. Everything else goes to the open view's editor, if it
    /// has one.
    pub fn handle_input(&mut self, event: InputEvent, sprites: Option<&mut SpriteDocument>) {
        match event {
            InputEvent::NextView => self.switch_to(self.tab.next()),
            InputEvent::PreviousView => self.switch_to(self.tab.previous()),
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
            } if self.layout.tab_at(at).is_some() => {
                if let Some(tab) = self.layout.tab_at(at) {
                    self.switch_to(tab);
                }
            }
            _ if self.tab == Tab::Sprites => self.sprites.handle_input(event, sprites),
            _ => {}
        }
    }

    /// Ends any drag in progress, which must not outlive the view or the
    /// visit it started in.
    pub fn end_stroke(&mut self) {
        self.sprites.end_stroke();
    }

    fn switch_to(&mut self, tab: Tab) {
        if tab != self.tab {
            self.end_stroke();
        }
        self.tab = tab;
    }

    /// Draws the tab bar and the selected view for `cartridge`, whose
    /// sprites the editor shows from `sprites`, with what is under
    /// `pointer` highlighted. The pointer itself is not drawn; see
    /// [`draw_cursor`].
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        ui: &SystemUi,
        cartridge: &LoadedCartridge,
        sprites: Result<SpriteSource<'_>, &str>,
        pointer: Option<Point>,
    ) {
        let layout = self.layout;
        clear(framebuffer, BACKGROUND);
        fill_rect(framebuffer, 0, 0, WIDTH as i32, layout.bar_height(), BAR);

        let hovered = pointer.and_then(|at| layout.tab_at(at));
        for (tab, rect) in layout.tabs() {
            let active = tab == self.tab;
            let inks: [u8; INK_SLOTS] = if active {
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
                [OUTLINE, ACCENT, TEXT]
            } else if hovered == Some(tab) {
                [OUTLINE, HOVER_SHADE, PALE]
            } else {
                [OUTLINE, IDLE_SHADE, DIM]
            };
            let icon_y = rect.y + (rect.height - ICON_SIZE as i32) / 2;
            draw_ink_image(
                framebuffer,
                ui.icon(tab.icon()),
                rect.x + PLATE_PAD,
                icon_y,
                &inks,
            );
            if active {
                fill_rect(framebuffer, rect.x, rect.y, rect.width, 1, ACCENT);
            }
        }

        // Icons alone don't say which editor is which: name the one under
        // the pointer, or else the open one.
        let (hint, color) = match hovered {
            Some(tab) => (tab, PALE),
            None => (self.tab, DIM),
        };
        draw_text(
            framebuffer,
            font,
            hint.label(),
            layout.hint_x(),
            layout.text_y(),
            color,
        );

        let name = &cartridge.manifest().cartridge.name;
        if let Some(name) = fit_name(name, layout.name_cells()) {
            let x = WIDTH as i32 - NAME_PAD - text_width(&name, layout.cell_w);
            draw_text(framebuffer, font, &name, x, layout.text_y(), PALE);
        }

        match self.tab {
            Tab::Sprites => self.sprites.render(framebuffer, font, ui, sprites, pointer),
            _ => self.render_placeholder(framebuffer, font),
        }
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

    /// Top-left corner of the tab name hint's first character cell.
    pub fn hint_origin(metrics: FontMetrics) -> (i32, i32) {
        let layout = Layout::new(metrics);
        (layout.hint_x(), layout.text_y())
    }

    /// Where the sprite editor draws the enlarged tile.
    pub fn sprite_canvas(workspace: &Workspace) -> Rect {
        workspace.sprites.canvas_rect()
    }

    /// The middle of tile pixel `(x, y)` on the sprite editor's canvas.
    pub fn sprite_pixel(workspace: &Workspace, x: i32, y: i32) -> Point {
        let canvas = sprite_canvas(workspace);
        let zoom = canvas.width / glaux_machine::sprite::OBJ_TILE_WIDTH as i32;
        Point::new(
            (canvas.x + x * zoom + zoom / 2) as u16,
            (canvas.y + y * zoom + zoom / 2) as u16,
        )
    }

    /// The leftmost column the cartridge name may use.
    pub fn name_start(metrics: FontMetrics) -> i32 {
        let layout = Layout::new(metrics);
        WIDTH as i32 - NAME_PAD - layout.name_cells() as i32 * layout.cell_w
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
        assert_eq!(layout().bar_height(), 16);
        for (tab, rect) in layout().tabs() {
            assert_eq!((rect.width, rect.height), (18, 16), "{tab:?}");
        }
        let [.., (_, last)] = layout().tabs();
        assert!(last.right() < WIDTH as i32 / 3);
        assert!(layout().name_cells() >= 24);
    }

    #[test]
    fn every_hint_fits_between_the_tabs_and_the_name() {
        let layout = layout();
        let [.., (_, last)] = layout.tabs();
        assert!(layout.hint_x() > last.right());
        let name_start = WIDTH as i32 - NAME_PAD - layout.name_cells() as i32 * layout.cell_w;
        for tab in Tab::ALL {
            let end = layout.hint_x() + text_width(tab.label(), layout.cell_w);
            assert!(end + NAME_GAP <= name_start, "{tab:?}");
        }
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
