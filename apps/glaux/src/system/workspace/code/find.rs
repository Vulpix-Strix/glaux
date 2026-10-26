//! The find bar searches the active document and sits docked above its
//! text. It can also replace matches on demand.
//!
//! The bar stays open while the text has the keys, so matches keep
//! showing as the text is edited. Searching is literal. See
//! [`search`](crate::system::editing::search).

use std::ops::Range;

use glaux_input::{InputEvent, Point};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{draw_text, fill_rect};

use crate::system::editing::document::{CodeDocument, DocumentId};
use crate::system::editing::search::{SearchOptions, find_all};
use crate::system::screen::{ACCENT, BACKGROUND, DIM, ERROR, PALE, TEXT};
use crate::system::workspace::text_field::{FieldColors, FieldInput, TextField};
use crate::system::workspace::{BAR, HOVER_SHADE, IDLE_SHADE, Rect};
use crate::ui_text;

/// Most characters a query or replacement holds.
const MAX_QUERY: usize = 128;
/// Space around the bar's parts.
const GAP: i32 = 2;
/// Cells of the match count, as in "999/999".
const COUNT_CELLS: i32 = 7;

/// A part of the bar that takes the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindFocus {
    Query,
    Replacement,
    ReplaceOne,
    ReplaceAll,
    MatchCase,
    WholeWord,
}

/// What the bar asks of the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindAction {
    /// Nothing for the view to do.
    None,
    /// The input is not the bar's, so the view handles it.
    Unhandled,
    /// The query or the options changed: show the match nearest the
    /// selection's start.
    Search,
    Next,
    Previous,
    ReplaceOne,
    ReplaceAll,
    /// Put this text on the clipboard.
    Copy(String),
    /// Paste the clipboard into the focused field.
    Paste,
    Close,
}

/// A clickable part of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Query,
    Replacement,
    MatchCase,
    WholeWord,
    Previous,
    Next,
    Close,
    ReplaceOne,
    ReplaceAll,
}

/// The open bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindBar {
    query: TextField,
    replacement: TextField,
    replacing: bool,
    options: SearchOptions,
    /// The part with the keys, `None` while the text has them.
    focus: Option<FindFocus>,
    matches: Vec<Range<usize>>,
    /// What `matches` were found for: the document and its revision, the
    /// query and the options.
    searched: Option<(DocumentId, u64, String, SearchOptions)>,
}

impl FindBar {
    /// A bar searching for `query`, with the replacement row if
    /// `replacing`, its query field focused.
    pub fn new(query: &str, replacing: bool) -> Self {
        let mut bar = Self {
            query: TextField::any(MAX_QUERY),
            replacement: TextField::any(MAX_QUERY),
            replacing,
            options: SearchOptions::default(),
            focus: Some(FindFocus::Query),
            matches: Vec::new(),
            searched: None,
        };
        bar.query.set_text(query);
        bar
    }

    pub fn focus(&self) -> Option<FindFocus> {
        self.focus
    }

    /// The text field holding the keys, if the focus is on one rather than
    /// on a button or an option.
    pub fn focused_field(&self) -> Option<&TextField> {
        match self.focus? {
            FindFocus::Query => Some(&self.query),
            FindFocus::Replacement => Some(&self.replacement),
            _ => None,
        }
    }

    pub fn is_replacing(&self) -> bool {
        self.replacing
    }

    #[cfg(test)]
    pub fn query(&self) -> String {
        self.query.text()
    }

    #[cfg(test)]
    pub fn options(&self) -> SearchOptions {
        self.options
    }

    pub fn replacement(&self) -> String {
        self.replacement.text()
    }

    /// Gives the keys to the text, leaving the bar open.
    pub fn unfocus(&mut self) {
        self.focus = None;
    }

