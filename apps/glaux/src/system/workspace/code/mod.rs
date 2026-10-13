//! The Code view: tabs over the cartridge's open scripts, a read-only view
//! of the active one, and OPEN FILE to open more.
//!
//! The documents and tabs belong to the cartridge and are lent in as a
//! [`CodeSession`]; the editor keeps only what is on screen around them.

mod layout;
mod picker;
mod render;
mod settings;
mod text_view;

use std::ops::Range;

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::machine::TICK_RATE;
use glaux_machine::system_rom::SystemCursor;

pub use layout::Metrics;
use layout::{CodeLayout, StripHit, tab_labels};
use picker::{FilePicker, PickerAction, PickerLayout};
use settings::{SettingsAction, SettingsDialog, SettingsLayout};
use text_view::TextGeometry;

use super::Rect;
use crate::cartridge::source::{ScriptListing, SourcePath};
use crate::cartridge_fs::FsError;
use crate::system::code_cursor::{Caret, line_at, word_at};
use crate::system::code_document::{CodeDocument, DocumentId};
use crate::system::code_selection::{Edge, Selection};
use crate::system::code_session::{CodeSession, DocumentView};
use crate::system::code_text::{TextMetrics, spot};
use crate::system::editor_settings::EditorSettings;
use crate::system::shell::BLINK_TICKS;

/// Rows the text moves per wheel notch.
const WHEEL_LINES: isize = 3;
/// Columns the text moves per sideways wheel notch.
const WHEEL_COLUMNS: isize = 6;
/// Ticks within which another press near the last one in the text counts
/// toward a double or triple click, as in the sprite editor.
const MULTI_CLICK_TICKS: u32 = TICK_RATE * 2 / 5;
/// Pixels a press may land from the previous one and still continue its
/// multi-click.
const MULTI_CLICK_SLOP: u16 = 2;
/// Ticks between the scroll steps of a selection dragged past the text.
const AUTOSCROLL_TICKS: u32 = 4;

/// The status bar hint while nothing covers the view.
const VIEW_HINT: &str = "Ctrl+P: open  Ctrl+W: close  Esc: shell";
/// The status bar hint while OPEN FILE is open.
const PICKER_HINT: &str = "Type: filter  Enter: open  Esc: close";
/// The status bar hint while the list of open tabs is open.
const MENU_HINT: &str = "Up/Down: pick  Enter: show  Esc: close";
/// The status bar hint while EDITOR SETTINGS is open.
const SETTINGS_HINT: &str = "Up/Down: pick  Enter: toggle  Esc: close";

/// File work the Code view asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeRequest {
    /// List the cartridge's scripts for OPEN FILE; see
    /// [`CodeEditor::open_picker`].
    ListScripts,
    /// Open this script in a tab, or show its tab if it has one.
    Open(SourcePath),
    /// Keep these settings for the next launch.
    SaveSettings(EditorSettings),
}

/// The list of every open tab, dropped from the strip's right end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TabMenu {
    /// Position in tab order of the highlighted tab.
    focus: usize,
}

/// What a run of presses in the text selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Character,
    Word,
    Line,
}

impl Unit {
    /// The unit holding position `byte` of `document`.
    fn at(self, document: &CodeDocument, byte: usize) -> Range<usize> {
        match self {
            Self::Character => byte..byte,
            Self::Word => word_at(document, byte),
            Self::Line => line_at(document, byte),
        }
    }
}

/// A press in the text being dragged, selecting from where it began.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Gesture {
    /// The document the press landed in.
    id: DocumentId,
    unit: Unit,
    /// What stays selected wherever the drag goes: the unit pressed on, or
    /// the anchor a Shift press extended from.
    origin: Range<usize>,
    /// The unit under the press itself, to tell when the drag leaves it.
    pressed: Range<usize>,
}

/// The last press in the text, to tell double and triple clicks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Click {
    at: Point,
    /// 1 for a single click, 2 for a double and 3 for a triple.
    count: u8,
    /// Ticks left for the next press to continue the run.
    ticks_left: u32,
}

