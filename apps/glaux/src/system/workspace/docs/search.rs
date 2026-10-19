//! SEARCH DOCS: a window to find a page by name, listing every page while
//! nothing is typed.

use glaux_docs::{DocId, Kind};
use glaux_input::{InputEvent, Point, PointerButton};

use super::nav;
use super::{Source, title_of};
use crate::system::workspace::Rect;
use crate::system::workspace::dialog::{AREA_MARGIN, PAD, SHADOW_OFFSET};
use crate::system::workspace::text_field::{FieldInput, TextField};
use crate::ui_text;

/// Rows of results the window shows at once.
pub const LIST_ROWS: usize = 7;
/// Most characters kept in the query.
const MAX_QUERY: usize = 32;
/// Cells across the window's text.
const WINDOW_CELLS: i32 = 40;
/// Space around a filter's label.
const CHIP_PAD: i32 = 3;
/// Gap between two filters.
const CHIP_GAP: i32 = 4;
/// Space between the window's lines.
const LINE_GAP: i32 = 3;

/// Which pages a search looks at, by the first part of their id.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    All,
    Api,
    Guides,
}

impl Scope {
    pub const ALL: [Self; 3] = [Self::All, Self::Api, Self::Guides];

    pub fn label(self) -> String {
        match self {
            Self::All => ui_text::get("docs-all"),
            Self::Api => ui_text::get("docs-api"),
            Self::Guides => ui_text::get("docs-guides"),
        }
    }

    fn includes(self, id: &DocId) -> bool {
        match self {
            Self::All => true,
            Self::Api => id.section() == "api",
            Self::Guides => id.section() == "guide",
        }
    }

    fn step(self, forward: bool) -> Self {
        let at = Self::ALL
            .iter()
            .position(|scope| *scope == self)
            .unwrap_or(0);
        let count = Self::ALL.len();
        Self::ALL[(at + if forward { 1 } else { count - 1 }) % count]
    }
}

/// One page found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub id: DocId,
    pub title: String,
    /// A letter for what the page is: `f` for a function, `g` for a guide.
    pub mark: char,
    /// The category of an API page, or the section of any other.
    pub tag: String,
    /// The page is not in the language asked for.
    pub is_fallback: bool,
}

/// What the user did with the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchAction {
    /// Show this page, in a tab of its own when `new_tab` is set.
    Open {
        id: DocId,
        new_tab: bool,
    },
    Close,
    Copy(String),
    Paste,
}

/// An open SEARCH DOCS window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    field: TextField,
    scope: Scope,
    results: Vec<Found>,
    /// Position of the highlighted result.
    focus: usize,
    /// Position of the first result shown.
    first: usize,
}

