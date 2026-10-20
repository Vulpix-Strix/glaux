//! The Code view shows tabs over the cartridge's open scripts, an editable
//! view of the active one, OPEN FILE and NEW FILE to open more, a find bar,
//! GO TO LINE, and context menus.
//!
//! The documents and tabs belong to the cartridge and are lent in as a
//! [`CodeSession`]. The editor keeps only what is on screen around them.

mod actions;
mod completion;
mod docs_lookup;
mod edit;
mod find;
mod go_to_line;
mod layout;
mod new_file;
mod picker;
mod popup;
mod prompt;
mod rename;
mod render;
mod searching;
mod settings;
mod signature;
pub(super) mod text_view;

use std::ops::Range;

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::machine::TICK_RATE;
use glaux_machine::system_rom::SystemCursor;

use actions::{CodeAction, TabTarget, file_menu, text_menu};
use completion::{CompletionPopup, completion_hint};
use find::{FindBar, FindLayout};
use go_to_line::GoToLine;
pub use layout::Metrics;
use layout::{CodeLayout, StripHit, tab_labels};
use new_file::{CODE_DIR, NewFile, NewFileAction};
use picker::{FilePicker, PickerAction, PickerLayout};
use prompt::{Prompt, PromptLayout};
use rename::{RenameAction, RenameScript};
use settings::{SettingsAction, SettingsDialog, SettingsLayout};
use signature::{Dismissed, SignaturePopup, signature_hint};
use text_view::TextGeometry;

use super::Rect;
use super::context_menu::{ContextMenu, MenuLayout, MenuOutcome, menu_hint as context_menu_hint};
use super::docs::DocTarget;
use crate::cartridge::Language;
use crate::cartridge::source::{ScriptListing, SourcePath};
use crate::cartridge_fs::FsError;
use crate::system::code_cursor::{Caret, line_at, word_at};
use crate::system::code_document::{CodeDocument, DocumentId};
use crate::system::code_selection::{Edge, Selection};
use crate::system::code_session::{CodeSession, DocumentView};
use crate::system::code_text::{TextMetrics, spot};
use crate::system::editor_settings::EditorSettings;
use crate::system::shell::BLINK_TICKS;
use crate::ui_text;

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
/// Ticks without an edit after which the next one starts a new undo step,
/// even where it continues the last.
const UNDO_IDLE_TICKS: u32 = TICK_RATE;

/// The status bar hint while nothing covers the view.
fn view_hint() -> String {
    ui_text::get("code-ctrl-s-save-ctrl-f-find-ctrl-n-new-esc-shell")
}
/// The status bar hint while the find bar has the keys.
fn find_hint() -> String {
    ui_text::get("code-enter-next-up-previous-tab-field-esc-close")
}
/// The status bar hint while NEW FILE is open.
fn new_file_hint() -> String {
    ui_text::get("code-type-path-enter-create-esc-close")
}
/// The status bar hint while RENAME SCRIPT is open.
fn rename_hint() -> String {
    ui_text::get("code-type-path-enter-move-esc-close")
}
/// The status bar hint while GO TO LINE is open.
fn go_to_hint() -> String {
    ui_text::get("code-type-line-enter-go-esc-close")
}
/// The status bar hint while OPEN FILE is open.
fn picker_hint() -> String {
    ui_text::get("code-type-filter-enter-open-esc-close")
}
/// The status bar hint while the list of open tabs is open.
fn menu_hint() -> String {
    ui_text::get("code-up-down-pick-enter-show-esc-close")
}
/// The status bar hint while EDITOR SETTINGS is open.
fn settings_hint() -> String {
    ui_text::get("code-up-down-pick-enter-toggle-esc-close")
}

