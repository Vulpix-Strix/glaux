use glaux_i18n::{FallbackChain, Language};
use pretty_assertions::assert_eq;

use super::*;
use crate::system::test_font::LAYOUT_FONT;

const METRICS: Metrics = Metrics {
    cell_w: 6,
    cell_h: 12,
    top: 14,
};

/// A page long enough to scroll, with every kind of link.
fn alpha() -> String {
    let mut text = String::from(
        "# Alpha\n\nIntro with [Beta](glaux://guide/b), [the web](https://example.com/x), [mail](mailto:me@example.com), [missing](glaux://guide/none), [file](file:///etc/passwd) and [jump](#two).\n",
    );
    for section in ["One", "Two"] {
        text.push_str(&format!("\n## {section}\n"));
        for n in 0..14 {
            text.push_str(&format!("\nParagraph {n} of {section}.\n"));
        }
    }
    text.push_str("\n## See also\n\n- [Beta](glaux://guide/b)\n- [Gamma](glaux://guide/c)\n");
    text
}

const BOTH: &str = "# Both\n\nCommon text.\n\n### Luau\n\nLuau only.\n\n### Rhai\n\nRhai only.\n";

fn files(extra: usize) -> Vec<(String, Vec<u8>)> {
    let mut files = vec![
        (
            "en/guide/index.md".to_owned(),
            "# Guide\n\nStart.\n\n[Alpha](glaux://guide/a) [Beta](glaux://guide/b)\n".to_owned(),
        ),
        ("en/guide/a.md".to_owned(), alpha()),
        ("en/guide/b.md".to_owned(), "# Beta\n\nSecond.\n".to_owned()),
        ("en/guide/c.md".to_owned(), "# Gamma\n\nThird.\n".to_owned()),
        ("en/guide/both.md".to_owned(), BOTH.to_owned()),
        ("en/api/index.md".to_owned(), "# API\n\nAll.\n".to_owned()),
        (
            "es/guide/b.md".to_owned(),
            "# Beta es\n\nSegunda.\n".to_owned(),
        ),
    ];
    files.extend((0..extra).map(|n| {
        (
            format!("en/guide/p{n:02}.md"),
            format!("# Page {n:02}\n\nText.\n"),
        )
    }));
    files
        .into_iter()
        .map(|(path, text)| (path, text.into_bytes()))
        .collect()
}

struct Case {
    library: Library,
    chain: FallbackChain,
    browser: DocsBrowser,
}

impl Case {
    fn new() -> Self {
        Self::with_pages(0)
    }

    /// A browser on `guide/index`, over the pages above and `extra` more
    /// named `guide/p00` onwards.
    fn with_pages(extra: usize) -> Self {
        let mut case = Self {
            library: Library::from_files(files(extra)),
            chain: FallbackChain::english(),
            browser: DocsBrowser::new(METRICS, LAYOUT_FONT.clone()),
        };
        let source = case.source();
        let mut browser = case.browser.clone();
        browser.show(source);
        case.browser = browser;
        case
    }

    fn source(&self) -> Source<'_> {
        Source {
            library: &self.library,
            chain: &self.chain,
        }
    }

    fn send(&mut self, event: InputEvent) -> Option<DocsRequest> {
        let mut browser = self.browser.clone();
        let request = browser.handle_input(
            event,
            Source {
                library: &self.library,
                chain: &self.chain,
            },
        );
        self.browser = browser;
        request
    }

    fn open(&mut self, id: &str, new_tab: bool) -> Option<DocsRequest> {
        let mut browser = self.browser.clone();
        let request = browser.open(
            Source {
                library: &self.library,
                chain: &self.chain,
            },
            DocId::parse(id).unwrap(),
            None,
            new_tab,
        );
        self.browser = browser;
        request
    }

    fn current(&self) -> &str {
        self.browser.current().map_or("", DocId::as_str)
    }

    fn tab_ids(&self) -> Vec<&str> {
        self.browser
            .tabs
            .iter()
            .map(|tab| tab.current().id.as_str())
            .collect()
    }

    fn press_tab(&mut self, times: usize) {
        for _ in 0..times {
            self.send(InputEvent::Tab { reverse: false });
        }
    }

    fn layout(&self) -> &Layout {
        &self.browser.shown.as_ref().unwrap().layout
    }

    /// A point on the text of link `link`, which must be in view.
    fn link_point(&self, link: usize) -> Point {
        let layout = self.layout();
        let row = layout.link_rows[link];
        let text = self.browser.geometry.text(self.browser.nav_collapsed);
        let top: i32 = layout.rows[self.browser.scroll()..row]
            .iter()
            .map(|row| row.height(layout.cell_h))
            .sum();
        let run = layout.rows[row]
            .runs
            .iter()
            .find(|run| run.style.link == Some(link))
            .unwrap();
        Point::new(
            (text.x + run.col as i32 * METRICS.cell_w + 2) as u16,
            (text.y + top + 3) as u16,
        )
    }

    fn press(&mut self, at: Point, button: PointerButton, select: bool) -> Option<DocsRequest> {
        self.send(InputEvent::PointerDown { button, at, select })
    }

    fn click(&mut self, at: Point) -> Option<DocsRequest> {
        self.press(at, PointerButton::Primary, false)
    }

    fn scroll(&self) -> usize {
        self.browser.scroll()
    }

    fn max_scroll(&self) -> usize {
        self.layout().max_scroll(self.browser.page_height())
    }
}