/// The Code view's own UI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeEditor {
    /// Also holds the font the text is drawn with, which decides the
    /// columns each character takes.
    layout: CodeLayout,
    settings: EditorSettings,
    /// While open, takes every input.
    settings_dialog: Option<SettingsDialog>,
    /// While open, takes every input.
    picker: Option<FilePicker>,
    /// While open, takes every input.
    menu: Option<TabMenu>,
    /// Position in tab order of the tab being dragged along the strip.
    drag: Option<usize>,
    /// Where the pointer last was, which decides what the wheel scrolls.
    pointer: Option<Point>,
    /// Why the cartridge's entry script could not be opened.
    entry_error: Option<String>,
    /// Ticks since the caret blink last restarted in its visible phase.
    blink: u32,
    /// The selection being dragged out in the text, while its press is held.
    selecting: Option<Gesture>,
    /// The last press in the text, while it can still start a multi-click.
    last_click: Option<Click>,
    /// Ticks the pointer has spent past the text while selecting.
    autoscroll: u32,
}

impl CodeEditor {
    pub fn new(metrics: Metrics, font: BitmapFont, settings: EditorSettings) -> Self {
        Self {
            layout: CodeLayout::new(metrics, font),
            settings,
            settings_dialog: None,
            picker: None,
            menu: None,
            drag: None,
            pointer: None,
            entry_error: None,
            blink: 0,
            selecting: None,
            last_click: None,
            autoscroll: 0,
        }
    }

    /// Advances the caret blink and the multi-click window by one tick, and
    /// scrolls `session`'s active document while a selection is dragged
    /// past its text.
    pub fn tick(&mut self, session: Option<&mut CodeSession>) {
        self.blink = self.blink.wrapping_add(1);
        self.last_click = self.last_click.and_then(|click| {
            let ticks_left = click.ticks_left.checked_sub(1).filter(|&left| left > 0)?;
            Some(Click {
                ticks_left,
                ..click
            })
        });
        if let Some(session) = session {
            self.autoscroll(session);
        }
    }

    /// Whether the caret is in the visible phase of its blink.
    fn caret_lit(&self) -> bool {
        (self.blink / BLINK_TICKS).is_multiple_of(2)
    }

    /// Whether EDITOR SETTINGS, OPEN FILE or the list of open tabs is open
    /// and taking all input.
    pub fn has_overlay(&self) -> bool {
        self.settings_dialog.is_some() || self.picker.is_some() || self.menu.is_some()
    }