/// File work the Code view asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeRequest {
    /// List the cartridge's scripts for OPEN FILE. See
    /// [`CodeEditor::open_picker`].
    ListScripts,
    /// Open this script in a tab, or show its tab if it has one.
    Open(SourcePath),
    /// Keep these settings for the next launch.
    SaveSettings(EditorSettings),
    /// Write this document to its file.
    Save(DocumentId),
    /// Ask whether to save this document's unsaved edits before closing
    /// its tab.
    ConfirmClose(DocumentId),
    /// Put this text on the clipboard.
    Copy(String),
    /// Read the clipboard and hand its text to [`CodeEditor::paste`].
    Paste,
    /// Open NEW FILE for the cartridge's language. See
    /// [`CodeEditor::open_new_file`].
    NewFile,
    /// Create this empty script and open it. Answer with
    /// [`CodeEditor::script_created`].
    Create(SourcePath),
    /// Ask whether to delete this document's file.
    DeleteScript(DocumentId),
    /// Move this document's file to this path. Answer with
    /// [`CodeEditor::script_renamed`].
    Rename { id: DocumentId, to: SourcePath },
    /// Show this in the status bar for a moment.
    Notice(String),
    /// Show this page of the documentation.
    OpenDocs(DocTarget),
    /// Show the documentation with its search open.
    SearchDocs,
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
    /// While open, takes every input.
    context_menu: Option<ContextMenu<CodeAction>>,
    /// While open, takes every input.
    new_file: Option<NewFile>,
    /// While open, takes every input.
    rename: Option<RenameScript>,
    /// While open, takes every input.
    go_to: Option<GoToLine>,
    /// Docked over the text while open. It takes the keys only while one of
    /// its parts has them.
    find: Option<FindBar>,
    /// Listed under the word being typed while open. It takes only the
    /// keys that pick, accept or dismiss a suggestion.
    completion: Option<CompletionPopup>,
    /// Over the line of the call being typed while open. It takes only
    /// Escape.
    signature: Option<SignaturePopup>,
    /// The call whose popup Escape closed.
    signature_dismissed: Option<Dismissed>,
    /// Position in tab order of the tab being dragged along the strip.
    drag: Option<usize>,
    /// Where the pointer last was, which decides what the wheel scrolls.
    pointer: Option<Point>,
    /// Why the cartridge's entry script could not be opened.
    entry_error: Option<String>,
    /// The cartridge's entry script, which cannot be renamed or deleted.
    entry: Option<SourcePath>,
    /// Ticks since the caret blink last restarted in its visible phase.
    blink: u32,
    /// The selection being dragged out in the text, while its press is held.
    selecting: Option<Gesture>,
    /// The last press in the text, while it can still start a multi-click.
    last_click: Option<Click>,
    /// Ticks the pointer has spent past the text while selecting.
    autoscroll: u32,
    /// Ticks since the last edit, saturating.
    idle: u32,
}

impl CodeEditor {
    pub fn new(metrics: Metrics, font: BitmapFont, settings: EditorSettings) -> Self {
        Self {
            layout: CodeLayout::new(metrics, font),
            settings,
            settings_dialog: None,
            picker: None,
            menu: None,
            context_menu: None,
            new_file: None,
            rename: None,
            go_to: None,
            find: None,
            completion: None,
            signature: None,
            signature_dismissed: None,
            drag: None,
            pointer: None,
            entry_error: None,
            entry: None,
            blink: 0,
            selecting: None,
            last_click: None,
            autoscroll: 0,
            idle: 0,
        }
    }

    /// Advances the caret blink and the multi-click window by one tick, and
    /// scrolls `session`'s active document while a selection is dragged
    /// past its text.
    pub fn tick(&mut self, session: Option<&mut CodeSession>) {
        self.blink = self.blink.wrapping_add(1);
        self.idle = self.idle.saturating_add(1);
        self.last_click = self.last_click.and_then(|click| {
            let ticks_left = click.ticks_left.checked_sub(1).filter(|&left| left > 0)?;
            Some(Click {
                ticks_left,
                ..click
            })
        });
        if let Some(session) = session {
            self.autoscroll(session);
            self.refresh_find(session);
        }
    }

    /// Whether the caret is in the visible phase of its blink.
    fn caret_lit(&self) -> bool {
        (self.blink / BLINK_TICKS).is_multiple_of(2)
    }

    /// Whether a window or menu is open over the view and taking all
    /// input: EDITOR SETTINGS, OPEN FILE, NEW FILE, RENAME SCRIPT, GO TO
    /// LINE, the list of open tabs or a context menu.
    pub fn has_overlay(&self) -> bool {
        self.settings_dialog.is_some()
            || self.picker.is_some()
            || self.menu.is_some()
            || self.context_menu.is_some()
            || self.new_file.is_some()
            || self.rename.is_some()
            || self.go_to.is_some()
    }

    /// Whether Escape belongs to the view rather than leaving the
    /// workspace: an overlay is open, or the find bar, the list of
    /// suggestions or the signature, which it closes.
    pub fn claims_break(&self) -> bool {
        self.has_overlay()
            || self.find.is_some()
            || self.completion.is_some()
            || self.signature.is_some()
    }

