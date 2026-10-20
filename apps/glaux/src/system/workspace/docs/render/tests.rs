//! Pixel checks of the browser's drawing. The font has blank glyphs, so only
//! what is drawn around the text shows: bands, rules, boxes and underlines.

use glaux_docs::{DocId, Library};
use glaux_i18n::FallbackChain;
use glaux_input::InputEvent;
use glaux_machine::framebuffer::Framebuffer;
use pretty_assertions::assert_eq;

use super::*;
use crate::system::test_font::LAYOUT_FONT;
use crate::system::workspace::HOVER_SHADE;
use crate::system::workspace::docs::layout::Layout;
use crate::system::workspace::docs::{LanguageFilter, Metrics};

const METRICS: Metrics = Metrics {
    cell_w: 6,
    cell_h: 12,
    top: 14,
};

const SHOWCASE: &str = "# Showcase\n\nIntro with [Beta](glaux://guide/b) inside.\n\n## Section\n\nBody text.\n\n### Small\n\n| Name | Meaning |\n|---|---|\n| a | first |\n| b | second |\n\n```luau\nlocal x = 1\nprint(x)\n```\n";

struct Scene {
    library: Library,
    chain: FallbackChain,
    browser: DocsBrowser,
}

impl Scene {
    fn new() -> Self {
        Self::with_pages(0)
    }

    fn with_pages(extra: usize) -> Self {
        let mut files = vec![
            (
                "en/guide/index.md".to_owned(),
                "# Guide\n\nStart.\n".as_bytes().to_vec(),
            ),
            ("en/guide/show.md".to_owned(), SHOWCASE.as_bytes().to_vec()),
            (
                "en/guide/long.md".to_owned(),
                format!("# Long\n\n{}", "A paragraph of text.\n\n".repeat(40)).into_bytes(),
            ),
            (
                "en/guide/both.md".to_owned(),
                b"# Both\n\nCommon.\n\n### Luau\n\nOnly Luau.\n\n### Rhai\n\nOnly Rhai.\n".to_vec(),
            ),
            (
                "en/guide/b.md".to_owned(),
                "# Beta\n\nSecond.\n".as_bytes().to_vec(),
            ),
        ];
        files.extend((0..extra).map(|n| {
            (
                format!("en/guide/p{n:02}.md"),
                format!("# Page {n:02}\n\nText.\n").into_bytes(),
            )
        }));
        let library = Library::from_files(files);
        let chain = FallbackChain::english();
        let mut scene = Self {
            library,
            chain,
            browser: DocsBrowser::new(METRICS, LAYOUT_FONT.clone()),
        };
        let mut browser = scene.browser.clone();
        browser.show(scene.source());
        scene.browser = browser;
        scene.open("guide/show", false);
        scene
    }

    fn source(&self) -> Source<'_> {
        Source {
            library: &self.library,
            chain: &self.chain,
        }
    }

    fn open(&mut self, id: &str, new_tab: bool) {
        let mut browser = self.browser.clone();
        browser.open(self.source(), DocId::parse(id).unwrap(), None, new_tab);
        self.browser = browser;
    }

    fn send(&mut self, event: InputEvent) {
        let mut browser = self.browser.clone();
        browser.handle_input(event, self.source());
        self.browser = browser;
    }

    fn frame(&self) -> Framebuffer {
        self.frame_with(None)
    }

    fn frame_with(&self, pointer: Option<Point>) -> Framebuffer {
        let mut framebuffer = Framebuffer::new();
        framebuffer.clear(BACKGROUND);
        self.browser
            .render(&mut framebuffer, &LAYOUT_FONT, self.source(), pointer, None);
        framebuffer
    }

    fn layout(&self) -> &Layout {
        &self.browser.shown.as_ref().unwrap().layout
    }

    /// Where the page text starts, and the top and height of the first row
    /// `wanted` accepts.
    fn row(&self, wanted: impl Fn(&Row) -> bool) -> (Rect, i32, i32) {
        let text = self.browser.geometry.text(self.browser.nav_collapsed);
        let layout = self.layout();
        let mut y = text.y;
        for row in &layout.rows[self.browser.scroll()..] {
            let height = row.height(layout.cell_h);
            if wanted(row) {
                return (text, y, height);
            }
            y += height;
        }
        panic!("no such row");
    }

    /// Every row with a table, from the top.
    fn table_rows(&self) -> Vec<(i32, i32)> {
        let text = self.browser.geometry.text(self.browser.nav_collapsed);
        let layout = self.layout();
        let mut y = text.y;
        let mut found = Vec::new();
        for row in &layout.rows {
            let height = row.height(layout.cell_h);
            if row.deco.table.is_some() {
                found.push((y, height));
            }
            y += height;
        }
        found
    }
}