    /// The status bar hint for what is on screen.
    pub fn hint(&self) -> &'static str {
        if self.settings_dialog.is_some() {
            SETTINGS_HINT
        } else if self.picker.is_some() {
            PICKER_HINT
        } else if self.menu.is_some() {
            MENU_HINT
        } else {
            VIEW_HINT
        }
    }

    #[cfg(test)]
    pub fn picker(&self) -> Option<&FilePicker> {
        self.picker.as_ref()
    }

    #[cfg(test)]
    pub fn layout(&self) -> &CodeLayout {
        &self.layout
    }

    #[cfg(test)]
    pub fn settings(&self) -> EditorSettings {
        self.settings
    }

    /// Shows code with `settings` from now on. Documents already open are
    /// measured again on the next input.
    pub fn set_settings(&mut self, settings: EditorSettings) {
        self.settings = settings;
    }

    /// Where the text view sits for `document` with the current settings.
    fn geometry(&self, document: &CodeDocument) -> TextGeometry {
        TextGeometry::new(&self.layout, document.line_count(), &self.settings)
    }

    /// Whether a tab or a selection is being dragged, which keeps its
    /// pointer wherever the pointer goes.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some() || self.selecting.is_some()
    }

    /// The pointer for `at`, with `session` shown: a closed hand while a
    /// tab is dragged, an I-beam while text is selected by dragging and
    /// over the text, and a pointing hand over what a click acts on.
    pub fn cursor(&self, at: Point, session: Option<&CodeSession>) -> SystemCursor {
        let hand = |clickable: bool| {
            if clickable {
                SystemCursor::Hand
            } else {
                SystemCursor::Arrow
            }
        };
        if self.drag.is_some() {
            return SystemCursor::Grab;
        }
        if self.selecting.is_some() {
            return SystemCursor::Text;
        }
        if self.settings_dialog.is_some() {
            let layout = settings_layout(&self.layout);
            let arrows = layout.minus.contains(at) || layout.plus.contains(at);
            return hand(arrows || layout.row_at(at).is_some());
        }
        if let Some(picker) = &self.picker {
            return hand(picker.opens_at(at, &picker_layout(&self.layout, picker)));
        }
        let Some(session) = session else {
            return hand(self.layout.settings_button().contains(at));
        };
        if let Some(menu) = self.menu {
            let (_, rows) = self.menu_rows(session.tabs().len(), menu.focus);
            return hand(rows.iter().any(|(_, rect)| rect.contains(at)));
        }
        let labels = labels(&self.layout.font, session);
        let on_strip = self.layout.strip_hit(&labels, session.tabs().scroll(), at);
        if on_strip != StripHit::None || self.layout.settings_button().contains(at) {
            return SystemCursor::Hand;
        }
        if session.active_document().is_some() && self.layout.text.contains(at) {
            return SystemCursor::Text;
        }
        SystemCursor::Arrow
    }

    /// The rows of the EDITOR SETTINGS window, top to bottom.
    #[cfg(test)]
    pub fn settings_rows(&self) -> Vec<Rect> {
        settings_layout(&self.layout).rows.to_vec()
    }

    /// Each document tab the strip shows: its position in tab order, its
    /// rectangle and its close glyph's.
    #[cfg(test)]
    pub fn tab_rects(&self, session: &CodeSession) -> Vec<(usize, Rect, Rect)> {
        self.layout
            .strip_tabs(&labels(&self.layout.font, session), session.tabs().scroll())
            .into_iter()
            .map(|tab| (tab.index, tab.rect, tab.close))
            .collect()
    }

    /// Drops everything shown of the previous cartridge. EDITOR SETTINGS
    /// belongs to no cartridge and stays open.
    pub fn forget(&mut self) {
        self.picker = None;
        self.menu = None;
        self.end_drag();
        self.last_click = None;
        self.entry_error = None;
    }

    /// Ends a tab drag or a selection drag in progress.
    pub fn end_drag(&mut self) {
        self.drag = None;
        self.selecting = None;
    }

    /// Opens OPEN FILE on `listing`.
    pub fn open_picker(&mut self, listing: Result<ScriptListing, FsError>) {
        self.menu = None;
        self.picker = Some(FilePicker::new(listing));
    }

    /// Records why the entry script could not be opened, or that it was.
    pub fn set_entry_error(&mut self, error: Option<String>) {
        self.entry_error = error;
    }

    /// Scrolls the strip so the active tab shows in full.
    pub fn reveal_active(&self, session: &mut CodeSession) {
        let Some(active) = session
            .tabs()
            .active()
            .and_then(|id| session.tabs().index_of(id))
        else {
            return;
        };
        let labels = labels(&self.layout.font, session);
        let scroll = self.layout.reveal(&labels, session.tabs().scroll(), active);
        session.tabs_mut().set_scroll(scroll);
    }

    /// Measures each of `session`'s documents whose measurements don't
    /// match how the view now shows it, moving its view back in range and
    /// its selection to valid positions without a preferred column, since
    /// columns may now mean something else. Documents already measured
    /// for the current settings are left alone.
    pub fn refresh_metrics(&self, session: &mut CodeSession) {
        let ids: Vec<DocumentId> = session.tabs().ids().collect();
        for id in ids {
            let Some(document) = session.document(id) else {
                continue;
            };
            let geometry = self.geometry(document);
            let key = geometry.metrics_key();
            if session.tabs().metrics(id).map(TextMetrics::key) == Some(key) {
                continue;
            }
            let metrics = TextMetrics::measure(document, key, &self.layout.font);
            let selection = session
                .tabs()
                .selection(id)
                .unwrap_or_default()
                .reset(document);
            if let Some(view) = session.tabs_mut().view_mut(id) {
                *view = geometry.clamp(*view, &metrics);
            }
            session.tabs_mut().set_selection(id, selection);
            session.tabs_mut().set_metrics(id, metrics);
        }
    }

    /// Applies `event` to `session` and returns the work it calls for, if
    /// any. Whenever the active tab changes, the strip scrolls to show it.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        session: Option<&mut CodeSession>,
    ) -> Option<CodeRequest> {
        if self.settings_dialog.is_some() {
            let request = self.settings_input(event);
            if let Some(session) = session {
                self.refresh_metrics(session);
            }
            return request;
        }
        if let InputEvent::PointerDown {
            button: PointerButton::Primary,
            at,
            ..
        } = event
            && self.picker.is_none()
            && self.menu.is_none()
            && self.layout.settings_button().contains(at)
        {
            self.settings_dialog = Some(SettingsDialog::new(self.settings));
            self.drag = None;
            return None;
        }
        let session = session?;
        self.refresh_metrics(session);
        let before = (session.tabs().active(), session.tabs().len());
        let request = self.input(event, session);
        if (session.tabs().active(), session.tabs().len()) != before {
            self.reveal_active(session);
        }
        self.refresh_metrics(session);
        request
    }

    /// Applies `event` to EDITOR SETTINGS, asking to keep the settings
    /// once it closes having changed them.
    fn settings_input(&mut self, event: InputEvent) -> Option<CodeRequest> {
        let layout = settings_layout(&self.layout);
        let dialog = self.settings_dialog.as_mut()?;
        match dialog.handle_input(event, &mut self.settings, &layout) {
            SettingsAction::None => None,
            SettingsAction::Close => {
                let opened_with = dialog.opened_with();
                self.settings_dialog = None;
                (self.settings != opened_with).then_some(CodeRequest::SaveSettings(self.settings))
            }
        }
    }

    fn input(&mut self, event: InputEvent, session: &mut CodeSession) -> Option<CodeRequest> {
        if let Some(picker) = &mut self.picker {
            let layout = picker_layout(&self.layout, picker);
            return match picker.handle_input(event, &layout)? {
                PickerAction::Close => {
                    self.picker = None;
                    None
                }
                PickerAction::Open(path) => {
                    self.picker = None;
                    Some(CodeRequest::Open(path))
                }
            };
        }
        if let Some(menu) = self.menu {
            self.menu_input(event, menu, session);
            return None;
        }
        let labels = labels(&self.layout.font, session);
        let scroll = session.tabs().scroll();
        let ids: Vec<_> = session.tabs().ids().collect();
        match event {
            InputEvent::OpenFile => return Some(CodeRequest::ListScripts),
            InputEvent::CloseDocument => {
                if let Some(active) = session.tabs().active() {
                    session.close(active);
                }
            }
            InputEvent::NextDocument => session.tabs_mut().cycle(1),
            InputEvent::PreviousDocument => session.tabs_mut().cycle(-1),
            InputEvent::PointerMove(at) => {
                self.pointer = at;
                if let Some(at) = at
                    && self.selecting.is_some()
                {
                    self.drag_selection(session, at);
                }
                if let (Some(dragged), Some(at)) = (self.drag, at)
                    && let Some(to) = self.layout.tab_column(&labels, scroll, i32::from(at.x))
                    && to != dragged
                {
                    session.tabs_mut().move_tab(dragged, to);
                    self.drag = Some(to);
                }
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                select,
            } => {
                self.pointer = Some(at);
                self.end_drag();
                match self.layout.strip_hit(&labels, scroll, at) {
                    StripHit::Tab(index) => {
                        session.tabs_mut().activate(ids[index]);
                        self.drag = Some(index);
                    }
                    StripHit::Close(index) => {
                        session.close(ids[index]);
                    }
                    StripHit::More => {
                        let active = session.tabs().active();
                        if let Some(focus) = active.and_then(|id| session.tabs().index_of(id)) {
                            self.menu = Some(TabMenu { focus });
                        }
                    }
                    StripHit::None if self.layout.text.contains(at) => {
                        self.press_text(session, at, select);
                    }
                    StripHit::None => {}
                }
            }
            InputEvent::PointerDown {
                button: PointerButton::Middle,
                at,
                ..
            } => {
                if let StripHit::Tab(index) | StripHit::Close(index) =
                    self.layout.strip_hit(&labels, scroll, at)
                {
                    session.close(ids[index]);
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } => self.end_drag(),
            InputEvent::PointerCancel => {
                self.end_drag();
                self.last_click = None;
            }
            InputEvent::Wheel { delta } => self.wheel(delta, &labels, session),
            InputEvent::Left { select } => {
                self.move_caret(session, 0, select, Some(Edge::Start), |doc, caret, _| {
                    caret.left(doc)
                });
            }
            InputEvent::Right { select } => {
                self.move_caret(session, 0, select, Some(Edge::End), |doc, caret, _| {
                    caret.right(doc)
                });
            }
            InputEvent::WordLeft { select } => {
                self.move_caret(session, 0, select, None, |doc, caret, _| {
                    caret.word_left(doc)
                });
            }
            InputEvent::WordRight { select } => {
                self.move_caret(session, 0, select, None, |doc, caret, _| {
                    caret.word_right(doc)
                });
            }
            InputEvent::Home { select } => {
                self.move_caret(session, 0, select, None, |doc, caret, _| caret.home(doc));
            }
            InputEvent::End { select } => {
                self.move_caret(session, 0, select, None, |doc, caret, _| caret.end(doc));
            }
            InputEvent::DocumentStart { select } => {
                self.move_caret(session, 0, select, None, |doc, _, _| {
                    Caret::document_start(doc)
                });
            }
            InputEvent::DocumentEnd { select } => {
                self.move_caret(session, 0, select, None, |doc, _, _| {
                    Caret::document_end(doc)
                });
            }
            InputEvent::Up { select } => {
                let font = self.layout.font.clone();
                self.move_caret(session, 0, select, None, |doc, caret, metrics| {
                    caret.vertical(doc, -1, metrics, &font)
                });
            }
            InputEvent::Down { select } => {
                let font = self.layout.font.clone();
                self.move_caret(session, 0, select, None, |doc, caret, metrics| {
                    caret.vertical(doc, 1, metrics, &font)
                });
            }
            InputEvent::SelectAll => self.select_all(session),
            InputEvent::PageUp { select } => self.page(session, -1, select),
            InputEvent::PageDown { select } => self.page(session, 1, select),
            InputEvent::WheelHorizontal { delta } if !self.pointer_over_strip() => {
                // Positive turns move toward the left.
                let step = -isize::from(delta) * WHEEL_COLUMNS;
                self.scroll_active(session, |geometry, view, metrics| {
                    geometry.scroll_columns(view, step, metrics)
                });
            }
            _ => {}
        }
        None
    }

    /// Scrolls the strip when the pointer is over it, and the active
    /// document otherwise.
    fn wheel(&mut self, delta: i8, labels: &[String], session: &mut CodeSession) {
        if self.pointer_over_strip() {
            let Some(last) = labels.len().checked_sub(1) else {
                return;
            };
            let furthest = self.layout.reveal(labels, 0, last);
            let scroll = session
                .tabs()
                .scroll()
                .saturating_add_signed(-isize::from(delta))
                .min(furthest);
            session.tabs_mut().set_scroll(scroll);
            return;
        }
        // Positive turns move away from the user, toward the top.
        let step = -isize::from(delta) * WHEEL_LINES;
        self.scroll_active(session, |geometry, view, metrics| {
            geometry.scroll_rows(view, step, metrics)
        });
    }

    /// Moves the active document's caret and its view one screenful up,
    /// or down when `direction` is positive, so the caret keeps its place
    /// on screen where the document allows. `select` extends the selection.
    fn page(&mut self, session: &mut CodeSession, direction: isize, select: bool) {
        let Some(document) = session.active_document() else {
            return;
        };
        let rows = self.geometry(document).rows as isize * direction;
        let font = self.layout.font.clone();
        self.move_caret(session, rows, select, None, |doc, caret, metrics| {
            caret.vertical(doc, rows, metrics, &font)
        });
    }

    /// Moves the active document's caret with `motion`, after scrolling
    /// its view `scroll` rows, then scrolls as little as it takes to show
    /// the caret. With `select` the selection follows the caret from its
    /// anchor; without it the selection is dropped, or, for moves that
    /// have an `edge`, collapsed to that end. Does nothing until its
    /// measurements describe the view.
    fn move_caret(
        &mut self,
        session: &mut CodeSession,
        scroll: isize,
        select: bool,
        edge: Option<Edge>,
        motion: impl FnOnce(&CodeDocument, Caret, &TextMetrics) -> Caret,
    ) {
        let Some(document) = session.active_document() else {
            return;
        };
        let id = document.id();
        let geometry = self.geometry(document);
        let tabs = session.tabs();
        let (Some(&view), Some(selection), Some(metrics)) =
            (tabs.view(id), tabs.selection(id), tabs.metrics(id))
        else {
            return;
        };
        if metrics.key() != geometry.metrics_key() {
            return;
        }
        let selection = match edge {
            Some(edge) => selection.stepped(document, select, edge, |caret| {
                motion(document, caret, metrics)
            }),
            None => selection.moved(motion(document, selection.caret(), metrics), select),
        };
        let view = geometry.scroll_rows(view, scroll, metrics);
        let here = spot(
            document,
            selection.caret().byte(),
            metrics.key(),
            &self.layout.font,
        );
        let view = geometry.reveal(view, &here, metrics);
        self.show_selection(session, id, selection, Some(view));
    }

    /// Selects the whole active document, leaving its view where it is.
    fn select_all(&mut self, session: &mut CodeSession) {
        let Some(document) = session.active_document() else {
            return;
        };
        let (id, selection) = (document.id(), Selection::all(document));
        self.show_selection(session, id, selection, None);
    }

    /// The active document's id and its position nearest `at`, once its
    /// measurements describe the view.
    fn hit_text(&self, session: &CodeSession, at: Point) -> Option<(DocumentId, usize)> {
        let document = session.active_document()?;
        let id = document.id();
        let geometry = self.geometry(document);
        let tabs = session.tabs();
        let (view, metrics) = (tabs.view(id)?, tabs.metrics(id)?);
        let byte = geometry.hit(*view, at, document, metrics, &self.layout.font)?;
        Some((id, Caret::at(document, byte).byte()))
    }

    /// Handles a primary press at `at` in the text: a single click puts the
    /// caret there, a double click selects the word there and a triple
    /// click its line, and `select` extends the selection to it instead.
    /// The press then drags the selection until it is released.
    fn press_text(&mut self, session: &mut CodeSession, at: Point, select: bool) {
        let Some((id, byte)) = self.hit_text(session, at) else {
            return;
        };
        let Some(document) = session.document(id) else {
            return;
        };
        let count = match self.last_click {
            Some(click) if !select && near(click.at, at) => click.count % 3 + 1,
            _ => 1,
        };
        self.last_click = Some(Click {
            at,
            count,
            ticks_left: MULTI_CLICK_TICKS,
        });
        let unit = match count {
            1 => Unit::Character,
            2 => Unit::Word,
            _ => Unit::Line,
        };
        let pressed = unit.at(document, byte);
        let (origin, selection) = if select {
            let current = session.tabs().selection(id).unwrap_or_default();
            let anchor = current.anchor();
            let selection = current.moved(Caret::at(document, byte), true);
            (anchor..anchor, selection)
        } else {
            let selection =
                Selection::between(document, pressed.start, Caret::at(document, pressed.end));
            (pressed.clone(), selection)
        };
        // A click shows where it landed; a word or line that more clicks
        // select may end past the view, which then stays put.
        let view = if unit == Unit::Character {
            self.revealing(session, id, selection.caret())
        } else {
            None
        };
        self.show_selection(session, id, selection, view);
        self.selecting = Some(Gesture {
            id,
            unit,
            origin,
            pressed,
        });
    }

    /// Extends the selection being dragged to the unit under `at`. A drag
    /// that leaves the unit it pressed on ends any multi-click run, so the
    /// next press is a single click.
    fn drag_selection(&mut self, session: &mut CodeSession, at: Point) {
        let Some(gesture) = &self.selecting else {
            return;
        };
        let Some((id, byte)) = self
            .hit_text(session, at)
            .filter(|&(id, _)| id == gesture.id)
        else {
            return;
        };
        let Some(document) = session.document(id) else {
            return;
        };
        let target = gesture.unit.at(document, byte);
        let origin = &gesture.origin;
        let selection = if target.start < origin.start {
            Selection::between(document, origin.end, Caret::at(document, target.start))
        } else {
            Selection::between(document, origin.start, Caret::at(document, target.end))
        };
        if target != gesture.pressed {
            self.last_click = None;
        }
        self.show_selection(session, id, selection, None);
    }

    /// Scrolls toward the pointer while a selection is dragged past the
    /// text's edges, by more the further past it is, and extends the
    /// selection to what scrolls under the pointer. Hosts only report the
    /// pointer when it moves, so a pointer held still scrolls from here.
    fn autoscroll(&mut self, session: &mut CodeSession) {
        let (Some(gesture), Some(at)) = (&self.selecting, self.pointer) else {
            self.autoscroll = 0;
            return;
        };
        let Some(document) = session
            .active_document()
            .filter(|document| document.id() == gesture.id)
        else {
            self.autoscroll = 0;
            return;
        };
        let geometry = self.geometry(document);
        let viewport = geometry.viewport;
        let rows = past(
            i32::from(at.y),
            viewport.y,
            viewport.bottom(),
            geometry.cell_h,
        );
        let columns = if geometry.wrap {
            0
        } else {
            past(
                i32::from(at.x),
                viewport.x,
                viewport.right(),
                geometry.cell_w,
            )
        };
        if rows == 0 && columns == 0 {
            self.autoscroll = 0;
            return;
        }
        let due = self.autoscroll.is_multiple_of(AUTOSCROLL_TICKS);
        self.autoscroll = self.autoscroll.wrapping_add(1);
        if !due {
            return;
        }
        self.scroll_active(session, |geometry, view, metrics| {
            let view = geometry.scroll_rows(view, rows, metrics);
            geometry.scroll_columns(view, columns, metrics)
        });
        self.drag_selection(session, at);
    }

    /// `id`'s view scrolled as little as it takes to show `caret`, once
    /// its measurements are taken.
    fn revealing(
        &self,
        session: &CodeSession,
        id: DocumentId,
        caret: Caret,
    ) -> Option<DocumentView> {
        let document = session.document(id)?;
        let geometry = self.geometry(document);
        let tabs = session.tabs();
        let (view, metrics) = (tabs.view(id)?, tabs.metrics(id)?);
        let here = spot(document, caret.byte(), metrics.key(), &self.layout.font);
        Some(geometry.reveal(*view, &here, metrics))
    }

    /// Records `id`'s selection, and its view if given, restarting the
    /// blink so the caret shows while it moves.
    fn show_selection(
        &mut self,
        session: &mut CodeSession,
        id: DocumentId,
        selection: Selection,
        view: Option<DocumentView>,
    ) {
        session.tabs_mut().set_selection(id, selection);
        if let (Some(view), Some(stored)) = (view, session.tabs_mut().view_mut(id)) {
            *stored = view;
        }
        self.blink = 0;
    }

    fn pointer_over_strip(&self) -> bool {
        self.pointer
            .is_some_and(|at| self.layout.strip.contains(at))
    }

    /// Moves the active document's view with `scroll`, once its
    /// measurements describe the view.
    fn scroll_active(
        &self,
        session: &mut CodeSession,
        scroll: impl FnOnce(&TextGeometry, DocumentView, &TextMetrics) -> DocumentView,
    ) {
        let Some(document) = session.active_document() else {
            return;
        };
        let id = document.id();
        let geometry = self.geometry(document);
        let tabs = session.tabs();
        let (Some(&view), Some(metrics)) = (tabs.view(id), tabs.metrics(id)) else {
            return;
        };
        if metrics.key() != geometry.metrics_key() {
            return;
        }
        let moved = scroll(&geometry, view, metrics);
        if let Some(view) = session.tabs_mut().view_mut(id) {
            *view = moved;
        }
    }

    fn menu_input(&mut self, event: InputEvent, menu: TabMenu, session: &mut CodeSession) {
        let ids: Vec<_> = session.tabs().ids().collect();
        let Some(last) = ids.len().checked_sub(1) else {
            self.menu = None;
            return;
        };
        let mut focus = menu.focus.min(last);
        match event {
            InputEvent::Break => {
                self.menu = None;
                return;
            }
            InputEvent::Up { .. } => focus = focus.saturating_sub(1),
            InputEvent::Down { .. } => focus = (focus + 1).min(last),
            InputEvent::Wheel { delta } => {
                focus = focus.saturating_add_signed(-isize::from(delta)).min(last);
            }
            InputEvent::Enter => {
                session.tabs_mut().activate(ids[focus]);
                self.menu = None;
                return;
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                let (_, rows) = self.menu_rows(ids.len(), focus);
                if let Some(&(index, _)) = rows.iter().find(|(_, rect)| rect.contains(at)) {
                    session.tabs_mut().activate(ids[index]);
                }
                self.menu = None;
                return;
            }
            _ => {}
        }
        self.menu = Some(TabMenu { focus });
    }

    /// The list of open tabs' window, and the tab-order index and rectangle
    /// of each row it shows, keeping `focus` in view.
    fn menu_rows(&self, count: usize, focus: usize) -> (Rect, Vec<(usize, Rect)>) {
        let layout = &self.layout;
        let cell_h = layout.metrics.cell_h;
        let more = layout.more_button();
        let fit =
            usize::try_from((layout.status.y - more.bottom() - 4) / cell_h.max(1)).unwrap_or(1);
        let shown = count.min(fit.max(1));
        let first = (focus + 1).saturating_sub(shown);
        let width = (layout::MAX_LABEL_CELLS as i32 + 2) * layout.metrics.cell_w + 6;
        let window = Rect {
            x: more.right() - width,
            y: more.bottom() + 1,
            width,
            height: shown as i32 * cell_h + 4,
        };
        let rows = (0..shown)
            .map(|row| {
                let rect = Rect {
                    x: window.x + 1,
                    y: window.y + 2 + row as i32 * cell_h,
                    width: window.width - 2,
                    height: cell_h,
                };
                (first + row, rect)
            })
            .collect();
        (window, rows)
    }
}