    /// Whether a part of the find bar has the keys.
    fn find_focused(&self) -> bool {
        self.find.as_ref().is_some_and(|bar| bar.focus().is_some())
    }

    /// The status bar hint for what is on screen.
    pub fn hint(&self) -> String {
        if self.settings_dialog.is_some() {
            settings_hint()
        } else if self.picker.is_some() {
            picker_hint()
        } else if self.menu.is_some() {
            menu_hint()
        } else if self.context_menu.is_some() {
            context_menu_hint()
        } else if self.new_file.is_some() {
            new_file_hint()
        } else if self.rename.is_some() {
            rename_hint()
        } else if self.go_to.is_some() {
            go_to_hint()
        } else if self.completion.is_some() {
            completion_hint()
        } else if self.find_focused() {
            find_hint()
        } else if self.signature.is_some() {
            signature_hint()
        } else {
            view_hint()
        }
    }

    #[cfg(test)]
    pub fn find(&self) -> Option<&FindBar> {
        self.find.as_ref()
    }

    #[cfg(test)]
    pub fn completion(&self) -> Option<&CompletionPopup> {
        self.completion.as_ref()
    }

    #[cfg(test)]
    pub fn signature(&self) -> Option<&SignaturePopup> {
        self.signature.as_ref()
    }

    #[cfg(test)]
    pub fn context_menu(&self) -> Option<&ContextMenu<CodeAction>> {
        self.context_menu.as_ref()
    }

    /// The label and whether it can be chosen of each item of the open
    /// context menu, separators left out.
    #[cfg(test)]
    pub fn menu_items(&self) -> Vec<(&str, bool)> {
        use crate::system::workspace::context_menu::MenuEntry;
        self.context_menu
            .iter()
            .flat_map(ContextMenu::entries)
            .filter_map(|entry| match entry {
                MenuEntry::Item { label, enabled, .. } => Some((label.as_str(), *enabled)),
                MenuEntry::Separator => None,
            })
            .collect()
    }

    #[cfg(test)]
    pub fn new_file(&self) -> Option<&NewFile> {
        self.new_file.as_ref()
    }

    #[cfg(test)]
    pub fn rename(&self) -> Option<&RenameScript> {
        self.rename.as_ref()
    }

    #[cfg(test)]
    pub fn go_to(&self) -> Option<&GoToLine> {
        self.go_to.as_ref()
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
        TextGeometry::in_area(
            &self.layout,
            self.text_area(),
            document.line_count(),
            &self.settings,
        )
    }

    /// The find bar's parts, while it is open.
    fn find_layout(&self) -> Option<FindLayout> {
        let bar = self.find.as_ref()?;
        Some(FindLayout::new(
            self.layout.text,
            self.layout.metrics.cell_w,
            self.layout.metrics.cell_h,
            bar.is_replacing(),
        ))
    }

    /// The part of the text area the text takes: all of it, or what the
    /// find bar leaves below it.
    fn text_area(&self) -> Rect {
        let text = self.layout.text;
        match self.find_layout() {
            Some(find) => Rect {
                y: find.panel.bottom(),
                height: text.bottom() - find.panel.bottom(),
                ..text
            },
            None => text,
        }
    }

    /// Where a context menu goes: anywhere over the view.
    fn menu_layout(&self, menu: &ContextMenu<CodeAction>) -> MenuLayout {
        let metrics = self.layout.metrics;
        menu.layout(
            &self.layout.font,
            overlay_area(&self.layout),
            metrics.cell_w,
            metrics.cell_h,
        )
    }

