//! The editor workspace is a tab bar over one view per cartridge resource.
//!
//! The workspace works on the loaded cartridge without starting it. The
//! system lends it the cartridge and the documents the editors change for
//! each tick and frame. The workspace keeps only its own UI state, so the
//! selected tab and each editor's view survive tab switches and trips to the
//! shell. File work the editors ask for goes back to the system as a
//! [`WorkspaceRequest`], and its outcome comes back as a brief notice or a
//! modal [`Dialog`].

mod answers;
mod code;
mod colors;
mod context_menu;
mod debugging;
mod dialog;
mod docs;
mod docs_apart;
mod input;
mod menu;
mod modal;
mod render;
mod request;
mod sprites;
mod tab;
mod text_field;

use glaux_machine::font::BitmapFont;
#[cfg(test)]
use glaux_machine::font::FontMetrics;
use glaux_machine::machine::TICK_RATE;
use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, OBJ_PALETTE_COLORS};
use glaux_machine::palette::Palette;

use super::docs as library_docs;
use super::editing::session::CodeSession;
use super::editor_settings::EditorSettings;
use super::screen::PALE;
use crate::ui_text;
pub(super) use answers::saved_before_stopping;
use code::CodeEditor;
pub use code::DebugAction;
pub use colors::WorkspaceVideo;
pub(super) use dialog::{Dialog, DialogLayout};
use docs::{DocsBrowser, Source};
pub use docs_apart::{DocsApart, DocsStart};
use input::Route;
use modal::Modal;
pub(super) use modal::{
    Choice, Target, asset_conflict_dialog, message_dialog, script_conflict_dialog,
    source_write_advice, unsaved_dialog, write_advice,
};
pub use render::draw_cursor;
pub use request::{AssetRequest, HostWork, NavigationRequest, ScriptRequest, WorkspaceRequest};
#[cfg(test)]
pub use sprites::ImportPreview;
use sprites::SpriteEditor;
pub(crate) use tab::take_cells;
use tab::{Layout, fit_name};
pub use tab::{Rect, Tab, TabLabels};

/// Entries of the workspace's frame palette that keep the system palette's
/// colors. Every UI color index is below it. The cartridge's object colors
/// are placed after it.
const SYSTEM_COLORS: usize = 32;

// The UI colors, every master color the object palettes can reach and the
// sprite editor's dialog colors each keep entries of their own.
const _: () =
    assert!(SYSTEM_COLORS + OBJ_PALETTE_BANKS * (OBJ_PALETTE_COLORS - 1) <= sprites::SCRATCH_BASE);

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
/// Space around the label of the Docs button.
const DOCS_PAD: i32 = 3;
/// Space between the Docs button and the right edge.
const DOCS_MARGIN: i32 = 2;
/// Fewest cells worth showing of a truncated cartridge name.
const MIN_NAME_CELLS: usize = 4;
/// Marks a cartridge name cut short to fit the bar.
const ELLIPSIS: &str = "..";
/// How long a notice stays in the status bar, in ticks.
const NOTICE_TICKS: u32 = 3 * TICK_RATE;
/// Ticks each frame of an animated cursor shows, as long as each of the
/// busy hourglass's source frames.
const CURSOR_FRAME_TICKS: u32 = TICK_RATE * 15 / 100;

/// A short message in the status bar answering what the user just did.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Notice {
    text: String,
    color: u8,
    ticks_left: u32,
}