fn centre(rect: Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

fn notice(text: &str) -> Option<DocsRequest> {
    Some(DocsRequest::Notice(text.to_owned()))
}

#[test]
fn a_browser_with_no_tabs_opens_the_guides_front_page() {
    let case = Case::new();
    assert_eq!(case.tab_ids(), ["guide/index"]);
    assert!(case.browser.shown.is_some());
}

#[test]
fn showing_the_browser_again_keeps_its_tabs() {
    let mut case = Case::new();
    case.open("guide/a", true);
    let mut browser = case.browser.clone();
    browser.show(case.source());
    assert_eq!(browser.tabs, case.browser.tabs);
    assert_eq!(browser.active, 1);
}

#[test]
fn escape_and_f1_ask_to_hide_the_browser() {
    let mut case = Case::new();
    assert_eq!(case.send(InputEvent::Docs), Some(DocsRequest::Close));
    assert_eq!(case.send(InputEvent::Break), Some(DocsRequest::Close));
}

#[test]
fn escape_closes_search_and_the_tab_menu_before_the_browser() {
    let mut case = Case::new();
    case.send(InputEvent::Find);
    assert!(case.browser.has_overlay());
    assert_eq!(case.send(InputEvent::Break), None);
    assert!(!case.browser.has_overlay());

    let more = case.browser.geometry.more_button();
    case.click(centre(more));
    assert!(case.browser.menu.is_some());
    assert_eq!(case.send(InputEvent::Break), None);
    assert!(case.browser.menu.is_none());
    assert_eq!(case.send(InputEvent::Break), Some(DocsRequest::Close));
}

#[test]
fn a_link_opens_in_the_same_tab_and_back_and_forward_walk_the_history() {
    let mut case = Case::new();
    case.open("guide/a", false);
    case.press_tab(1);
    assert_eq!(case.send(InputEvent::Enter), None);
    assert_eq!((case.current(), case.browser.tab_count()), ("guide/b", 1));
    case.send(InputEvent::Back);
    assert_eq!(case.current(), "guide/a");
    case.send(InputEvent::Back);
    assert_eq!(case.current(), "guide/index");
    case.send(InputEvent::Back);
    assert_eq!(case.current(), "guide/index");
    case.send(InputEvent::Forward);
    case.send(InputEvent::Forward);
    assert_eq!(case.current(), "guide/b");
    case.send(InputEvent::Forward);
    assert_eq!(case.current(), "guide/b");
}

#[test]
fn going_back_and_forward_restores_where_each_page_was_read() {
    let mut case = Case::new();
    case.open("guide/a", false);
    case.send(InputEvent::Down { select: false });
    case.send(InputEvent::Down { select: false });
    case.press_tab(1);
    let (scroll, focus) = (case.scroll(), case.browser.tabs[0].current().focus);
    assert!(scroll >= 2 && focus.is_some());

    case.open("guide/b", false);
    assert_eq!(case.scroll(), 0);
    case.send(InputEvent::Back);
    assert_eq!(case.current(), "guide/a");
    assert_eq!(
        (case.scroll(), case.browser.tabs[0].current().focus),
        (scroll, focus)
    );
    case.send(InputEvent::Forward);
    assert_eq!(case.current(), "guide/b");
}

#[test]
fn navigating_after_going_back_drops_what_lay_ahead() {
    let mut case = Case::new();
    case.open("guide/a", false);
    case.open("guide/b", false);
    case.send(InputEvent::Back);
    case.open("guide/c", false);
    let history: Vec<_> = case.browser.tabs[0]
        .history
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(history, ["guide/index", "guide/a", "guide/c"]);
    case.send(InputEvent::Forward);
    assert_eq!(case.current(), "guide/c");
}

#[test]
fn each_tab_has_a_history_of_its_own() {
    let mut case = Case::new();
    case.open("guide/a", true);
    case.open("guide/b", false);
    assert_eq!(case.tab_ids(), ["guide/index", "guide/b"]);
    case.send(InputEvent::Back);
    assert_eq!(case.tab_ids(), ["guide/index", "guide/a"]);
    case.send(InputEvent::PreviousDocument);
    case.send(InputEvent::Back);
    assert_eq!(case.tab_ids(), ["guide/index", "guide/a"]);
}

#[test]
fn a_link_opens_in_a_new_tab_from_the_keyboard_or_with_shift_or_middle() {
    let mut case = Case::new();
    case.open("guide/a", false);
    case.press_tab(1);
    case.send(InputEvent::Text('t'));
    assert_eq!(case.tab_ids(), ["guide/a", "guide/b"]);
    assert_eq!(case.browser.active_tab(), 1);

    let mut case = Case::new();
    case.open("guide/a", false);
    let at = case.link_point(0);
    case.press(at, PointerButton::Primary, true);
    assert_eq!(case.tab_ids(), ["guide/a", "guide/b"]);

    let mut case = Case::new();
    case.open("guide/a", false);
    let at = case.link_point(0);
    case.press(at, PointerButton::Middle, false);
    assert_eq!(case.tab_ids(), ["guide/a", "guide/b"]);
}

#[test]
fn a_plain_press_on_a_link_opens_it_in_the_same_tab() {
    let mut case = Case::new();
    case.open("guide/a", false);
    let at = case.link_point(0);
    assert_eq!(case.click(at), None);
    assert_eq!((case.current(), case.browser.tab_count()), ("guide/b", 1));
}

#[test]
fn a_page_already_open_is_brought_forward_instead_of_opened_twice() {
    let mut case = Case::new();
    case.open("guide/a", true);
    case.open("guide/b", true);
    assert_eq!(case.browser.active_tab(), 2);
    case.open("guide/index", true);
    assert_eq!(
        (case.browser.tab_count(), case.browser.active_tab()),
        (3, 0)
    );
    case.open("guide/b", true);
    assert_eq!(
        (case.browser.tab_count(), case.browser.active_tab()),
        (3, 2)
    );
}

#[test]
fn many_tabs_open_without_replacing_or_losing_any() {
    let mut case = Case::with_pages(12);
    // Give the first tab some state to lose.
    case.open("guide/a", false);
    case.send(InputEvent::Down { select: false });
    case.press_tab(1);
    let first = case.browser.tabs[0].clone();
    for n in 0..12 {
        assert_eq!(case.open(&format!("guide/p{n:02}"), true), None);
    }
    assert_eq!(case.browser.tab_count(), 13);
    assert_eq!(case.browser.tabs[0], first);
    for n in 0..12 {
        assert_eq!(
            case.browser.tabs[n + 1].current().id.as_str(),
            format!("guide/p{n:02}")
        );
    }
}

#[test]
fn the_strip_scrolls_to_keep_the_active_tab_in_view() {
    let mut case = Case::with_pages(12);
    for n in 0..12 {
        case.open(&format!("guide/p{n:02}"), true);
    }
    let labels = case.browser.labels(case.source());
    let visible = |case: &Case| {
        case.browser
            .geometry
            .strip_tabs(&LAYOUT_FONT, &labels, case.browser.strip_scroll)
            .iter()
            .map(|tab| tab.index)
            .collect::<Vec<_>>()
    };
    assert!(visible(&case).contains(&12));
    assert!(case.browser.strip_scroll > 0);
    // Wrapping to the first tab brings the strip back to the start.
    case.send(InputEvent::NextDocument);
    assert_eq!(case.browser.active_tab(), 0);
    assert_eq!(case.browser.strip_scroll, 0);
    assert!(visible(&case).contains(&0));
}

#[test]
fn the_tab_menu_reaches_every_tab() {
    let mut case = Case::with_pages(20);
    for n in 0..20 {
        case.open(&format!("guide/p{n:02}"), true);
    }
    case.send(InputEvent::NextDocument);
    assert_eq!(case.browser.active_tab(), 0);
    let more = case.browser.geometry.more_button();
    case.click(centre(more));
    for _ in 0..20 {
        case.send(InputEvent::Down { select: false });
    }
    assert_eq!(case.send(InputEvent::Enter), None);
    assert_eq!(case.browser.active_tab(), 20);
    assert!(case.browser.menu.is_none());
}

#[test]
fn clicking_a_row_of_the_tab_menu_shows_that_tab() {
    let mut case = Case::with_pages(3);
    for n in 0..3 {
        case.open(&format!("guide/p{n:02}"), true);
    }
    let more = case.browser.geometry.more_button();
    case.click(centre(more));
    let (_, rows) = case
        .browser
        .geometry
        .menu_rows(4, case.browser.active_tab());
    let (index, rect) = rows[1];
    case.click(centre(rect));
    assert_eq!(case.browser.active_tab(), index);
    assert!(case.browser.menu.is_none());
}

#[test]
fn the_thirty_third_tab_is_refused_and_nothing_changes() {
    let mut case = Case::with_pages(40);
    for n in 0..MAX_TABS - 1 {
        assert_eq!(case.open(&format!("guide/p{n:02}"), true), None);
    }
    assert_eq!(case.browser.tab_count(), MAX_TABS);
    let before = case.browser.clone();
    assert_eq!(
        case.open("guide/p39", true),
        notice("Docs tab limit reached (32). Close a tab first.")
    );
    assert_eq!(case.browser, before);
    // A page one of them shows is still reachable, and closing one makes
    // room.
    assert_eq!(case.open("guide/p00", true), None);
    assert_eq!(case.browser.active_tab(), 1);
    case.send(InputEvent::CloseDocument);
    assert_eq!(case.open("guide/p39", true), None);
    assert_eq!(case.browser.tab_count(), MAX_TABS);
}

#[test]
fn the_limit_also_refuses_a_link_in_a_new_tab() {
    let mut case = Case::with_pages(40);
    for n in 0..MAX_TABS - 1 {
        case.open(&format!("guide/p{n:02}"), true);
    }
    case.open("guide/a", false);
    let before = case.browser.clone();
    case.press_tab(1);
    let request = case.send(InputEvent::Text('t'));
    assert_eq!(
        request,
        notice("Docs tab limit reached (32). Close a tab first.")
    );
    assert_eq!(case.browser.tab_count(), before.tab_count());
    assert_eq!(case.current(), "guide/a");
}

#[test]
fn closing_tabs_keeps_the_right_one_active() {
    let mut case = Case::with_pages(3);
    for n in 0..3 {
        case.open(&format!("guide/p{n:02}"), true);
    }
    // Tabs: index, p00, p01, p02, with p02 active.
    case.send(InputEvent::PreviousDocument);
    assert_eq!(case.current(), "guide/p01");
    case.browser.close_tab(0);
    assert_eq!(case.current(), "guide/p01");
    case.send(InputEvent::CloseDocument);
    assert_eq!(case.current(), "guide/p02");
    case.send(InputEvent::CloseDocument);
    assert_eq!(case.current(), "guide/p00");
    assert_eq!(case.tab_ids(), ["guide/p00"]);
}

#[test]
fn closing_the_last_tab_hides_the_browser_and_showing_it_opens_the_home_page() {
    let mut case = Case::new();
    assert_eq!(
        case.send(InputEvent::CloseDocument),
        Some(DocsRequest::Close)
    );
    assert_eq!(case.browser.tab_count(), 0);
    assert!(case.browser.shown.is_none());
    let mut browser = case.browser.clone();
    browser.show(case.source());
    assert_eq!(browser.current().map(DocId::as_str), Some("guide/index"));
}

#[test]
fn the_close_glyph_and_the_middle_button_close_a_tab() {
    let mut case = Case::new();
    case.open("guide/a", true);
    case.open("guide/b", true);
    let labels = case.browser.labels(case.source());
    let tabs = case.browser.geometry.strip_tabs(&LAYOUT_FONT, &labels, 0);
    case.click(centre(tabs[1].close));
    assert_eq!(case.tab_ids(), ["guide/index", "guide/b"]);
    let labels = case.browser.labels(case.source());
    let tabs = case.browser.geometry.strip_tabs(&LAYOUT_FONT, &labels, 0);
    case.press(centre(tabs[0].rect), PointerButton::Middle, false);
    assert_eq!(case.tab_ids(), ["guide/b"]);
}

#[test]
fn clicking_a_tab_shows_it() {
    let mut case = Case::new();
    case.open("guide/a", true);
    let labels = case.browser.labels(case.source());
    let tabs = case.browser.geometry.strip_tabs(&LAYOUT_FONT, &labels, 0);
    case.click(Point::new(
        (tabs[0].rect.x + 3) as u16,
        (tabs[0].rect.y + 3) as u16,
    ));
    assert_eq!(case.browser.active_tab(), 0);
}

#[test]
fn every_tab_keeps_its_own_scroll_and_focus() {
    let mut case = Case::with_pages(9);
    case.open("guide/a", false);
    case.send(InputEvent::PageDown { select: false });
    case.press_tab(2);
    let state = |case: &Case| {
        let entry = case.browser.tabs[0].current();
        (entry.scroll, entry.focus)
    };
    let first = state(&case);
    assert_ne!(first.0, 0);
    for n in 0..9 {
        case.open(&format!("guide/p{n:02}"), true);
        case.send(InputEvent::Tab { reverse: false });
    }
    assert_eq!(state(&case), first);
    case.send(InputEvent::NextDocument);
    assert_eq!(case.browser.active_tab(), 0);
    assert_eq!(case.scroll(), first.0);
    assert_eq!(case.browser.tabs[0].current().focus, first.1);
}

#[test]
fn links_that_cannot_be_followed_say_why_and_change_nothing() {
    let mut case = Case::new();
    case.open("guide/a", false);
    let pages = |case: &Case| {
        case.browser.tabs[0]
            .history
            .iter()
            .map(|entry| (entry.id.clone(), entry.fragment.clone()))
            .collect::<Vec<_>>()
    };
    let before = pages(&case);

    case.press_tab(4);
    assert_eq!(
        case.send(InputEvent::Enter),
        notice("Broken link: glaux://guide/none")
    );
    case.press_tab(1);
    assert_eq!(case.send(InputEvent::Enter), notice("Blocked link: file"));
    assert_eq!(case.current(), "guide/a");
    assert_eq!(pages(&case), before);
}

#[test]
fn web_and_mail_links_ask_the_workspace_to_open_them_outside() {
    let mut case = Case::new();
    case.open("guide/a", false);
    case.press_tab(2);
    assert_eq!(
        case.send(InputEvent::Enter),
        Some(DocsRequest::OpenExternal(
            ExternalUrl::parse("https://example.com/x").unwrap()
        ))
    );
    case.press_tab(1);
    assert_eq!(
        case.send(InputEvent::Enter),
        Some(DocsRequest::OpenExternal(
            ExternalUrl::parse("mailto:me@example.com").unwrap()
        ))
    );
    assert_eq!(case.current(), "guide/a");
}

#[test]
fn nothing_is_opened_outside_until_the_user_acts() {
    let mut case = Case::new();
    case.open("guide/a", false);
    // Showing, scrolling and focusing a link never ask for anything.
    for event in [
        InputEvent::Down { select: false },
        InputEvent::Tab { reverse: false },
        InputEvent::Tab { reverse: false },
        InputEvent::Wheel { delta: -1 },
    ] {
        assert_eq!(case.send(event), None);
    }
}

#[test]
fn a_fragment_scrolls_to_its_heading() {
    let mut case = Case::new();
    case.open("guide/a", false);
    case.press_tab(6);
    assert_eq!(case.send(InputEvent::Enter), None);
    assert_eq!(case.browser.tabs[0].history.len(), 3);
    assert_eq!(case.current(), "guide/a");
    let row = case.layout().anchor_row("two").unwrap();
    assert_eq!(case.scroll(), row.min(case.max_scroll()));
    assert!(case.scroll() > 0);
    case.send(InputEvent::Back);
    assert_eq!(case.scroll(), 0);
}

#[test]
fn a_link_to_a_page_with_a_fragment_opens_it_there() {
    let mut case = Case::new();
    let source = case.source();
    let mut browser = case.browser.clone();
    browser.open(
        source,
        DocId::parse("guide/a").unwrap(),
        Some("two".to_owned()),
        false,
    );
    case.browser = browser;
    let row = case.layout().anchor_row("two").unwrap();
    assert_eq!(case.scroll(), row.min(case.max_scroll()));
}

#[test]
fn an_unknown_fragment_opens_the_page_at_the_top() {
    let mut case = Case::new();
    let source = case.source();
    let mut browser = case.browser.clone();
    browser.open(
        source,
        DocId::parse("guide/a").unwrap(),
        Some("nowhere".to_owned()),
        false,
    );
    case.browser = browser;
    assert_eq!(case.scroll(), 0);
}

#[test]
fn a_page_in_another_language_than_asked_is_labelled_with_its_own() {
    let mut case = Case::new();
    case.chain = FallbackChain::new(Language::parse("es").unwrap());
    // Beta has a Spanish page, Gamma does not.
    case.open("guide/b", false);
    let header = |case: &Case| {
        case.layout().rows[0]
            .runs
            .iter()
            .map(|run| run.text.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(header(&case), ["Beta es", "guide"]);
    case.open("guide/c", false);
    assert_eq!(header(&case), ["Gamma", "[en] guide"]);
}

#[test]
fn the_language_filter_changes_the_content_and_not_the_pages_language() {
    let mut case = Case::new();
    case.open("guide/both", false);
    let texts = |case: &Case| -> Vec<String> {
        case.layout()
            .rows
            .iter()
            .flat_map(|row| row.runs.iter().map(|run| run.text.clone()))
            .collect()
    };
    assert!(texts(&case).contains(&"Luau only.".to_owned()));
    assert!(texts(&case).contains(&"Rhai only.".to_owned()));
    case.send(InputEvent::Text('l'));
    assert_eq!(case.browser.filter(), LanguageFilter::Luau);
    assert!(texts(&case).contains(&"Luau only.".to_owned()));
    assert!(!texts(&case).contains(&"Rhai only.".to_owned()));
    case.send(InputEvent::Text('L'));
    assert_eq!(case.browser.filter(), LanguageFilter::Rhai);
    assert!(!texts(&case).contains(&"Luau only.".to_owned()));
    assert!(texts(&case).contains(&"Rhai only.".to_owned()));
    case.send(InputEvent::Text('l'));
    assert_eq!(case.browser.filter(), LanguageFilter::All);
    // The page is the same one, in the same language.
    assert_eq!(
        case.browser.shown.as_ref().unwrap().key.language.as_deref(),
        Some("en")
    );
}

#[test]
fn the_filter_button_cycles_it_and_it_stays_when_opening_other_pages() {
    let mut case = Case::new();
    let bar = case.browser.options_bar();
    case.click(centre(bar.filter));
    assert_eq!(case.browser.filter(), LanguageFilter::Luau);
    case.open("guide/both", true);
    assert_eq!(case.browser.filter(), LanguageFilter::Luau);
    assert_eq!(
        case.browser.shown.as_ref().unwrap().key.filter,
        LanguageFilter::Luau
    );
}

#[test]
fn hiding_the_side_panel_lays_the_page_out_wider() {
    let mut case = Case::new();
    case.open("guide/a", false);
    let narrow = case.browser.shown.as_ref().unwrap().key.cols;
    case.send(InputEvent::ToggleSidebar);
    assert!(case.browser.nav_collapsed());
    let wide = case.browser.shown.as_ref().unwrap().key.cols;
    assert!(wide > narrow);
    assert_eq!(wide, case.browser.geometry.cols(true));
    // The header spans the wider page.
    let header = &case.layout().rows[0];
    assert!(
        header
            .runs
            .iter()
            .map(|run| run.col + run.cells)
            .max()
            .unwrap()
            > narrow
    );
    case.send(InputEvent::ToggleSidebar);
    assert_eq!(case.browser.shown.as_ref().unwrap().key.cols, narrow);
}

#[test]
fn the_nav_button_toggles_the_panel_too() {
    let mut case = Case::new();
    let bar = case.browser.options_bar();
    case.click(centre(bar.nav));
    assert!(case.browser.nav_collapsed());
}

#[test]
fn the_view_keeps_inside_the_page_when_the_layout_shrinks() {
    let mut case = Case::new();
    case.open("guide/a", false);
    case.send(InputEvent::ToggleSidebar);
    case.send(InputEvent::DocumentEnd { select: false });
    let far = case.scroll();
    assert_eq!(far, case.max_scroll());
    case.send(InputEvent::ToggleSidebar);
    assert!(case.scroll() <= case.max_scroll());
    assert!(far > 0);
}

#[test]
fn keys_and_the_wheel_scroll_within_the_page() {
    let mut case = Case::new();
    case.open("guide/a", false);
    assert_eq!(case.scroll(), 0);
    case.send(InputEvent::Up { select: false });
    assert_eq!(case.scroll(), 0);
    case.send(InputEvent::Down { select: false });
    assert_eq!(case.scroll(), 1);
    case.send(InputEvent::Wheel { delta: -1 });
    assert_eq!(case.scroll(), 4);
    case.send(InputEvent::Wheel { delta: 2 });
    assert_eq!(case.scroll(), 0);
    let screenful = case.browser.screenful() as usize;
    case.send(InputEvent::PageDown { select: false });
    assert_eq!(case.scroll(), screenful);
    case.send(InputEvent::PageUp { select: false });
    assert_eq!(case.scroll(), 0);
    case.send(InputEvent::End { select: false });
    assert_eq!(case.scroll(), case.max_scroll());
    case.send(InputEvent::Down { select: false });
    assert_eq!(case.scroll(), case.max_scroll());
    case.send(InputEvent::Home { select: false });
    assert_eq!(case.scroll(), 0);
}

#[test]
fn a_page_that_fits_does_not_scroll() {
    let mut case = Case::new();
    case.open("guide/b", false);
    case.send(InputEvent::PageDown { select: false });
    case.send(InputEvent::Wheel { delta: -3 });
    assert_eq!(case.scroll(), 0);
}

#[test]
fn tab_walks_the_links_in_both_directions_and_wraps() {
    let mut case = Case::new();
    case.open("guide/a", false);
    let focus = |case: &Case| case.browser.tabs[0].current().focus;
    assert_eq!(focus(&case), None);
    case.press_tab(1);
    assert_eq!(focus(&case), Some(0));
    case.press_tab(1);
    assert_eq!(focus(&case), Some(1));
    case.send(InputEvent::Tab { reverse: true });
    assert_eq!(focus(&case), Some(0));
    case.send(InputEvent::Tab { reverse: true });
    let last = case.layout().links.len() - 1;
    assert_eq!(focus(&case), Some(last));
    case.press_tab(1);
    assert_eq!(focus(&case), Some(0));
}

#[test]
fn focusing_a_link_below_the_view_scrolls_to_it() {
    let mut case = Case::new();
    case.open("guide/a", false);
    case.press_tab(1);
    assert_eq!(case.scroll(), 0);
    // Going back from the first link wraps to the last, far below.
    case.send(InputEvent::Tab { reverse: true });
    let last = case.layout().links.len() - 1;
    assert_eq!(case.browser.tabs[0].current().focus, Some(last));
    let row = case.layout().link_rows[last];
    let visible = case
        .layout()
        .visible(case.scroll(), case.browser.page_height());
    assert!(case.scroll() > 0);
    assert!(visible.contains(&row), "{visible:?} {row}");
}

#[test]
fn a_page_without_links_ignores_tab_and_enter() {
    let mut case = Case::new();
    case.open("guide/b", false);
    assert_eq!(case.send(InputEvent::Tab { reverse: false }), None);
    assert_eq!(case.send(InputEvent::Enter), None);
    assert_eq!(case.send(InputEvent::Text('t')), None);
}

#[test]
fn next_and_previous_document_cycle_the_tabs() {
    let mut case = Case::new();
    case.open("guide/a", true);
    case.open("guide/b", true);
    case.send(InputEvent::NextDocument);
    assert_eq!(case.browser.active_tab(), 0);
    case.send(InputEvent::PreviousDocument);
    assert_eq!(case.browser.active_tab(), 2);
}

#[test]
fn find_opens_search_and_choosing_a_page_opens_it() {
    let mut case = Case::new();
    case.send(InputEvent::Find);
    assert!(case.browser.search.is_some());
    for ch in "gamma".chars() {
        case.send(InputEvent::Text(ch));
    }
    assert_eq!(case.send(InputEvent::Enter), None);
    assert!(case.browser.search.is_none());
    assert_eq!(case.current(), "guide/c");
    assert_eq!(case.browser.tab_count(), 1);
}

#[test]
fn the_search_button_opens_search_and_shift_enter_is_not_needed_for_new_tabs() {
    let mut case = Case::new();
    let bar = case.browser.options_bar();
    case.click(centre(bar.search));
    assert!(case.browser.search.is_some());
    let layout = case.browser.search_layout(case.browser.search_area());
    case.press(centre(layout.rows[1]), PointerButton::Primary, true);
    assert_eq!(case.browser.tab_count(), 2);
}

#[test]
fn typing_while_searching_does_not_change_the_language_filter() {
    let mut case = Case::new();
    case.send(InputEvent::Find);
    case.send(InputEvent::Text('l'));
    case.send(InputEvent::Text('t'));
    assert_eq!(case.browser.filter(), LanguageFilter::All);
    assert_eq!(case.browser.search.as_ref().unwrap().field().text(), "lt");
}

#[test]
fn search_asks_for_the_clipboard_and_takes_its_text() {
    let mut case = Case::new();
    case.send(InputEvent::Find);
    assert_eq!(case.send(InputEvent::Paste), Some(DocsRequest::Paste));
    let mut browser = case.browser.clone();
    browser.paste(case.source(), Some("gamma".to_owned()));
    assert_eq!(browser.search.as_ref().unwrap().results().len(), 1);
    assert_eq!(
        case.send(InputEvent::Copy),
        None,
        "nothing is selected, so there is nothing to copy"
    );
}

#[test]
fn the_back_and_forward_buttons_travel_the_history() {
    let mut case = Case::new();
    case.open("guide/a", false);
    let bar = case.browser.options_bar();
    case.click(centre(bar.back));
    assert_eq!(case.current(), "guide/index");
    case.click(centre(bar.forward));
    assert_eq!(case.current(), "guide/a");
}

#[test]
fn the_outline_scrolls_to_a_heading_and_related_pages_open() {
    let mut case = Case::new();
    case.open("guide/a", false);
    let rows = case.browser.panel_rows().unwrap();
    let titles: Vec<_> = rows
        .iter()
        .map(|row| match &row.kind {
            PanelKind::Title(title) => (*title).to_owned(),
            PanelKind::Outline { row, .. } => case.browser.outline_title(*row),
            PanelKind::Related(index) => {
                format!(
                    "-> {}",
                    case.browser.shown.as_ref().unwrap().related[*index].1
                )
            }
        })
        .collect();
    assert_eq!(
        titles,
        [
            "On this page",
            "One",
            "Two",
            "See also",
            "Related",
            "-> Beta",
            "-> Gamma"
        ]
    );
    let two = &rows[2];
    case.click(centre(two.rect));
    let PanelKind::Outline { row, .. } = two.kind else {
        panic!("not an outline row");
    };
    assert_eq!(case.scroll(), row.min(case.max_scroll()));
    assert_eq!(case.current(), "guide/a");

    let beta = &rows[5];
    case.click(centre(beta.rect));
    assert_eq!(case.current(), "guide/b");
}

#[test]
fn the_outline_marks_the_section_the_view_is_in() {
    let mut case = Case::new();
    case.open("guide/a", false);
    let active = |case: &Case| {
        case.browser
            .panel_rows()
            .unwrap()
            .iter()
            .filter_map(|row| match row.kind {
                PanelKind::Outline { row, active: true } => Some(case.browser.outline_title(row)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(active(&case), Vec::<String>::new());
    let two = case.layout().anchor_row("two").unwrap();
    case.browser.set_scroll(two);
    assert_eq!(active(&case), ["Two"]);
}

#[test]
fn a_hidden_panel_has_no_rows_and_takes_no_presses() {
    let mut case = Case::new();
    case.open("guide/a", false);
    case.send(InputEvent::ToggleSidebar);
    assert_eq!(case.browser.panel_rows(), None);
    assert!(case.browser.panel_at(Point::new(10, 40)).is_none());
}

#[test]
fn the_pointer_is_a_hand_over_anything_that_acts() {
    let mut case = Case::new();
    case.open("guide/a", false);
    let source = case.source();
    let cursor = |at| case.browser.cursor(at, source);
    assert_eq!(cursor(case.link_point(0)), SystemCursor::Hand);
    let bar = case.browser.options_bar();
    for rect in [bar.back, bar.forward, bar.filter, bar.nav, bar.search] {
        assert_eq!(cursor(centre(rect)), SystemCursor::Hand);
    }
    let labels = case.browser.labels(source);
    let tab = case.browser.geometry.strip_tabs(&LAYOUT_FONT, &labels, 0)[0].rect;
    assert_eq!(cursor(centre(tab)), SystemCursor::Hand);
    let text = case.browser.geometry.text(false);
    assert_eq!(
        cursor(Point::new(
            (text.right() - 1) as u16,
            (text.bottom() - 2) as u16
        )),
        SystemCursor::Arrow
    );
}

#[test]
fn the_status_bar_names_the_link_under_the_pointer() {
    let mut case = Case::new();
    case.open("guide/a", false);
    assert_eq!(
        case.browser.hovered_target(case.link_point(0)),
        Some("glaux://guide/b")
    );
    assert_eq!(
        case.browser.hovered_target(case.link_point(1)),
        Some("https://example.com/x")
    );
    assert_eq!(case.browser.hovered_target(Point::new(1, 100)), None);
    case.send(InputEvent::Find);
    assert_eq!(case.browser.hovered_target(case.link_point(0)), None);
    assert_eq!(
        case.browser.hint(),
        "Type: query  Tab: filter  Enter: open  Esc: close"
    );
}

#[test]
fn a_page_that_vanished_shows_a_message_instead_of_failing() {
    let mut case = Case::new();
    case.library = Library::from_files(Vec::new());
    let mut browser = case.browser.clone();
    browser.open(
        case.source(),
        DocId::parse("guide/gone").unwrap(),
        None,
        false,
    );
    case.browser = browser;
    assert_eq!(case.layout().rows.len(), 1);
    assert_eq!(case.layout().rows[0].runs[0].text, "No page guide/gone");
}

#[test]
fn the_real_documentation_opens_and_every_internal_link_resolves() {
    let library = Library::embedded();
    let chain = FallbackChain::english();
    let source = Source {
        library: &library,
        chain: &chain,
    };
    let mut browser = DocsBrowser::new(METRICS, LAYOUT_FONT.clone());
    browser.show(source);
    assert_eq!(browser.current().map(DocId::as_str), Some("guide/index"));
    for id in nav::order(&library, &chain) {
        browser.open(source, id.clone(), None, false);
        let shown = browser.shown.as_ref().unwrap();
        assert!(!shown.layout.rows.is_empty(), "{id}");
        for target in &shown.layout.links {
            if let Some(Ok(linked)) = DocId::from_uri(target) {
                assert!(library.page(&linked, &chain).is_some(), "{id} -> {target}");
            }
        }
    }
}

impl Case {
    fn show_target(&mut self, id: &str, fragment: Option<&str>) -> Option<DocsRequest> {
        let mut browser = self.browser.clone();
        let request = browser.show_target(
            self.source(),
            DocTarget {
                id: DocId::parse(id).unwrap(),
                fragment: fragment.map(str::to_owned),
            },
        );
        self.browser = browser;
        request
    }
}

#[test]
fn a_target_opens_in_a_new_tab_and_leaves_the_others_alone() {
    let mut case = Case::new();
    case.open("guide/a", false);
    assert_eq!(case.show_target("guide/b", None), None);
    assert_eq!(case.tab_ids(), ["guide/a", "guide/b"]);
    assert_eq!(case.current(), "guide/b");
    // The tab it left still has its history.
    case.send(InputEvent::PreviousDocument);
    assert_eq!(case.current(), "guide/a");
    case.send(InputEvent::Back);
    assert_eq!(case.current(), "guide/index");
}

#[test]
fn a_target_with_no_tabs_does_not_leave_the_front_page_behind() {
    let mut case = Case::new();
    case.browser.tabs.clear();
    case.browser.shown = None;
    case.show_target("guide/b", None);
    assert_eq!(case.tab_ids(), ["guide/b"]);
}

#[test]
fn a_target_for_an_open_page_shows_that_tab_and_goes_to_the_heading() {
    let mut case = Case::new();
    case.open("guide/a", true);
    case.open("guide/b", true);
    assert_eq!(case.show_target("guide/a", Some("two")), None);
    assert_eq!(case.tab_ids().len(), 3);
    assert_eq!(case.current(), "guide/a");
    assert_eq!(case.browser.active_tab(), 1);
    assert!(case.scroll() > 0, "the view went to the heading");
    assert_eq!(
        case.browser.location().map(|(_, fragment)| fragment),
        Some(Some("two"))
    );
}

#[test]
fn a_target_is_refused_at_the_tab_limit_and_changes_nothing() {
    let mut case = Case::with_pages(MAX_TABS);
    for n in 0..MAX_TABS - 1 {
        case.open(&format!("guide/p{n:02}"), true);
    }
    assert_eq!(case.tab_ids().len(), MAX_TABS);
    let before = case.browser.clone();
    let request = case.show_target(&format!("guide/p{:02}", MAX_TABS - 1), None);
    assert!(matches!(request, Some(DocsRequest::Notice(_))));
    assert_eq!(case.browser, before);
    // A page that is open still comes forward.
    assert_eq!(case.show_target("guide/p00", None), None);
    assert_eq!(case.current(), "guide/p00");
}

#[test]
fn showing_the_search_keeps_the_tabs_and_opens_the_query() {
    let mut case = Case::new();
    case.open("guide/a", true);
    let mut browser = case.browser.clone();
    browser.show_search(case.source());
    assert_eq!(browser.tab_count(), 2);
    assert_eq!(browser.active_tab(), 1);
    assert!(browser.search_hits().is_some());
}