    /// Where `prompt` goes: centred over the view.
    fn prompt_layout(&self, prompt: &Prompt) -> PromptLayout {
        let metrics = self.layout.metrics;
        prompt.layout(overlay_area(&self.layout), metrics.cell_w, metrics.cell_h)
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
            return hand(layout.arrow_at(at).is_some() || layout.row_at(at).is_some());
        }
        if let Some(picker) = &self.picker {
            return hand(picker.opens_at(at, &picker_layout(&self.layout, picker)));
        }
        if let Some(menu) = &self.context_menu {
            return hand(menu.chooses_at(at, &self.menu_layout(menu)));
        }
        let prompt = self
            .new_file
            .as_ref()
            .map(NewFile::prompt)
            .or(self.rename.as_ref().map(RenameScript::prompt))
            .or(self.go_to.as_ref().map(GoToLine::prompt));
        if let Some(prompt) = prompt {
            return if self.prompt_layout(prompt).field.contains(at) {
                SystemCursor::Text
            } else {
                SystemCursor::Arrow
            };
        }
        if let (Some(bar), Some(layout)) = (&self.find, self.find_layout())
            && layout.panel.contains(at)
        {
            return if bar.clickable_at(at, &layout) {
                SystemCursor::Hand
            } else if layout.query.contains(at)
                || (bar.is_replacing() && layout.replacement.contains(at))
            {
                SystemCursor::Text
            } else {
                SystemCursor::Arrow
            };
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
        if session.active_document().is_some() && self.text_area().contains(at) {
            return SystemCursor::Text;
        }
        if session.active_document().is_none() && self.empty_action_at(at).is_some() {
            return SystemCursor::Hand;
        }
        SystemCursor::Arrow
    }

    /// The links the empty view offers, and what each does.
    fn empty_actions(&self) -> [(Rect, String, CodeAction); 2] {
        let area = self.layout.text;
        let (cell_w, cell_h) = (self.layout.metrics.cell_w, self.layout.metrics.cell_h);
        let link = |row: i32, label: String, action| {
            let rect = Rect {
                x: area.x + cell_w,
                y: area.y + row * cell_h,
                width: self.layout.cells(&label) * cell_w,
                height: cell_h,
            };
            (rect, label, action)
        };
        [
            link(2, ui_text::get("code-new-file-ctrl-n"), CodeAction::NewFile),
            link(
                3,
                ui_text::get("code-open-file-ctrl-p"),
                CodeAction::OpenFile,
            ),
        ]
    }

    /// The empty view's link under `at`.
    fn empty_action_at(&self, at: Point) -> Option<CodeAction> {
        self.empty_actions()
            .into_iter()
            .find(|(rect, ..)| rect.contains(at))
            .map(|(_, _, action)| action)
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
        self.context_menu = None;
        self.new_file = None;
        self.rename = None;
        self.go_to = None;
        self.find = None;
        self.completion = None;
        self.signature = None;
        self.signature_dismissed = None;
        self.end_drag();
        self.last_click = None;
        self.entry_error = None;
        self.entry = None;
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

    /// Opens NEW FILE for a cartridge written in `language`.
    pub fn open_new_file(&mut self, language: Language) {
        self.new_file = Some(NewFile::new(language));
    }

    /// Reports how creating the script NEW FILE asked for went: it closes
    /// once the script is created and open, and shows why otherwise.
    pub fn script_created(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => self.new_file = None,
            Err(message) => {
                if let Some(dialog) = &mut self.new_file {
                    dialog.set_failure(message);
                }
            }
        }
    }

