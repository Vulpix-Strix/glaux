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

use glaux_input::{InputEvent, Point, PointerButton};

pub use layout::Metrics;
use layout::{CodeLayout, StripHit, tab_labels};
use picker::{FilePicker, PickerAction, PickerLayout};
use settings::{SettingsAction, SettingsDialog, SettingsLayout};
use text_view::TextGeometry;

use super::Rect;
use crate::cartridge::source::{ScriptListing, SourcePath};
use crate::cartridge_fs::FsError;
use crate::system::code_document::{CodeDocument, DocumentId};
use crate::system::code_session::{CodeSession, DocumentView};
use crate::system::code_text::TextMetrics;
use crate::system::editor_settings::EditorSettings;

/// Rows the text moves per wheel notch.
const WHEEL_LINES: isize = 3;
/// Columns the text moves per sideways wheel notch.
const WHEEL_COLUMNS: isize = 6;

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

/// The Code view's own UI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeEditor {
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
}

impl CodeEditor {
    pub fn new(metrics: Metrics, settings: EditorSettings) -> Self {
        Self {
            layout: CodeLayout::new(metrics),
            settings,
            settings_dialog: None,
            picker: None,
            menu: None,
            drag: None,
            pointer: None,
            entry_error: None,
        }
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
    pub fn layout(&self) -> CodeLayout {
        self.layout
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

    /// Each document tab the strip shows: its position in tab order, its
    /// rectangle and its close glyph's.
    #[cfg(test)]
    pub fn tab_rects(&self, session: &CodeSession) -> Vec<(usize, Rect, Rect)> {
        self.layout
            .strip_tabs(&labels(session), session.tabs().scroll())
            .into_iter()
            .map(|tab| (tab.index, tab.rect, tab.close))
            .collect()
    }

    /// Drops everything shown of the previous cartridge. EDITOR SETTINGS
    /// belongs to no cartridge and stays open.
    pub fn forget(&mut self) {
        self.picker = None;
        self.menu = None;
        self.drag = None;
        self.entry_error = None;
    }

    /// Ends a tab drag in progress.
    pub fn end_drag(&mut self) {
        self.drag = None;
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
        let labels = labels(session);
        let scroll = self.layout.reveal(&labels, session.tabs().scroll(), active);
        session.tabs_mut().set_scroll(scroll);
    }

    /// Measures each of `session`'s documents whose measurements don't
    /// match how the view now shows it, moving its view back in range.
    /// Documents already measured for the current settings are left alone.
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
            let metrics = TextMetrics::measure(document, key);
            if let Some(view) = session.tabs_mut().view_mut(id) {
                *view = geometry.clamp(*view, &metrics);
            }
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
        let labels = labels(session);
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
            } => {
                self.pointer = Some(at);
                self.drag = None;
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
                    StripHit::None => {}
                }
            }
            InputEvent::PointerDown {
                button: PointerButton::Middle,
                at,
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
            }
            | InputEvent::PointerCancel => self.drag = None,
            InputEvent::Wheel { delta } => self.wheel(delta, &labels, session),
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
            InputEvent::Up => focus = focus.saturating_sub(1),
            InputEvent::Down => focus = (focus + 1).min(last),
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
fn labels(session: &CodeSession) -> Vec<String> {
    let paths: Vec<&SourcePath> = session
        .tabs()
        .ids()
        .filter_map(|id| session.document(id))
        .map(|document| document.path())
        .collect();
    tab_labels(&paths)
}

#[cfg(test)]
mod tests;