impl Search {
    pub fn new(source: Source<'_>) -> Self {
        let mut search = Self {
            field: TextField::any(MAX_QUERY),
            scope: Scope::All,
            results: Vec::new(),
            focus: 0,
            first: 0,
        };
        search.refresh(source);
        search
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub fn results(&self) -> &[Found] {
        &self.results
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    pub fn first(&self) -> usize {
        self.first
    }

    pub fn field(&self) -> &TextField {
        &self.field
    }

    /// Inserts the clipboard's `text` into the query.
    pub fn paste(&mut self, text: &str, source: Source<'_>) {
        self.field.paste(text);
        self.refresh(source);
    }

    fn refresh(&mut self, source: Source<'_>) {
        let query = self.field.text();
        let ids: Vec<(DocId, bool)> = if query.trim().is_empty() {
            nav::order(source.library, source.chain)
                .into_iter()
                .map(|id| (id, false))
                .collect()
        } else {
            source
                .library
                .search(&query, source.chain)
                .into_iter()
                .map(|hit| (hit.id, hit.is_fallback))
                .collect()
        };
        self.results = ids
            .into_iter()
            .filter(|(id, _)| self.scope.includes(id))
            .map(|(id, is_fallback)| describe(source, id, is_fallback))
            .collect();
        self.focus = 0;
        self.first = 0;
    }

    fn move_focus(&mut self, by: isize) {
        let last = self.results.len().saturating_sub(1);
        self.focus = self.focus.saturating_add_signed(by).min(last);
        if self.focus < self.first {
            self.first = self.focus;
        } else if self.focus >= self.first + LIST_ROWS {
            self.first = self.focus + 1 - LIST_ROWS;
        }
    }

    /// Applies `event`. Every event is taken while the window is open.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        layout: &SearchLayout,
        source: Source<'_>,
    ) -> Option<SearchAction> {
        match event {
            InputEvent::Break => return Some(SearchAction::Close),
            InputEvent::Enter => return self.chosen(false),
            InputEvent::Up { .. } => self.move_focus(-1),
            InputEvent::Down { .. } => self.move_focus(1),
            InputEvent::PageUp { .. } => self.move_focus(-(LIST_ROWS as isize)),
            InputEvent::PageDown { .. } => self.move_focus(LIST_ROWS as isize),
            InputEvent::Wheel { delta } => self.move_focus(-isize::from(delta)),
            InputEvent::Tab { reverse } => {
                self.scope = self.scope.step(!reverse);
                self.refresh(source);
            }
            InputEvent::PointerDown { button, at, select } => {
                if !layout.window.contains(at) {
                    return Some(SearchAction::Close);
                }
                if let Some(scope) = layout.scope_at(at) {
                    if scope != self.scope {
                        self.scope = scope;
                        self.refresh(source);
                    }
                } else if let Some(row) = layout.row_at(at) {
                    let result = self.results.get(self.first + row)?;
                    let new_tab = select || button == PointerButton::Middle;
                    return Some(SearchAction::Open {
                        id: result.id.clone(),
                        new_tab,
                    });
                }
            }
            event => match self.field.handle_input(event) {
                FieldInput::Edited => self.refresh(source),
                FieldInput::Copy(text) => return Some(SearchAction::Copy(text)),
                FieldInput::Paste => return Some(SearchAction::Paste),
                FieldInput::Kept | FieldInput::Unhandled => {}
            },
        }
        None
    }

    fn chosen(&self, new_tab: bool) -> Option<SearchAction> {
        let result = self.results.get(self.focus)?;
        Some(SearchAction::Open {
            id: result.id.clone(),
            new_tab,
        })
    }
}

/// How the window lists `id`.
fn describe(source: Source<'_>, id: DocId, is_fallback: bool) -> Found {
    let entry = source.library.api().get(&id);
    let mark = match (id.section(), entry.map(glaux_docs::ApiEntry::kind)) {
        ("guide", _) => 'g',
        (_, Some(Kind::Function | Kind::Callback)) => 'f',
        (_, Some(Kind::Namespace)) => 'n',
        (_, Some(Kind::ConstantGroup)) => 'c',
        (_, Some(Kind::Keyword)) => 'k',
        _ => 'i',
    };
    let tag = match entry {
        Some(entry) => entry.category().to_owned(),
        None => id.section().to_owned(),
    };
    Found {
        title: title_of(source, &id),
        id,
        mark,
        tag,
        is_fallback,
    }
}

/// Where the window's parts are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchLayout {
    pub window: Rect,
    pub title_bar: Rect,
    pub field: Rect,
    pub scopes: [Rect; 3],
    /// Right-aligned beside the scopes: how many pages were found.
    pub count: Rect,
    pub rows: Vec<Rect>,
    pub text_x: i32,
    pub cell_w: i32,
    pub cell_h: i32,
}

impl SearchLayout {
    /// The window centred in `area`.
    pub fn new(area: Rect, cell_w: i32, cell_h: i32, labels: [i32; 3]) -> Self {
        let cell_w = cell_w.max(1);
        let inner = (WINDOW_CELLS * cell_w).min(area.width - 2 * AREA_MARGIN - 2 * PAD - 2);
        let width = inner + 2 * PAD + 2;
        let title_h = cell_h + 2;
        let field_h = cell_h + 4;
        let chip_h = cell_h;
        let list_h = LIST_ROWS as i32 * cell_h;
        let height = 1 + title_h + PAD + field_h + LINE_GAP + chip_h + LINE_GAP + list_h + PAD + 1;
        let window = Rect {
            x: area.x + (area.width - width - SHADOW_OFFSET) / 2,
            y: area.y + (area.height - height - SHADOW_OFFSET) / 2,
            width,
            height,
        };
        let title_bar = Rect {
            x: window.x + 1,
            y: window.y + 1,
            width: width - 2,
            height: title_h,
        };
        let text_x = window.x + 1 + PAD;
        let field = Rect {
            x: text_x,
            y: title_bar.bottom() + PAD,
            width: inner,
            height: field_h,
        };
        let chips_y = field.bottom() + LINE_GAP;
        let mut x = text_x;
        let scopes = labels.map(|cells| {
            let rect = Rect {
                x,
                y: chips_y,
                width: cells * cell_w + 2 * CHIP_PAD,
                height: chip_h,
            };
            x += rect.width + CHIP_GAP;
            rect
        });
        let count = Rect {
            x,
            y: chips_y,
            width: (text_x + inner - x).max(0),
            height: chip_h,
        };
        let list_y = chips_y + chip_h + LINE_GAP;
        let rows = (0..LIST_ROWS)
            .map(|row| Rect {
                x: text_x,
                y: list_y + row as i32 * cell_h,
                width: inner,
                height: cell_h,
            })
            .collect();
        Self {
            window,
            title_bar,
            field,
            scopes,
            count,
            rows,
            text_x,
            cell_w,
            cell_h,
        }
    }

    pub fn scope_at(&self, at: Point) -> Option<Scope> {
        self.scopes
            .iter()
            .zip(Scope::ALL)
            .find(|(rect, _)| rect.contains(at))
            .map(|(_, scope)| scope)
    }

    /// The row of the list under `at`, counted from the first shown.
    pub fn row_at(&self, at: Point) -> Option<usize> {
        self.rows.iter().position(|rect| rect.contains(at))
    }
}

#[cfg(test)]
mod tests;