    /// Reports how moving the file RENAME SCRIPT asked for went: it closes
    /// once the file moved, and shows why otherwise.
    pub fn script_renamed(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => self.rename = None,
            Err(message) => {
                if let Some(dialog) = &mut self.rename {
                    dialog.set_failure(message);
                }
            }
        }
    }

    /// Inserts the clipboard's `text` where the keys are: a field of NEW
    /// FILE, RENAME SCRIPT, GO TO LINE or the find bar, or else the active
    /// document of `session`. `None`, an empty clipboard, inserts nothing.
    pub fn paste(&mut self, text: Option<String>, session: Option<&mut CodeSession>) {
        let Some(text) = text else {
            return;
        };
        if let Some(dialog) = &mut self.new_file {
            dialog.paste(&text);
            return;
        }
        if let Some(dialog) = &mut self.rename {
            dialog.paste(&text);
            return;
        }
        if let Some(dialog) = &mut self.go_to {
            dialog.paste(&text);
            return;
        }
        let Some(session) = session else {
            return;
        };
        self.refresh_metrics(session);
        if self.find_focused() {
            self.paste_into_find(session, &text);
        } else {
            self.paste_into_document(session, &text);
        }
        self.refresh_find(session);
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
    /// match how the view now shows it or its current text, moving its
    /// view back in range and its selection to valid positions without a
    /// preferred column, since columns may now mean something else.
    /// Documents already measured for the current settings and text are
    /// left alone.
    pub fn refresh_metrics(&self, session: &mut CodeSession) {
        let ids: Vec<DocumentId> = session.tabs().ids().collect();
        for &id in &ids {
            self.refresh_syntax(session, id);
        }
        for id in ids {
            let Some(document) = session.document(id) else {
                continue;
            };
            let geometry = self.geometry(document);
            let key = geometry.metrics_key();
            if session
                .tabs()
                .metrics(id)
                .is_some_and(|metrics| metrics.describes(document, key))
            {
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

    /// Highlights the lines of `id` the view shows, from where it is scrolled.
    /// A wrapped line takes at least one row, so the rows shown bound the
    /// lines shown.
    fn refresh_syntax(&self, session: &mut CodeSession, id: DocumentId) {
        let Some(document) = session.document(id) else {
            return;
        };
        let rows = self.geometry(document).rows;
        let top = session.tabs().view(id).map_or(0, |view| view.top_line);
        session.refresh_syntax(id, top..top + rows);
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
            self.completion = None;
            self.signature = None;
            self.drag = None;
            return None;
        }
        let session = session?;
        self.refresh_metrics(session);
        let before = (session.tabs().active(), session.tabs().len());
        let was_open = self.completion.is_some();
        let revision = session.active_document().map(CodeDocument::revision);
        let request = self.input(event, session);
        if (session.tabs().active(), session.tabs().len()) != before {
            self.reveal_active(session);
        }
        if session.active_document().is_none() {
            self.find = None;
        }
        self.refresh_metrics(session);
        self.refresh_find(session);
        self.sync_completion(session);
        self.request_completion(event, was_open, revision, session);
        self.update_signature(event, revision, session);
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
        if self.context_menu.is_some() {
            return self.context_menu_input(event, session);
        }
        if self.new_file.is_some() {
            return self.new_file_input(event);
        }
        if self.rename.is_some() {
            return self.rename_input(event);
        }
        if self.go_to.is_some() {
            return self.go_to_input(event, session);
        }
        if event == InputEvent::Docs {
            return Some(self.docs_request(session));
        }
        if let Some(request) = self.completion_input(event, session) {
            return request;
        }
        if self.find.is_some()
            && let Some(request) = self.find_input(event, session)
        {
            return request;
        }
        if let Some(request) = self.signature_input(event) {
            return request;
        }
        if let Some(request) = self.edit_input(event, session) {
            return request;
        }
        let labels = labels(&self.layout.font, session);
        let scroll = session.tabs().scroll();
        let ids: Vec<_> = session.tabs().ids().collect();
        match event {
            InputEvent::OpenFile => return Some(CodeRequest::ListScripts),
            InputEvent::NewFile => return Some(CodeRequest::NewFile),
            InputEvent::Find => self.open_find(session, false),
            InputEvent::Replace => self.open_find(session, true),
            InputEvent::GoToLine => self.open_go_to(session),
            InputEvent::Save => return session.tabs().active().map(CodeRequest::Save),
            InputEvent::CloseDocument => {
                return session
                    .tabs()
                    .active()
                    .and_then(|active| close_tab(session, active));
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
                    StripHit::Close(index) => return close_tab(session, ids[index]),
                    StripHit::More => {
                        let active = session.tabs().active();
                        if let Some(focus) = active.and_then(|id| session.tabs().index_of(id)) {
                            self.menu = Some(TabMenu { focus });
                        }
                    }
                    StripHit::None if session.active_document().is_none() => {
                        if let Some(action) = self.empty_action_at(at) {
                            return self.run_action(action, session);
                        }
                    }
                    StripHit::None if self.text_area().contains(at) => {
                        self.press_text(session, at, select);
                    }
                    StripHit::None => {}
                }
            }
            InputEvent::PointerDown {
                button: PointerButton::Secondary,
                at,
                ..
            } => {
                self.pointer = Some(at);
                self.end_drag();
                self.open_context_menu(session, &labels, at);
            }
            InputEvent::PointerDown {
                button: PointerButton::Middle,
                at,
                ..
            } => {
                if let StripHit::Tab(index) | StripHit::Close(index) =
                    self.layout.strip_hit(&labels, scroll, at)
                {
                    return close_tab(session, ids[index]);
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

    /// Opens the context menu for what a secondary press at `at` lands on:
    /// a tab or the strip, the empty view, or the active document's text.
    /// A press in the text outside the selection first moves the caret
    /// there, so the menu acts where it was opened.
    fn open_context_menu(&mut self, session: &mut CodeSession, labels: &[String], at: Point) {
        let tab = match self.layout.strip_hit(labels, session.tabs().scroll(), at) {
            StripHit::Tab(index) | StripHit::Close(index) => session.tabs().ids().nth(index),
            StripHit::More => return,
            StripHit::None if self.layout.strip.contains(at) => None,
            StripHit::None if session.active_document().is_none() => {
                if !self.layout.text.contains(at) {
                    return;
                }
                None
            }
            StripHit::None => {
                if !self.text_area().contains(at) {
                    return;
                }
                if let Some((id, byte)) = self.hit_text(session, at) {
                    let selection = session.tabs().selection(id).unwrap_or_default();
                    let range = selection.range();
                    let inside = !selection.is_empty() && range.start <= byte && byte <= range.end;
                    if !inside && let Some(document) = session.document(id) {
                        let caret = Selection::collapsed(Caret::at(document, byte));
                        self.show_selection(session, id, caret, None);
                    }
                }
                self.context_menu = Some(text_menu(session, at));
                return;
            }
        };
        let tab = tab.and_then(|id| {
            let path = session.document(id)?.path();
            Some(TabTarget {
                id,
                protected: self.entry.as_ref() == Some(path),
                renamable: path.as_str().starts_with(CODE_DIR),
            })
        });
        self.context_menu = Some(file_menu(tab, at));
    }

    fn context_menu_input(
        &mut self,
        event: InputEvent,
        session: &mut CodeSession,
    ) -> Option<CodeRequest> {
        let menu = self.context_menu.as_mut()?;
        let layout = {
            let metrics = self.layout.metrics;
            menu.layout(
                &self.layout.font,
                overlay_area(&self.layout),
                metrics.cell_w,
                metrics.cell_h,
            )
        };
        match menu.handle_input(event, &layout) {
            MenuOutcome::Stay => None,
            MenuOutcome::Close => {
                self.context_menu = None;
                None
            }
            MenuOutcome::Outside => {
                self.context_menu = None;
                // A secondary press elsewhere opens the menu there instead.
                match event {
                    InputEvent::PointerDown {
                        button: PointerButton::Secondary,
                        ..
                    } => self.input(event, session),
                    _ => None,
                }
            }
            MenuOutcome::Run(action) => {
                self.context_menu = None;
                self.run_action(action, session)
            }
        }
    }

    fn new_file_input(&mut self, event: InputEvent) -> Option<CodeRequest> {
        let layout = self.prompt_layout(self.new_file.as_ref()?.prompt());
        match self.new_file.as_mut()?.handle_input(event, &layout) {
            NewFileAction::None => None,
            NewFileAction::Create(path) => Some(CodeRequest::Create(path)),
            NewFileAction::Close => {
                self.new_file = None;
                None
            }
            NewFileAction::Copy(text) => Some(CodeRequest::Copy(text)),
            NewFileAction::Paste => Some(CodeRequest::Paste),
        }
    }

    fn rename_input(&mut self, event: InputEvent) -> Option<CodeRequest> {
        let layout = self.prompt_layout(self.rename.as_ref()?.prompt());
        match self.rename.as_mut()?.handle_input(event, &layout) {
            RenameAction::None => None,
            RenameAction::Rename { id, to } => Some(CodeRequest::Rename { id, to }),
            RenameAction::Close => {
                self.rename = None;
                None
            }
            RenameAction::Copy(text) => Some(CodeRequest::Copy(text)),
            RenameAction::Paste => Some(CodeRequest::Paste),
        }
    }

    /// Remembers which script is the cartridge's entry, from its manifest
    /// path, so the menus can keep it from being renamed or deleted.
    pub fn set_entry(&mut self, entry: &str) {
        if self.entry.as_ref().map(SourcePath::as_str) != Some(entry) {
            self.entry = SourcePath::parse(entry).ok();
        }
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
    /// anchor. Without it the selection is dropped, or, for moves that
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
        if !metrics.describes(document, geometry.metrics_key()) {
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
        // A click shows where it landed. A word or line that more clicks
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
        if !metrics.describes(document, geometry.metrics_key()) {
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

/// Closes `id`'s tab if its document has no unsaved edits, and otherwise
/// asks to confirm.
fn close_tab(session: &mut CodeSession, id: DocumentId) -> Option<CodeRequest> {
    if session.document(id).is_some_and(CodeDocument::is_modified) {
        return Some(CodeRequest::ConfirmClose(id));
    }
    session.close(id);
    None
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