#[test]
fn the_page_header_is_a_band_across_the_text_width_only() {
    let scene = Scene::new();
    let frame = scene.frame();
    let text = scene.browser.geometry.text(false);
    let (_, y, height) = scene.row(|row| row.deco.fill == Some(BAR));
    assert_eq!(y, text.y, "the header is the first row");
    for (x, at) in [(text.x, y), (text.x + text.width - 1, y + height - 1)] {
        assert_eq!(frame.pixel(x, at), Some(BAR));
    }
    assert_eq!(frame.pixel(text.x - 1, y + 1), Some(BACKGROUND));
    assert_eq!(frame.pixel(text.x + text.width, y + 1), Some(BACKGROUND));
}

#[test]
fn a_section_heading_has_a_rule_under_it_and_a_small_one_does_not() {
    let scene = Scene::new();
    let frame = scene.frame();
    let (text, y, height) = scene.row(|row| row.deco.underline.is_some());
    let last = y + height - 1;
    for x in [text.x, text.x + text.width / 2, text.x + text.width - 1] {
        assert_eq!(frame.pixel(x, last), Some(BAR), "rule at {x}");
        assert_eq!(frame.pixel(x, last - 1), Some(BACKGROUND), "above the rule");
    }
    let rules = scene
        .layout()
        .rows
        .iter()
        .filter(|row| row.deco.underline.is_some())
        .count();
    assert_eq!(rules, 1, "the page has one section heading with a rule");
}

#[test]
fn a_table_has_a_banded_header_dim_edges_and_quiet_row_lines() {
    let scene = Scene::new();
    let frame = scene.frame();
    let text = scene.browser.geometry.text(false);
    let rows = scene.table_rows();
    assert_eq!(rows.len(), 3, "header and two rows");
    let layout = scene.layout();
    let table = layout
        .rows
        .iter()
        .find_map(|row| row.deco.table.clone())
        .unwrap();
    let left = text.x + FRAME_INSET;
    let right = text.x + table.width as i32 * METRICS.cell_w - FRAME_INSET - 1;
    let divider = text.x + table.dividers[0] as i32 * METRICS.cell_w - METRICS.cell_w / 2;
    let (header_y, _) = rows[0];
    let (second_y, second_h) = rows[1];
    let (last_y, last_h) = rows[2];
    let at = |x, y| frame.pixel(x, y);

    // The header: a band, a dim top edge, and dim sides and divider.
    assert_eq!(at(left + 2, header_y + 2), Some(BAR));
    assert_eq!(at(left + 2, header_y), Some(DIM));
    assert_eq!(at(left, header_y + 3), Some(DIM));
    assert_eq!(at(right, header_y + 3), Some(DIM));
    assert_eq!(at(divider, header_y + 3), Some(DIM));
    // Nothing is drawn outside the table's edges.
    assert_eq!(at(left - 1, header_y + 3), Some(BACKGROUND));
    assert_eq!(at(right + 1, header_y + 3), Some(BACKGROUND));

    // Body rows: no band, a quiet line between rows and a quiet divider.
    assert_eq!(at(left + 2, second_y + 4), Some(BACKGROUND));
    assert_eq!(at(left + 2, second_y), Some(BAR));
    assert_eq!(at(divider, second_y + 4), Some(BAR));
    // The last row closes the table with a dim edge.
    assert_eq!(at(left + 2, last_y + last_h - 1), Some(DIM));
    assert_eq!(at(left + 2, second_y + second_h - 1), Some(BACKGROUND));
}