    /// Opens again with Ctrl+F or Ctrl+H: the query takes the keys,
    /// replacing too when `replacing`, and `query`, if given, replaces the
    /// one typed.
    pub fn reopen(&mut self, query: Option<&str>, replacing: bool) {
        self.replacing |= replacing;
        if let Some(query) = query {
            self.query.set_text(query);
        } else {
            self.query.handle_input(InputEvent::SelectAll);
        }
        self.focus = Some(FindFocus::Query);
    }

    /// The matches in `document`, found again when the document, the query
    /// or the options changed since the last search.
    pub fn refresh(&mut self, document: &CodeDocument) {
        let key = (
            document.id(),
            document.revision(),
            self.query.text(),
            self.options,
        );
        if self.searched.as_ref() != Some(&key) {
            self.matches = find_all(document.text(), &key.2, self.options);
            self.searched = Some(key);
        }
    }

    /// The matches in `document`, if they were found in its current text.
    pub fn matches_in(&self, document: &CodeDocument) -> &[Range<usize>] {
        match &self.searched {
            Some((id, revision, ..))
                if *id == document.id() && *revision == document.revision() =>
            {
                &self.matches
            }
            _ => &[],
        }
    }

    /// The parts focus cycles through with Tab, in order.
    fn order(&self) -> &'static [FindFocus] {
        if self.replacing {
            &[
                FindFocus::Query,
                FindFocus::Replacement,
                FindFocus::ReplaceOne,
                FindFocus::ReplaceAll,
                FindFocus::MatchCase,
                FindFocus::WholeWord,
            ]
        } else {
            &[FindFocus::Query, FindFocus::MatchCase, FindFocus::WholeWord]
        }
    }

    /// Applies a key while the bar has the keys. Inputs the focused part
    /// does not take come back as [`FindAction::Unhandled`].
    pub fn handle_input(&mut self, event: InputEvent) -> FindAction {
        let Some(focus) = self.focus else {
            return FindAction::Unhandled;
        };
        let field = match focus {
            FindFocus::Query => Some(&mut self.query),
            FindFocus::Replacement => Some(&mut self.replacement),
            _ => None,
        };
        if let Some(field) = field {
            match field.handle_input(event) {
                FieldInput::Edited if focus == FindFocus::Query => return FindAction::Search,
                FieldInput::Edited | FieldInput::Kept => return FindAction::None,
                FieldInput::Copy(text) => return FindAction::Copy(text),
                FieldInput::Paste => return FindAction::Paste,
                FieldInput::Unhandled => {}
            }
        }
        match event {
            InputEvent::Break => FindAction::Close,
            InputEvent::Tab { reverse } => {
                let order = self.order();
                let at = order.iter().position(|&part| part == focus).unwrap_or(0);
                let step = if reverse { order.len() - 1 } else { 1 };
                self.focus = Some(order[(at + step) % order.len()]);
                FindAction::None
            }
            InputEvent::Down { .. } => FindAction::Next,
            InputEvent::Up { .. } => FindAction::Previous,
            InputEvent::Enter | InputEvent::Text(' ') => match focus {
                FindFocus::Query => FindAction::Next,
                FindFocus::Replacement | FindFocus::ReplaceOne => FindAction::ReplaceOne,
                FindFocus::ReplaceAll => FindAction::ReplaceAll,
                FindFocus::MatchCase => self.toggle(|options| &mut options.match_case),
                FindFocus::WholeWord => self.toggle(|options| &mut options.whole_word),
            },
            _ => FindAction::Unhandled,
        }
    }

    fn toggle(&mut self, option: fn(&mut SearchOptions) -> &mut bool) -> FindAction {
        let flag = option(&mut self.options);
        *flag = !*flag;
        FindAction::Search
    }

    /// Inserts the clipboard's text into the focused field. Returns
    /// whether the query changed.
    pub fn paste(&mut self, text: &str) -> bool {
        match self.focus {
            Some(FindFocus::Query) => {
                self.query.paste(text);
                true
            }
            Some(FindFocus::Replacement) => {
                self.replacement.paste(text);
                false
            }
            _ => false,
        }
    }

    /// Applies a primary press at `at` inside the bar.
    pub fn click(&mut self, at: Point, layout: &FindLayout) -> FindAction {
        match layout.part_at(at, self.replacing) {
            Some(Part::Query) => {
                self.focus = Some(FindFocus::Query);
                FindAction::None
            }
            Some(Part::Replacement) => {
                self.focus = Some(FindFocus::Replacement);
                FindAction::None
            }
            Some(Part::MatchCase) => self.toggle(|options| &mut options.match_case),
            Some(Part::WholeWord) => self.toggle(|options| &mut options.whole_word),
            Some(Part::Previous) => FindAction::Previous,
            Some(Part::Next) => FindAction::Next,
            Some(Part::Close) => FindAction::Close,
            Some(Part::ReplaceOne) => FindAction::ReplaceOne,
            Some(Part::ReplaceAll) => FindAction::ReplaceAll,
            None => FindAction::None,
        }
    }

    /// Whether a press at `at` acts on a button rather than a field.
    pub fn clickable_at(&self, at: Point, layout: &FindLayout) -> bool {
        layout
            .part_at(at, self.replacing)
            .is_some_and(|part| !matches!(part, Part::Query | Part::Replacement))
    }

    /// Draws the bar. `current` is the position among `matches` of the
    /// one selected, if any. `lit` shows the focused field's cursor.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        layout: &FindLayout,
        matches: usize,
        current: Option<usize>,
        lit: bool,
    ) {
        let fill = |framebuffer: &mut Framebuffer, rect: Rect, color| {
            fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, color);
        };
        fill(framebuffer, layout.panel, BAR);
        fill(
            framebuffer,
            Rect {
                y: layout.panel.bottom() - 1,
                height: 1,
                ..layout.panel
            },
            DIM,
        );
        let field_colors = |focused: bool| FieldColors {
            ground: if focused { BACKGROUND } else { IDLE_SHADE },
            ink: TEXT,
            selection: HOVER_SHADE,
            selected_ink: TEXT,
        };
        let query_focused = self.focus == Some(FindFocus::Query);
        self.query.render(
            framebuffer,
            font,
            layout.query,
            field_colors(query_focused),
            query_focused && lit,
            layout.cell_w,
        );
        let count = match (self.query.text().is_empty(), matches, current) {
            (true, ..) => String::new(),
            (false, 0, _) => "0/0".to_owned(),
            (false, total, Some(at)) => format!("{}/{total}", at + 1),
            (false, total, None) => format!("-/{total}"),
        };
        let count_color = if matches == 0 { ERROR } else { PALE };
        draw_text(
            framebuffer,
            font,
            &count,
            layout.count.x,
            layout.text_y(layout.count),
            count_color,
        );

        let button =
            |framebuffer: &mut Framebuffer, rect: Rect, label: &str, on: bool, focused: bool| {
                let ground = if focused { HOVER_SHADE } else { BAR };
                fill(framebuffer, rect, ground);
                let ink = if on { ACCENT } else { PALE };
                if on {
                    fill(
                        framebuffer,
                        Rect {
                            y: rect.bottom() - 1,
                            height: 1,
                            ..rect
                        },
                        ACCENT,
                    );
                }
                let cells = i32::try_from(font.text_cells(label)).unwrap_or(0);
                let x = rect.x + (rect.width - cells * layout.cell_w) / 2;
                draw_text(framebuffer, font, label, x, layout.text_y(rect), ink);
            };
        let focused = |part: FindFocus| self.focus == Some(part);
        button(
            framebuffer,
            layout.match_case,
            "Aa",
            self.options.match_case,
            focused(FindFocus::MatchCase),
        );
        button(
            framebuffer,
            layout.whole_word,
            &ui_text::get("code-find-whole-word-button"),
            self.options.whole_word,
            focused(FindFocus::WholeWord),
        );
        button(framebuffer, layout.previous, "<", false, false);
        button(framebuffer, layout.next, ">", false, false);
        button(framebuffer, layout.close, "x", false, false);
        if self.replacing {
            let replacement_focused = focused(FindFocus::Replacement);
            self.replacement.render(
                framebuffer,
                font,
                layout.replacement,
                field_colors(replacement_focused),
                replacement_focused && lit,
                layout.cell_w,
            );
            button(
                framebuffer,
                layout.replace_one,
                &ui_text::get("code-replace-2"),
                false,
                focused(FindFocus::ReplaceOne),
            );
            button(
                framebuffer,
                layout.replace_all,
                &ui_text::get("code-all"),
                false,
                focused(FindFocus::ReplaceAll),
            );
        }
    }
}

