//! The documentation browser: pages in tabs, each with its own history,
//! beside an outline of the page, with a search over every page.
//!
//! The browser belongs to the workspace and knows nothing about the Code
//! view. It reads pages from a [`Source`] it is lent, so it keeps no copy of
//! the library, and it never touches the cartridge's documents.

mod layout;
mod nav;
mod render;
mod search;
mod tabs;
mod view;

use glaux_docs::{DocId, Kind, Library, Scope as PageScope, ScriptLanguage};
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::system_rom::SystemCursor;

use layout::{Layout, Params};
use search::{Search, SearchAction, SearchLayout};
use tabs::StripHit;
pub use view::Metrics;
use view::{Geometry, OptionsBar};

use super::Rect;
use crate::system::ExternalUrl;
use crate::system::ExternalUrlError;

/// Most page tabs open at once. Opening one more is refused, so no tab is
/// ever replaced behind the user's back.
pub const MAX_TABS: usize = 32;
/// Rows the page moves per wheel notch.
const WHEEL_ROWS: isize = 3;
/// The page opened when the browser has none.
const HOME: &str = "guide/index";
/// Heading the side panel lists the headings under, then the next level.
const OUTLINE_LEVEL: u8 = 2;

/// The status bar hint while the page has the keys.
pub const DOCS_HINT: &str = "Alt+Left: back  Alt+Right: fwd  Ctrl+B: nav";
/// The status bar hint while SEARCH DOCS is open.
pub const SEARCH_HINT: &str = "Type: query  Tab: filter  Enter: open  Esc: close";
/// The status bar hint while the list of tabs is open.
pub const MENU_HINT: &str = "Up/Down: pick  Enter: show  Esc: close";

/// The pages the browser reads, and the languages to read them in.
#[derive(Debug, Clone, Copy)]
pub struct Source<'a> {
    pub library: &'a Library,
    pub chain: &'a glaux_i18n::FallbackChain,
}

/// Which script language's parts of a page show. This is about content and
/// has nothing to do with the language the interface is read in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LanguageFilter {
    #[default]
    All,
    Luau,
    Rhai,
}

impl LanguageFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Luau => "Luau",
            Self::Rhai => "Rhai",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::All => Self::Luau,
            Self::Luau => Self::Rhai,
            Self::Rhai => Self::All,
        }
    }

    /// Whether a part of a page about `scope` shows.
    pub fn shows(self, scope: PageScope) -> bool {
        match (self, scope) {
            (_, PageScope::Common) | (Self::All, _) => true,
            (Self::Luau, PageScope::Language(language)) => language == ScriptLanguage::Luau,
            (Self::Rhai, PageScope::Language(language)) => language == ScriptLanguage::Rhai,
        }
    }
}

/// What the browser asks the workspace to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocsRequest {
    /// Hide the browser. Its tabs stay.
    Close,
    /// Show this in the status bar for a moment.
    Notice(String),
    /// Open this link in the program the system uses for it.
    OpenExternal(ExternalUrl),
    /// Put this text on the clipboard.
    Copy(String),
    /// Read the clipboard and hand its text to [`DocsBrowser::paste`].
    Paste,
}

/// One page a tab has shown, and where its reader was in it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    id: DocId,
    fragment: Option<String>,
    /// The row the view starts at.
    scroll: usize,
    /// The link with the keyboard focus.
    focus: Option<usize>,
    /// The fragment has been scrolled to.
    jumped: bool,
}

impl Entry {
    fn new(id: DocId, fragment: Option<String>) -> Self {
        Self {
            id,
            jumped: fragment.is_none(),
            fragment,
            scroll: 0,
            focus: None,
        }
    }
}

/// A tab: the pages it has shown and which one it shows now.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DocTab {
    history: Vec<Entry>,
    cursor: usize,
}

impl DocTab {
    fn current(&self) -> &Entry {
        &self.history[self.cursor]
    }

    fn current_mut(&mut self) -> &mut Entry {
        &mut self.history[self.cursor]
    }
}

/// The list of every tab, dropped from the strip's right end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TabMenu {
    focus: usize,
}

/// What a page was laid out for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    id: DocId,
    cols: usize,
    filter: LanguageFilter,
    /// The language of the page that was found, `None` for no page.
    language: Option<String>,
}