#[test]
fn a_code_block_is_a_box_of_dim_lines_with_the_text_inside() {
    let scene = Scene::new();
    let frame = scene.frame();
    let text = scene.browser.geometry.text(false);
    let layout = scene.layout();
    let mut y = text.y;
    let mut boxed = Vec::new();
    for row in &layout.rows {
        let height = row.height(layout.cell_h);
        if let Some(frame) = row.deco.frame.filter(|frame| frame.boxed) {
            boxed.push((y, height, frame));
        }
        y += height;
    }
    assert_eq!(boxed.len(), 2, "two lines of code");
    let left = text.x + FRAME_INSET;
    let right = text.x + text.width - FRAME_INSET - 1;
    let (top_y, _, first) = boxed[0];
    let (last_y, last_h, last) = boxed[1];
    assert!(first.first && !first.last && last.last && !last.first);
    assert_eq!(first.ink, DIM);
    for x in [left, (left + right) / 2, right] {
        assert_eq!(frame.pixel(x, top_y), Some(DIM), "top edge at {x}");
        assert_eq!(
            frame.pixel(x, last_y + last_h - 1),
            Some(DIM),
            "bottom edge"
        );
    }
    // The sides run through both rows, and the inside is left alone.
    for (y, height, _) in &boxed {
        assert_eq!(frame.pixel(left, y + height / 2), Some(DIM));
        assert_eq!(frame.pixel(right, y + height / 2), Some(DIM));
    }
    assert_eq!(frame.pixel(left + 3, top_y + 4), Some(BACKGROUND));
    assert_eq!(frame.pixel(left - 1, top_y + 4), Some(BACKGROUND));
}

#[test]
fn a_link_is_underlined_and_the_focused_one_is_lit() {
    let mut scene = Scene::new();
    let (text, y, _) = scene.row(|row| row.runs.iter().any(|run| run.style.link.is_some()));
    let run = scene
        .layout()
        .rows
        .iter()
        .flat_map(|row| &row.runs)
        .find(|run| run.style.link.is_some())
        .unwrap()
        .clone();
    let left = text.x + run.col as i32 * METRICS.cell_w;
    let line = y + METRICS.cell_h - 2;
    let middle = left + run.cells as i32 * METRICS.cell_w / 2;

    let idle = scene.frame();
    assert_eq!(idle.pixel(middle, line), Some(ACCENT));
    assert_eq!(idle.pixel(middle, y + 4), Some(BACKGROUND));
    assert_eq!(idle.pixel(left - 1, line), Some(BACKGROUND));
    assert_eq!(
        idle.pixel(left + run.cells as i32 * METRICS.cell_w, line),
        Some(BACKGROUND)
    );

    scene.send(InputEvent::Tab { reverse: false });
    let focused = scene.frame();
    assert_eq!(focused.pixel(middle, y + 4), Some(HOVER_SHADE), "band");
    assert_eq!(focused.pixel(middle, line), Some(TEXT), "underline");
    // The band stops a pixel short of the line above and below.
    assert_eq!(focused.pixel(middle, y), Some(BACKGROUND));
    assert_eq!(
        focused.pixel(middle, y + METRICS.cell_h - 1),
        Some(BACKGROUND)
    );
}

#[test]
fn the_side_panel_has_a_rule_and_hiding_it_widens_the_page() {
    let mut scene = Scene::new();
    let geometry = scene.browser.geometry;
    let nav = geometry.nav(false).unwrap();
    let wide = scene.frame();
    assert_eq!(wide.pixel(nav.right() - 1, nav.y + 20), Some(BAR));
    let before = geometry.text(false);

    scene.send(InputEvent::ToggleSidebar);
    assert!(scene.browser.nav_collapsed);
    let hidden = scene.frame();
    let after = geometry.text(true);
    assert_eq!(hidden.pixel(nav.right() - 1, nav.y + 20), Some(BACKGROUND));
    assert!(after.x < before.x && after.width > before.width);
    // The header band follows the page to its new left edge and width.
    let header = scene.layout().rows[0].deco.fill;
    assert_eq!(header, Some(BAR));
    assert_eq!(hidden.pixel(after.x, after.y + 2), Some(BAR));
    assert_eq!(
        hidden.pixel(after.x + after.width - 1, after.y + 2),
        Some(BAR)
    );
    assert_eq!(hidden.pixel(after.x - 1, after.y + 2), Some(BACKGROUND));
}