/// Whether a press at `b` lands close enough to one at `a` to continue its
/// multi-click.
fn near(a: Point, b: Point) -> bool {
    a.x.abs_diff(b.x) <= MULTI_CLICK_SLOP && a.y.abs_diff(b.y) <= MULTI_CLICK_SLOP
}

/// Steps a pointer at `at` lies past the span from `start` to `end` in
/// `cell`-sized steps: negative before it, positive after it and 0 inside.
fn past(at: i32, start: i32, end: i32, cell: i32) -> isize {
    let steps = if at < start {
        -(1 + (start - at) / cell)
    } else if at >= end {
        1 + (at - end) / cell
    } else {
        0
    };
    steps as isize
}

/// Where windows over the Code view are centred: everything between the
/// workspace's tab bar and the status bar.
fn overlay_area(layout: &CodeLayout) -> Rect {
    Rect {
        x: 0,
        y: layout.strip.y,
        width: layout.strip.width,
        height: layout.status.y - layout.strip.y,
    }
}

/// The OPEN FILE window for `picker` over the Code view.
fn picker_layout(layout: &CodeLayout, picker: &FilePicker) -> PickerLayout {
    PickerLayout::new(
        overlay_area(layout),
        layout.metrics.cell_h,
        picker.gap_lines().len(),
    )
}

/// The EDITOR SETTINGS window over the Code view.
fn settings_layout(layout: &CodeLayout) -> SettingsLayout {
    SettingsDialog::layout(
        overlay_area(layout),
        layout.metrics.cell_w,
        layout.metrics.cell_h,
    )
}

/// The label of each of `session`'s tabs, in tab order.
fn labels(font: &BitmapFont, session: &CodeSession) -> Vec<String> {
    let paths: Vec<&SourcePath> = session
        .tabs()
        .ids()
        .filter_map(|id| session.document(id))
        .map(|document| document.path())
        .collect();
    tab_labels(font, &paths)
}

#[cfg(test)]
mod tests;