/// The active tab's page, laid out.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shown {
    key: Key,
    layout: Layout,
    title: String,
    /// The path of the page above its title.
    crumb: String,
    /// The pages the page lists as related reading, with their titles.
    related: Vec<(DocId, String)>,
}

/// The documentation browser's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsBrowser {
    font: BitmapFont,
    geometry: Geometry,
    tabs: Vec<DocTab>,
    active: usize,
    /// The first tab the strip shows.
    strip_scroll: usize,
    filter: LanguageFilter,
    nav_collapsed: bool,
    search: Option<Search>,
    menu: Option<TabMenu>,
    shown: Option<Shown>,
}

impl DocsBrowser {
    pub fn new(metrics: Metrics, font: BitmapFont) -> Self {
        Self {
            font,
            geometry: Geometry::new(metrics),
            tabs: Vec::new(),
            active: 0,
            strip_scroll: 0,
            filter: LanguageFilter::default(),
            nav_collapsed: false,
            search: None,
            menu: None,
            shown: None,
        }
    }

    /// Whether SEARCH DOCS or the list of tabs is open over the page.
    pub fn has_overlay(&self) -> bool {
        self.search.is_some() || self.menu.is_some()
    }

    #[cfg(test)]
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    #[cfg(test)]
    pub fn active_tab(&self) -> usize {
        self.active
    }

    /// The page the active tab shows.
    pub fn current(&self) -> Option<&DocId> {
        self.tabs.get(self.active).map(|tab| &tab.current().id)
    }

    #[cfg(test)]
    pub fn filter(&self) -> LanguageFilter {
        self.filter
    }

    #[cfg(test)]
    pub fn nav_collapsed(&self) -> bool {
        self.nav_collapsed
    }

    /// Prepares the browser to be shown: with no tabs yet it opens the
    /// guide's front page.
    pub fn show(&mut self, source: Source<'_>) {
        if self.tabs.is_empty() {
            let home = DocId::parse(HOME)
                .ok()
                .filter(|id| source.library.page(id, source.chain).is_some());
            let home = home.or_else(|| nav::order(source.library, source.chain).into_iter().next());
            if let Some(id) = home {
                self.tabs.push(DocTab {
                    history: vec![Entry::new(id, None)],
                    cursor: 0,
                });
                self.active = 0;
            }
        }
        self.search = None;
        self.menu = None;
        self.refresh(source);
    }

    /// Shows page `id`, at `fragment` when given, in the active tab or in a
    /// tab of its own. A page a tab already shows is brought forward
    /// instead of opened twice when it goes in a new tab.
    pub fn open(
        &mut self,
        source: Source<'_>,
        id: DocId,
        fragment: Option<String>,
        new_tab: bool,
    ) -> Option<DocsRequest> {
        let request = self.navigate(id, fragment, new_tab);
        self.refresh(source);
        request
    }

    fn navigate(
        &mut self,
        id: DocId,
        fragment: Option<String>,
        new_tab: bool,
    ) -> Option<DocsRequest> {
        if new_tab || self.tabs.is_empty() {
            if let Some(at) = self.tabs.iter().position(|tab| tab.current().id == id) {
                self.select_tab(at);
                if fragment.is_some() {
                    self.push(id, fragment);
                }
                return None;
            }
            if self.tabs.len() >= MAX_TABS {
                return Some(DocsRequest::Notice(format!(
                    "Docs tab limit reached ({MAX_TABS}). Close a tab first."
                )));
            }
            self.tabs.push(DocTab {
                history: vec![Entry::new(id, fragment)],
                cursor: 0,
            });
            self.select_tab(self.tabs.len() - 1);
            return None;
        }
        self.push(id, fragment);
        None
    }

    /// Adds a page to the active tab's history, dropping what lay ahead of
    /// the page it was left on.
    fn push(&mut self, id: DocId, fragment: Option<String>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let current = tab.current();
        if current.id == id && current.fragment == fragment {
            if fragment.is_some() {
                tab.current_mut().jumped = false;
            }
            return;
        }
        tab.history.truncate(tab.cursor + 1);
        tab.history.push(Entry::new(id, fragment));
        tab.cursor += 1;
    }

    fn select_tab(&mut self, at: usize) {
        self.active = at.min(self.tabs.len().saturating_sub(1));
    }