#[test]
fn a_long_page_draws_a_scroll_thumb_and_a_short_one_does_not() {
    let mut scene = Scene::new();
    let track = scene.browser.geometry.track();
    let short = scene.frame();
    assert_eq!(short.pixel(track.x, track.y + 2), Some(BACKGROUND), "fits");

    scene.open("guide/long", false);
    let frame = scene.frame();
    let layout = scene.layout();
    let visible = layout.visible(0, scene.browser.page_height()).len();
    let (start, length) = thumb(0, visible, layout.rows.len(), track.height).unwrap();
    assert_eq!(start, 0);
    assert!(length < track.height);
    assert_eq!(frame.pixel(track.x, track.y), Some(DIM), "thumb at the top");
    assert_eq!(frame.pixel(track.x, track.y + length - 1), Some(DIM));
    assert_eq!(frame.pixel(track.x, track.y + length), Some(BAR), "track");
    assert_eq!(frame.pixel(track.x, track.y + track.height - 1), Some(BAR));
}

#[test]
fn the_active_tab_is_underlined_and_the_others_are_plain() {
    let mut scene = Scene::new();
    scene.open("guide/b", true);
    let frame = scene.frame();
    let source = scene.source();
    let labels = scene.browser.labels(source);
    let tabs = scene
        .browser
        .geometry
        .strip_tabs(&LAYOUT_FONT, &labels, scene.browser.strip_scroll);
    assert_eq!(tabs.len(), 2);
    for tab in &tabs {
        let active = tab.index == scene.browser.active;
        let x = tab.rect.x + 1;
        assert_eq!(
            frame.pixel(x, tab.rect.y + 2),
            Some(BAR),
            "tab {}",
            tab.index
        );
        let underline = frame.pixel(x, tab.rect.bottom() - 1);
        assert_eq!(
            underline,
            Some(if active { ACCENT } else { BAR }),
            "tab {}",
            tab.index
        );
    }
}

#[test]
fn many_tabs_stay_left_of_the_more_button_which_shows_a_chevron() {
    let mut scene = Scene::with_pages(24);
    for n in 0..24 {
        scene.open(&format!("guide/p{n:02}"), true);
    }
    let labels = scene.browser.labels(scene.source());
    assert_eq!(labels.len(), 25);
    let geometry = scene.browser.geometry;
    let tabs = geometry.strip_tabs(&LAYOUT_FONT, &labels, scene.browser.strip_scroll);
    let more = geometry.more_button();
    assert!(tabs.len() < labels.len(), "the strip cannot hold them all");
    assert!(tabs.iter().any(|tab| tab.index == scene.browser.active));
    for tab in &tabs {
        assert!(tab.rect.right() <= more.x, "tab {} overlaps", tab.index);
    }

    let frame = scene.frame();
    let chevron_x = more.right() - 2 - CHEVRON_WIDTH;
    let top = more.y + (more.height - 3) / 2;
    // The chevron narrows by a pixel at each end from its top row down.
    assert_eq!(frame.pixel(chevron_x, top), Some(DIM));
    assert_eq!(frame.pixel(chevron_x + CHEVRON_WIDTH - 1, top), Some(DIM));
    assert_eq!(frame.pixel(chevron_x, top + 1), Some(BACKGROUND));
    assert_eq!(frame.pixel(chevron_x + 2, top + 2), Some(DIM));
    // With the pointer over it, the button is lit.
    let over = Point::new((more.x + 1) as u16, (more.y + 1) as u16);
    let lit = scene.frame_with(Some(over));
    assert_eq!(lit.pixel(chevron_x, top), Some(PALE));
    assert_eq!(lit.pixel(more.x + 1, more.y + 1), Some(BAR));
}

#[test]
fn a_filtered_language_leaves_no_room_for_the_hidden_blocks() {
    let mut scene = Scene::new();
    scene.open("guide/both", false);
    let all = scene.layout().rows.len();
    scene.send(InputEvent::Text('l'));
    assert_eq!(scene.browser.filter, LanguageFilter::Luau);
    let luau = scene.layout().rows.len();
    assert!(luau < all, "{luau} < {all}");
}