/// Where the bar's parts sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindLayout {
    /// The whole bar, a rule along its bottom edge included.
    pub panel: Rect,
    pub query: Rect,
    pub count: Rect,
    pub match_case: Rect,
    pub whole_word: Rect,
    pub previous: Rect,
    pub next: Rect,
    pub close: Rect,
    /// The replacement row's parts, laid out even when it is hidden.
    pub replacement: Rect,
    pub replace_one: Rect,
    pub replace_all: Rect,
    cell_w: i32,
    cell_h: i32,
}

impl FindLayout {
    /// The bar across the top of `text`, one row tall, or two when
    /// `replacing`.
    pub fn new(text: Rect, cell_w: i32, cell_h: i32, replacing: bool) -> Self {
        let row_h = cell_h + 2;
        let rows = if replacing { 2 } else { 1 };
        let panel = Rect {
            height: GAP + rows * (row_h + GAP) + 1,
            ..text
        };
        let row = |n: i32| panel.y + GAP + n * (row_h + GAP);
        // Buttons right to left from the bar's right end.
        let mut right = panel.right() - GAP;
        let mut take = |cells: i32, y: i32| {
            let width = cells * cell_w + 2;
            right -= width;
            let rect = Rect {
                x: right,
                y,
                width,
                height: row_h,
            };
            right -= GAP;
            rect
        };
        let top = row(0);
        let close = take(1, top);
        let next = take(1, top);
        let previous = take(1, top);
        let whole_word = take(1, top);
        let match_case = take(2, top);
        let count = take(COUNT_CELLS, top);
        let fields_right = right;
        let field = |y: i32, right: i32| Rect {
            x: panel.x + GAP,
            y,
            width: right - panel.x - GAP,
            height: row_h,
        };
        let query = field(top, fields_right);
        let bottom = row(1);
        let mut right = panel.right() - GAP;
        let mut take = |cells: i32| {
            let width = cells * cell_w + 2;
            right -= width;
            let rect = Rect {
                x: right,
                y: bottom,
                width,
                height: row_h,
            };
            right -= GAP;
            rect
        };
        let replace_all = take(3);
        let replace_one = take(7);
        let replacement = field(bottom, fields_right.min(right));
        Self {
            panel,
            query,
            count,
            match_case,
            whole_word,
            previous,
            next,
            close,
            replacement,
            replace_one,
            replace_all,
            cell_w,
            cell_h,
        }
    }

    /// The top of text drawn in a part.
    fn text_y(&self, rect: Rect) -> i32 {
        rect.y + (rect.height - self.cell_h) / 2
    }

    fn part_at(&self, at: Point, replacing: bool) -> Option<Part> {
        let mut parts = vec![
            (self.query, Part::Query),
            (self.match_case, Part::MatchCase),
            (self.whole_word, Part::WholeWord),
            (self.previous, Part::Previous),
            (self.next, Part::Next),
            (self.close, Part::Close),
        ];
        if replacing {
            parts.extend([
                (self.replacement, Part::Replacement),
                (self.replace_one, Part::ReplaceOne),
                (self.replace_all, Part::ReplaceAll),
            ]);
        }
        parts
            .into_iter()
            .find(|(rect, _)| rect.contains(at))
            .map(|(_, part)| part)
    }
}

#[cfg(test)]
mod tests;