/// `text` broken into lines of at most `columns` of `font`'s cells, at
/// spaces where possible.
pub(in crate::system) fn wrap(font: &BitmapFont, text: &str, columns: usize) -> Vec<String> {
    let columns = columns.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word = word;
        while font.text_cells(word) > columns {
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // A row always takes at least one character, even one wider
            // than the row.
            let head = match take_cells(font, word, columns) {
                "" => word.chars().next().map_or("", |ch| &word[..ch.len_utf8()]),
                head => head,
            };
            lines.push(head.to_owned());
            word = &word[head.len()..];
        }
        let len = font.text_cells(&line);
        if len > 0 && len + 1 + font.text_cells(word) > columns {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The workspace's UI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    layout: Layout,
    /// The system font, which measures the text the views lay out.
    font: BitmapFont,
    tab: Tab,
    tab_labels: TabLabels,
    code: CodeEditor,
    sprites: SpriteEditor,
    docs: DocsBrowser,
    /// Whether the documentation browser covers the selected view. Hiding
    /// it keeps its tabs.
    docs_open: bool,
    /// While open, the dialog takes every input and the views take none.
    modal: Option<Modal>,
    notice: Option<Notice>,
    /// Ticks since the workspace was created, which time animated cursors.
    ticks: u32,
    /// The system palette, whose first [`SYSTEM_COLORS`] entries the UI is
    /// drawn with.
    system_palette: Palette,
}

impl Workspace {
    /// A workspace on the Code tab, laid out for `font` and drawn in
    /// `system_palette`.
    pub fn new(font: &BitmapFont, system_palette: Palette) -> Self {
        let layout = Layout::new(font.metrics());
        Self {
            layout,
            font: font.clone(),
            tab: Tab::default(),
            tab_labels: TabLabels::new(&ui_text::translator()),
            code: CodeEditor::new(
                layout.code_metrics(),
                font.clone(),
                EditorSettings::default(),
            ),
            sprites: SpriteEditor::new(layout.sprite_metrics()),
            docs: DocsBrowser::new(layout.docs_metrics(), font.clone()),
            docs_open: false,
            modal: None,
            notice: None,
            ticks: 0,
            system_palette,
        }
    }

    #[cfg(test)]
    pub fn sprites(&self) -> &SpriteEditor {
        &self.sprites
    }

    #[cfg(test)]
    pub fn code(&self) -> &CodeEditor {
        &self.code
    }

    /// Shows code with `settings` from now on.
    pub fn set_editor_settings(&mut self, settings: EditorSettings) {
        self.code.set_settings(settings);
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// The Code view's settings, the ones kept across launches.
    pub fn editor_settings(&self) -> EditorSettings {
        self.code.settings()
    }

    /// Tells the Code view which script is the cartridge's entry, as the
    /// manifest gives it.
    pub fn set_entry_script(&mut self, entry: &str) {
        self.code.set_entry(entry);
    }

    /// Advances time-limited UI state by one tick, letting the Code view
    /// scroll `code` while a selection is dragged past its text.
    pub fn tick(&mut self, code: Option<&mut CodeSession>) {
        self.ticks = self.ticks.wrapping_add(1);
        self.sprites.tick();
        if self.tab != Tab::Code || self.docs_open || self.has_modal() {
            self.code.close_hover();
        }
        self.code.tick(code);
        if let Some(notice) = &mut self.notice {
            notice.ticks_left = notice.ticks_left.saturating_sub(1);
            if notice.ticks_left == 0 {
                self.notice = None;
            }
        }
    }

    #[cfg(test)]
    pub fn notice_text(&self) -> Option<&str> {
        self.notice.as_ref().map(|notice| notice.text.as_str())
    }

    /// Shows `text` in the status bar for a while.
    pub fn report(&mut self, text: String) {
        self.notify(text, PALE);
    }

    fn notify(&mut self, text: impl Into<String>, color: u8) {
        self.notice = Some(Notice {
            text: text.into(),
            color,
            ticks_left: NOTICE_TICKS,
        });
    }
}

/// The embedded documentation and the languages it is read in.
fn docs_source() -> Source<'static> {
    #[cfg(test)]
    if let Some(source) = tests_support::docs_override() {
        return source;
    }
    Source {
        library: library_docs::library(),
        chain: library_docs::chain(),
    }
}

#[cfg(test)]
pub(super) mod tests_support;

#[cfg(test)]
mod tests;