    fn close_tab(&mut self, at: usize) -> Option<DocsRequest> {
        if at >= self.tabs.len() {
            return None;
        }
        self.tabs.remove(at);
        if self.tabs.is_empty() {
            self.active = 0;
            self.strip_scroll = 0;
            self.shown = None;
            return Some(DocsRequest::Close);
        }
        if at < self.active || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        self.strip_scroll = self.strip_scroll.min(self.tabs.len() - 1);
        None
    }

    /// Goes back or forward `by` pages in the active tab.
    fn travel(&mut self, backward: bool) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if backward {
            tab.cursor = tab.cursor.saturating_sub(1);
        } else if tab.cursor + 1 < tab.history.len() {
            tab.cursor += 1;
        }
    }

    fn can_go(&self, backward: bool) -> bool {
        self.tabs.get(self.active).is_some_and(|tab| {
            if backward {
                tab.cursor > 0
            } else {
                tab.cursor + 1 < tab.history.len()
            }
        })
    }

    /// The title of every tab's page, shortened to fit its tab.
    fn labels(&self, source: Source<'_>) -> Vec<String> {
        self.tabs
            .iter()
            .map(|tab| tabs::label(&self.font, &title_of(source, &tab.current().id)))
            .collect()
    }

    /// Lays the active page out again when it, the width or the filter
    /// changed, and keeps the view inside it.
    fn refresh(&mut self, source: Source<'_>) {
        let labels = self.labels(source);
        if self.active < labels.len() {
            self.strip_scroll =
                self.geometry
                    .reveal(&self.font, &labels, self.strip_scroll, self.active);
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            self.shown = None;
            return;
        };
        let cols = self.geometry.cols(self.nav_collapsed);
        let id = tab.current().id.clone();
        let resolved = source.library.page(&id, source.chain);
        let key = Key {
            id: id.clone(),
            cols,
            filter: self.filter,
            language: resolved.map(|resolved| resolved.page.language().to_string()),
        };
        if self.shown.as_ref().is_none_or(|shown| shown.key != key) {
            self.shown = Some(self.build(source, key));
        }
        let height = self.geometry.text(self.nav_collapsed).height;
        let (Some(shown), Some(tab)) = (&self.shown, self.tabs.get_mut(self.active)) else {
            return;
        };
        let entry = tab.current_mut();
        if !entry.jumped {
            entry.jumped = true;
            let row = entry
                .fragment
                .as_deref()
                .and_then(|anchor| shown.layout.anchor_row(anchor));
            entry.scroll = row.unwrap_or(entry.scroll);
        }
        entry.scroll = entry.scroll.min(shown.layout.max_scroll(height));
        entry.focus = entry.focus.filter(|&link| link < shown.layout.links.len());
    }

    fn build(&self, source: Source<'_>, key: Key) -> Shown {
        let Some(resolved) = source.library.page(&key.id, source.chain) else {
            let layout = layout::message(&self.font, &format!("No page {}", key.id));
            return Shown {
                title: key.id.to_string(),
                crumb: String::new(),
                related: Vec::new(),
                layout,
                key,
            };
        };
        let page = resolved.page;
        let entry = source.library.api().get(&key.id);
        let mut tag = match entry {
            Some(entry) => {
                let kind = match entry.kind() {
                    Kind::Function => "fn",
                    Kind::Namespace => "ns",
                    Kind::ConstantGroup => "const",
                    Kind::Keyword => "kw",
                    Kind::Callback => "cb",
                };
                format!("{kind}  {}", entry.category())
            }
            None => key.id.section().to_owned(),
        };
        if resolved.is_fallback {
            tag = format!("[{}] {tag}", page.language());
        }
        let layout = layout::build(&Params {
            page,
            tag,
            cols: key.cols,
            filter: key.filter,
            font: &self.font,
        });
        let mut segments: Vec<&str> = key.id.as_str().split('/').collect();
        segments.pop();
        let crumb = std::iter::once("Glaux")
            .chain(segments)
            .collect::<Vec<_>>()
            .join(" > ")
            + " > ";
        let related = nav::related(page)
            .into_iter()
            .map(|id| {
                let title = title_of(source, &id);
                (id, title)
            })
            .collect();
        Shown {
            title: page
                .title()
                .map_or_else(|| key.id.to_string(), str::to_owned),
            crumb,
            related,
            layout,
            key,
        }
    }

    /// The view's top row of the active page.
    fn scroll(&self) -> usize {
        self.tabs
            .get(self.active)
            .map_or(0, |tab| tab.current().scroll)
    }

    fn page_height(&self) -> i32 {
        self.geometry.text(self.nav_collapsed).height
    }

    fn set_scroll(&mut self, row: usize) {
        let height = self.page_height();
        let Some(max) = self
            .shown
            .as_ref()
            .map(|shown| shown.layout.max_scroll(height))
        else {
            return;
        };
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.current_mut().scroll = row.min(max);
        }
    }

    fn scroll_by(&mut self, rows: isize) {
        self.set_scroll(self.scroll().saturating_add_signed(rows));
    }

    /// Rows a page key moves the view: a screenful less one.
    fn screenful(&self) -> isize {
        let height = self.page_height();
        let rows = self
            .shown
            .as_ref()
            .map_or(1, |shown| shown.layout.visible(self.scroll(), height).len());
        isize::try_from(rows.saturating_sub(1).max(1)).unwrap_or(1)
    }

    /// Scrolls until `row` is in view, moving as little as it can.
    fn reveal_row(&mut self, row: usize) {
        let height = self.page_height();
        let Some(shown) = &self.shown else {
            return;
        };
        let mut scroll = self.scroll();
        if row < scroll {
            scroll = row;
        } else {
            while !shown.layout.visible(scroll, height).contains(&row) && scroll < row {
                scroll += 1;
            }
        }
        self.set_scroll(scroll);
    }

    fn focus_link(&mut self, forward: bool) {
        let height = self.page_height();
        let Some(shown) = &self.shown else {
            return;
        };
        let count = shown.layout.links.len();
        if count == 0 {
            return;
        }
        let scroll = self.scroll();
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let visible = shown.layout.visible(scroll, height);
        let next = match (tab.current().focus, forward) {
            (Some(link), true) => (link + 1) % count,
            (Some(link), false) => (link + count - 1) % count,
            (None, true) => (0..count)
                .find(|&link| shown.layout.link_rows[link] >= scroll)
                .unwrap_or(0),
            (None, false) => (0..count)
                .rev()
                .find(|&link| shown.layout.link_rows[link] < visible.end)
                .unwrap_or(count - 1),
        };
        tab.current_mut().focus = Some(next);
        let row = shown.layout.link_rows[next];
        self.reveal_row(row);
    }

    /// Follows the link `target` from the page being read.
    fn follow(&mut self, source: Source<'_>, target: &str, new_tab: bool) -> Option<DocsRequest> {
        if let Some(fragment) = target.strip_prefix('#') {
            let id = self.current()?.clone();
            return self.open(source, id, Some(fragment.to_owned()), false);
        }
        match DocId::from_uri(target) {
            Some(Ok(id)) => {
                if source.library.page(&id, source.chain).is_none() {
                    return Some(DocsRequest::Notice(format!("Broken link: {target}")));
                }
                let fragment = target
                    .split_once('#')
                    .map(|(_, fragment)| fragment.to_owned());
                self.open(source, id, fragment, new_tab)
            }
            Some(Err(_)) => Some(DocsRequest::Notice(format!("Broken link: {target}"))),
            None => Some(match ExternalUrl::parse(target) {
                Ok(url) => DocsRequest::OpenExternal(url),
                Err(ExternalUrlError::Scheme(scheme)) => {
                    DocsRequest::Notice(format!("Blocked link: {scheme}"))
                }
                Err(_) => DocsRequest::Notice(format!("Broken link: {target}")),
            }),
        }
    }

    /// The target of the link with the keyboard focus.
    fn focused_target(&self) -> Option<String> {
        let focus = self.tabs.get(self.active)?.current().focus?;
        self.shown.as_ref()?.layout.links.get(focus).cloned()
    }

    /// The row of the page and the cell in it that `at` points at.
    fn page_cell(&self, at: Point) -> Option<(usize, usize)> {
        let (y, col) = self.geometry.text_hit(self.nav_collapsed, at)?;
        let shown = self.shown.as_ref()?;
        let mut top = 0;
        for row in shown.layout.rows.iter().enumerate().skip(self.scroll()) {
            let height = row.1.height(shown.layout.cell_h);
            if top + height > self.page_height() {
                return None;
            }
            if y < top + height {
                return Some((row.0, col));
            }
            top += height;
        }
        None
    }

    fn link_at(&self, at: Point) -> Option<usize> {
        let (row, col) = self.page_cell(at)?;
        self.shown.as_ref()?.layout.link_at(row, col)
    }

    fn open_search(&mut self, source: Source<'_>) {
        self.menu = None;
        self.search = Some(Search::new(source));
    }

    fn options_bar(&self) -> OptionsBar {
        self.geometry.options_bar(&self.font, self.filter)
    }

    fn search_layout(&self, area: Rect) -> SearchLayout {
        let labels = search::Scope::ALL
            .map(|scope| i32::try_from(self.font.text_cells(scope.label())).unwrap_or(0));
        SearchLayout::new(
            area,
            self.geometry.metrics.cell_w,
            self.geometry.metrics.cell_h,
            labels,
        )
    }

    /// Where SEARCH DOCS goes: centred below the workspace's tab bar.
    fn search_area(&self) -> Rect {
        Rect {
            x: 0,
            y: self.geometry.metrics.top,
            width: self.geometry.strip.width,
            height: self.geometry.status.bottom() - self.geometry.metrics.top,
        }
    }

    /// Inserts the clipboard's `text` into the open SEARCH DOCS query.
    pub fn paste(&mut self, source: Source<'_>, text: Option<String>) {
        if let (Some(search), Some(text)) = (&mut self.search, text) {
            search.paste(&text, source);
        }
    }

    /// Applies `event`. While SEARCH DOCS or the list of tabs is open it
    /// takes every event.
    pub fn handle_input(&mut self, event: InputEvent, source: Source<'_>) -> Option<DocsRequest> {
        let request = self.dispatch(event, source);
        self.refresh(source);
        request
    }

    fn dispatch(&mut self, event: InputEvent, source: Source<'_>) -> Option<DocsRequest> {
        if let Some(mut search) = self.search.take() {
            let layout = self.search_layout(self.search_area());
            let action = search.handle_input(event, &layout, source);
            return match action {
                None => {
                    self.search = Some(search);
                    None
                }
                Some(SearchAction::Close) => None,
                Some(SearchAction::Open { id, new_tab }) => self.open(source, id, None, new_tab),
                Some(SearchAction::Copy(text)) => {
                    self.search = Some(search);
                    Some(DocsRequest::Copy(text))
                }
                Some(SearchAction::Paste) => {
                    self.search = Some(search);
                    Some(DocsRequest::Paste)
                }
            };
        }
        if let Some(menu) = self.menu {
            return self.menu_input(event, menu);
        }
        match event {
            InputEvent::Break | InputEvent::Docs => return Some(DocsRequest::Close),
            InputEvent::Up { .. } => self.scroll_by(-1),
            InputEvent::Down { .. } => self.scroll_by(1),
            InputEvent::PageUp { .. } => self.scroll_by(-self.screenful()),
            InputEvent::PageDown { .. } => self.scroll_by(self.screenful()),
            InputEvent::Home { .. } | InputEvent::DocumentStart { .. } => self.set_scroll(0),
            InputEvent::End { .. } | InputEvent::DocumentEnd { .. } => self.set_scroll(usize::MAX),
            InputEvent::Wheel { delta } => self.scroll_by(-isize::from(delta) * WHEEL_ROWS),
            InputEvent::WheelHorizontal { delta } => {
                let last = self.tabs.len().saturating_sub(1);
                self.strip_scroll = self
                    .strip_scroll
                    .saturating_add_signed(-isize::from(delta))
                    .min(last);
            }
            InputEvent::Tab { reverse } => self.focus_link(!reverse),
            InputEvent::Enter => {
                let target = self.focused_target()?;
                return self.follow(source, &target, false);
            }
            InputEvent::Text('t' | 'T') => {
                let target = self.focused_target()?;
                return self.follow(source, &target, true);
            }
            InputEvent::Text('l' | 'L') => self.filter = self.filter.next(),
            InputEvent::Back => self.travel(true),
            InputEvent::Forward => self.travel(false),
            InputEvent::ToggleSidebar => self.nav_collapsed = !self.nav_collapsed,
            InputEvent::Find => self.open_search(source),
            InputEvent::CloseDocument => return self.close_tab(self.active),
            InputEvent::NextDocument if !self.tabs.is_empty() => {
                self.select_tab((self.active + 1) % self.tabs.len());
            }
            InputEvent::PreviousDocument if !self.tabs.is_empty() => {
                self.select_tab((self.active + self.tabs.len() - 1) % self.tabs.len());
            }
            InputEvent::PointerDown { button, at, select } => {
                return self.pointer_down(source, button, at, select);
            }
            _ => {}
        }
        None
    }

    fn menu_input(&mut self, event: InputEvent, menu: TabMenu) -> Option<DocsRequest> {
        let count = self.tabs.len();
        match event {
            InputEvent::Break => self.menu = None,
            InputEvent::Up { .. } => {
                self.menu = Some(TabMenu {
                    focus: (menu.focus + count - 1) % count.max(1),
                });
            }
            InputEvent::Down { .. } => {
                self.menu = Some(TabMenu {
                    focus: (menu.focus + 1) % count.max(1),
                });
            }
            InputEvent::Enter => {
                self.menu = None;
                self.select_tab(menu.focus);
            }
            InputEvent::PointerDown { at, .. } => {
                let (window, rows) = self.geometry.menu_rows(count, menu.focus);
                self.menu = None;
                if window.contains(at)
                    && let Some((index, _)) = rows.iter().find(|(_, rect)| rect.contains(at))
                {
                    self.select_tab(*index);
                }
            }
            _ => {}
        }
        None
    }

    fn pointer_down(
        &mut self,
        source: Source<'_>,
        button: PointerButton,
        at: Point,
        select: bool,
    ) -> Option<DocsRequest> {
        let primary = button == PointerButton::Primary;
        let new_tab = select || button == PointerButton::Middle;
        let labels = self.labels(source);
        match self
            .geometry
            .strip_hit(&self.font, &labels, self.strip_scroll, at)
        {
            StripHit::Tab(index) if button == PointerButton::Middle => {
                return self.close_tab(index);
            }
            StripHit::Tab(index) if primary => {
                self.select_tab(index);
                return None;
            }
            StripHit::Close(index) if primary => return self.close_tab(index),
            StripHit::More if primary => {
                self.menu = Some(TabMenu { focus: self.active });
                return None;
            }
            StripHit::None => {}
            _ => return None,
        }
        if !primary && button != PointerButton::Middle {
            return None;
        }
        let bar = self.options_bar();
        if self.geometry.options.contains(at) {
            if !primary {
                return None;
            }
            if bar.back.contains(at) {
                self.travel(true);
            } else if bar.forward.contains(at) {
                self.travel(false);
            } else if bar.filter.contains(at) {
                self.filter = self.filter.next();
            } else if bar.nav.contains(at) {
                self.nav_collapsed = !self.nav_collapsed;
            } else if bar.search.contains(at) {
                self.open_search(source);
            }
            return None;
        }
        if let Some(item) = self.panel_at(at) {
            return match item {
                PanelHit::Heading(row) => {
                    self.set_scroll(row);
                    None
                }
                PanelHit::Related(id) => self.open(source, id, None, new_tab),
            };
        }
        let link = self.link_at(at)?;
        let target = self.shown.as_ref()?.layout.links.get(link)?.clone();
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.current_mut().focus = Some(link);
        }
        self.follow(source, &target, new_tab)
    }

    /// What the side panel has at `at`.
    fn panel_at(&self, at: Point) -> Option<PanelHit> {
        let rows = self.panel_rows()?;
        let row = rows.into_iter().find(|row| row.rect.contains(at))?;
        match row.kind {
            PanelKind::Outline { row, .. } => Some(PanelHit::Heading(row)),
            PanelKind::Related(index) => {
                let (id, _) = self.shown.as_ref()?.related.get(index)?;
                Some(PanelHit::Related(id.clone()))
            }
            PanelKind::Title(_) => None,
        }
    }

    /// The pointer to show at `at`.
    pub fn cursor(&self, at: Point, source: Source<'_>) -> SystemCursor {
        let hand = |clickable: bool| {
            if clickable {
                SystemCursor::Hand
            } else {
                SystemCursor::Arrow
            }
        };
        if let Some(search) = &self.search {
            let layout = self.search_layout(self.search_area());
            let over = layout.scope_at(at).is_some()
                || layout
                    .row_at(at)
                    .is_some_and(|row| search.first() + row < search.results().len());
            return hand(over);
        }
        if let Some(menu) = self.menu {
            let (_, rows) = self.geometry.menu_rows(self.tabs.len(), menu.focus);
            return hand(rows.iter().any(|(_, rect)| rect.contains(at)));
        }
        let labels = self.labels(source);
        if self
            .geometry
            .strip_hit(&self.font, &labels, self.strip_scroll, at)
            != StripHit::None
        {
            return SystemCursor::Hand;
        }
        let bar = self.options_bar();
        let buttons = [bar.back, bar.forward, bar.filter, bar.nav, bar.search];
        let over_button = buttons.iter().any(|rect| rect.contains(at));
        hand(over_button || self.panel_at(at).is_some() || self.link_at(at).is_some())
    }

    /// The target of the link under `at`, for the status bar.
    pub fn hovered_target(&self, at: Point) -> Option<&str> {
        if self.has_overlay() {
            return None;
        }
        let link = self.link_at(at)?;
        self.shown
            .as_ref()?
            .layout
            .links
            .get(link)
            .map(String::as_str)
    }

    /// What the status bar says while nothing else is to be reported.
    pub fn hint(&self) -> &'static str {
        if self.search.is_some() {
            SEARCH_HINT
        } else if self.menu.is_some() {
            MENU_HINT
        } else {
            DOCS_HINT
        }
    }
}

/// What a click in the side panel does.
enum PanelHit {
    Heading(usize),
    Related(DocId),
}

/// One line of the side panel.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PanelRow {
    rect: Rect,
    kind: PanelKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PanelKind {
    Title(&'static str),
    /// A heading of the page, and whether the view is in its section.
    Outline {
        row: usize,
        active: bool,
    },
    /// The page listed at this position in the page's related reading.
    Related(usize),
}

impl DocsBrowser {
    /// The side panel's lines, or `None` while it is hidden: the page's
    /// outline, then its related reading, as many as fit.
    fn panel_rows(&self) -> Option<Vec<PanelRow>> {
        let nav = self.geometry.nav(self.nav_collapsed)?;
        let shown = self.shown.as_ref()?;
        let cell_h = self.geometry.metrics.cell_h;
        let room = usize::try_from((nav.height - 4) / cell_h.max(1)).unwrap_or(0);
        let mut lines = Vec::new();
        let level = if shown
            .layout
            .headings
            .iter()
            .any(|h| h.level == OUTLINE_LEVEL)
        {
            OUTLINE_LEVEL
        } else {
            OUTLINE_LEVEL + 1
        };
        let outline: Vec<_> = shown
            .layout
            .headings
            .iter()
            .filter(|heading| heading.level == level)
            .collect();
        let scroll = self.scroll();
        let current = outline.iter().rposition(|heading| heading.row <= scroll);
        if !outline.is_empty() {
            lines.push(PanelKind::Title("On this page"));
            for (index, heading) in outline.iter().enumerate() {
                lines.push(PanelKind::Outline {
                    row: heading.row,
                    active: current == Some(index),
                });
            }
        }
        if !shown.related.is_empty() {
            lines.push(PanelKind::Title("Related"));
            lines.extend((0..shown.related.len()).map(PanelKind::Related));
        }
        lines.truncate(room);
        Some(
            lines
                .into_iter()
                .enumerate()
                .map(|(index, kind)| PanelRow {
                    rect: Rect {
                        x: nav.x,
                        y: nav.y + 2 + index as i32 * cell_h,
                        width: nav.width - 1,
                        height: cell_h,
                    },
                    kind,
                })
                .collect(),
        )
    }
}

/// The title of page `id` in the language it is read in, or its id.
pub fn title_of(source: Source<'_>, id: &DocId) -> String {
    source
        .library
        .page(id, source.chain)
        .and_then(|resolved| resolved.page.title().map(str::to_owned))
        .unwrap_or_else(|| id.to_string())
}

#[cfg(test)]
mod tests;
