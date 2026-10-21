//! The editor workspace is a tab bar over one view per cartridge resource.
//!
//! The workspace works on the loaded cartridge without starting it. The
//! system lends it the cartridge and the documents the editors change for
//! each tick and frame. The workspace keeps only its own UI state, so the
//! selected tab and each editor's view survive tab switches and trips to the
//! shell. File work the editors ask for goes back to the system as a
//! [`WorkspaceRequest`], and its outcome comes back as a brief notice or a
//! modal [`Dialog`].

mod code;
mod context_menu;
mod dialog;
mod docs;
mod sprites;
mod text_field;

use glaux_i18n::{Arg, Translator};
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::{BitmapFont, FontMetrics};
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use glaux_machine::ink_image::INK_SLOTS;
use glaux_machine::machine::TICK_RATE;
use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, OBJ_PALETTE_COLORS, ObjPalettes};
use glaux_machine::palette::Palette;
use glaux_machine::system_rom::{Cursor, SystemCursor, SystemIcon, SystemUi};
use glaux_video::draw::{clear, draw_ink_image, draw_text, draw_text_scaled, fill_rect};
use glaux_video::preview::ObjPreviewPalette;

use super::code_document::{
    CodeDocumentError, CodeSaveError, CodeSaved, DiskState, DocumentId, SourceConflict,
    SourceConflictKind,
};
use super::code_session::{CodeSession, MoveScriptError};
use super::docs as library_docs;
use super::editor_settings::EditorSettings;
use super::sprite_document::{
    Conflict, ConflictKind, DiskAsset, Reopened, SaveError, SaveFailure, Saved, SavedFile,
    SpriteDocument, file_list,
};
use super::video::CartridgeVideoDefaults;
use super::{ACCENT, BACKGROUND, DIM, ERROR, ExternalUrl, OK, PALE, TEXT};
use crate::cartridge::source::{ScriptListing, SourceEditError, SourcePath};
use crate::cartridge::{CartridgeError, Language, LoadedCartridge, VideoAsset};
use crate::cartridge_fs::FsError;
use crate::rom::ICON_SIZE;
use crate::sprite_import::SheetSource;
use crate::ui_text;
use code::{CodeEditor, CodeRequest};
pub(super) use dialog::{Dialog, DialogLayout};
use docs::{DocTarget, DocsBrowser, DocsRequest, Source};
use glaux_docs::DocId;
#[cfg(test)]
pub use sprites::ImportPreview;
#[cfg(not(test))]
use sprites::ImportPreview;
pub use sprites::SpriteSource;
use sprites::{SpriteEditor, SpriteRequest};

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

/// File work the workspace asks the system to do on the loaded cartridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceRequest {
    /// Save every modified asset of the sprite document.
    Save,
    /// Save the sprite document, writing over these files, which changed
    /// outside Glaux, provided each still holds what the user was shown.
    Overwrite(Vec<DiskAsset>),
    /// Discard the changes to these assets, and the history steps involving
    /// them, and read their files again.
    Reload(Vec<VideoAsset>),
    /// Ask the host's file dialog for a PNG spritesheet to import. The
    /// workspace takes no other input until [`Workspace::sheet_read`] or
    /// [`Workspace::file_dialog_closed`] answers it, or Escape stops the
    /// wait.
    PickSpritesheet,
    /// List the cartridge's scripts for OPEN FILE. Answer with
    /// [`Workspace::scripts_listed`].
    ListScripts,
    /// Open this script in a tab, or show its tab if it has one. Answer
    /// with [`Workspace::script_opened`].
    OpenScript(SourcePath),
    /// Keep the Code view's settings for the next launch.
    SaveEditorSettings(EditorSettings),
    /// Write this script document to its file. Answer with
    /// [`Workspace::script_saved`].
    SaveScript(DocumentId),
    /// Write this script document over its file, which changed outside
    /// Glaux, provided it still holds `seen` (`None` means it is still
    /// absent). Answer with [`Workspace::script_saved`].
    OverwriteScript {
        id: DocumentId,
        seen: Option<Vec<u8>>,
    },
    /// Write this script document to its file and, once it is written,
    /// close its tab. Answer with [`Workspace::script_saved`] and, if it
    /// closed, [`Workspace::script_closed`].
    SaveAndCloseScript(DocumentId),
    /// Ask for a new script's path. Answer with
    /// [`Workspace::open_new_script`] and the cartridge's language.
    NewScript,
    /// Create this empty script and open it in a tab. Answer with
    /// [`Workspace::script_created`].
    CreateScript(SourcePath),
    /// Read the state of this script's file for DELETE SCRIPT. Answer with
    /// [`Workspace::delete_prompt`].
    PrepareDelete(DocumentId),
    /// Delete this script's file and close its tab, provided the file still
    /// holds `seen` (`None` means it is still absent). Answer with
    /// [`Workspace::script_deleted`].
    DeleteScript {
        id: DocumentId,
        seen: Option<Vec<u8>>,
    },
    /// Move this script's file to this path. Answer with
    /// [`Workspace::script_renamed`].
    RenameScript { id: DocumentId, to: SourcePath },
    /// Open this link in the program the host uses for it.
    OpenExternal(ExternalUrl),
    /// Put this text on the host's clipboard.
    Copy(String),
    /// Read the host's clipboard. Answer with [`Workspace::paste`].
    Paste,
}

/// What a dialog's buttons do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Choice {
    /// Save unsaved edits, then go on with what asked.
    Save,
    /// Drop unsaved edits and go on with what asked.
    Discard,
    Overwrite,
    Reload,
    /// Delete the script's file.
    Delete,
    Cancel,
    /// Close a message that offers no decision.
    Dismiss,
}

/// The open dialog and what its choice acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Modal {
    dialog: Dialog<Choice>,
    target: Target,
}

/// What a dialog's choice acts on, as the dialog showed it: what Overwrite
/// may replace, Reload reads again, or Save and Discard close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Target {
    /// The dialog acts on nothing.
    None,
    /// Asset files of the sprite document.
    Assets(Vec<DiskAsset>),
    /// The file of one script document, holding `seen` (`None` means absent).
    Script {
        id: DocumentId,
        seen: Option<Vec<u8>>,
    },
    /// The tab of a script document with unsaved edits.
    CloseScript(DocumentId),
    /// The file of a script document to delete, holding `seen` when the
    /// dialog was shown (`None` means absent).
    DeleteScript {
        id: DocumentId,
        seen: Option<Vec<u8>>,
    },
}

/// A short message in the status bar answering what the user just did.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Notice {
    text: String,
    color: u8,
    ticks_left: u32,
}

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

    /// The id of the message holding the tab's name.
    pub fn message_id(self) -> &'static str {
        match self {
            Self::Code => "tab-code",
            Self::Sprites => "tab-sprites",
            Self::Map => "tab-map",
            Self::Sfx => "tab-sfx",
            Self::Music => "tab-music",
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
    fn purpose(self) -> String {
        match self {
            Self::Code => ui_text::get("workspace-edit-the-cartridge-s-luau-and-rhai-scripts"),
            Self::Sprites => ui_text::get("workspace-draw-object-tiles-and-their-palettes"),
            Self::Map => ui_text::get("workspace-lay-out-tiles-into-maps"),
            Self::Sfx => ui_text::get("workspace-shape-sound-effects-for-the-psg"),
            Self::Music => ui_text::get("workspace-arrange-sound-effects-into-songs"),
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
    pub(crate) fn contains_xy(self, x: i32, y: i32) -> bool {
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
    const fn new(metrics: FontMetrics) -> Self {
        Self {
            cell_w: metrics.cell_width() as i32,
            cell_h: metrics.cell_height() as i32,
        }
    }

    /// Height of the tab bar: an icon, or one text row with a pixel above
    /// and below, whichever is taller.
    const fn bar_height(self) -> i32 {
        let text = self.cell_h + 2;
        let icon = ICON_SIZE as i32;
        if text > icon { text } else { icon }
    }

    /// The cell size and top of the Code view, below the tab bar.
    const fn code_metrics(self) -> code::Metrics {
        code::Metrics {
            cell_w: self.cell_w,
            cell_h: self.cell_h,
            top: self.bar_height(),
        }
    }

    /// The cell size and top of the documentation browser, below the tab
    /// bar.
    const fn docs_metrics(self) -> docs::Metrics {
        docs::Metrics {
            cell_w: self.cell_w,
            cell_h: self.cell_h,
            top: self.bar_height(),
        }
    }

    /// The cell size and top of the sprite editor, below the tab bar.
    const fn sprite_metrics(self) -> sprites::Metrics {
        sprites::Metrics {
            cell_w: self.cell_w,
            cell_h: self.cell_h,
            top: self.bar_height(),
        }
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

    /// The screen below the tab bar, where the views and dialogs go.
    fn view_area(self) -> Rect {
        let top = self.bar_height();
        Rect {
            x: 0,
            y: top,
            width: WIDTH as i32,
            height: HEIGHT as i32 - top,
        }
    }

    /// Text row origin within the bar.
    fn text_y(self) -> i32 {
        (self.bar_height() - self.cell_h) / 2 + 1
    }

    /// The Docs button at the bar's right end, showing `label`.
    fn docs_button(self, label: &str, font: &BitmapFont) -> Rect {
        let width = text_width(font, label) + 2 * DOCS_PAD;
        Rect {
            x: WIDTH as i32 - DOCS_MARGIN - width,
            y: 0,
            width,
            height: self.bar_height(),
        }
    }

    /// Where the cartridge name ends: left of the Docs button.
    fn name_right(self, labels: &TabLabels, font: &BitmapFont) -> i32 {
        self.docs_button(labels.docs(), font).x - NAME_PAD
    }

    /// Cells available to the cartridge name, right of the longest hint.
    fn name_cells(self, labels: &TabLabels, font: &BitmapFont) -> usize {
        let longest = i32::try_from(labels.longest_cells()).unwrap_or(0) * self.cell_w;
        let start = self.hint_x() + longest + NAME_GAP;
        let space = self.name_right(labels, font) - start;
        usize::try_from(space / self.cell_w.max(1)).unwrap_or(0)
    }
}

/// The tabs' names in the interface language, resolved once because the
/// language does not change while the workspace exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabLabels {
    tabs: [String; Tab::ALL.len()],
    /// The Docs button's label, and the hint naming it.
    docs: String,
    docs_hint: String,
}

impl TabLabels {
    pub fn new(text: &Translator) -> Self {
        Self {
            tabs: Tab::ALL.map(|tab| text.get(tab.message_id()).into_owned()),
            docs: text.get("docs-button").into_owned(),
            docs_hint: text.get("docs-button-hint").into_owned(),
        }
    }

    pub fn get(&self, tab: Tab) -> &str {
        &self.tabs[tab.index()]
    }

    /// The Docs button's label.
    fn docs(&self) -> &str {
        &self.docs
    }

    /// The hint shown while the pointer is over the Docs button.
    fn docs_hint(&self) -> &str {
        &self.docs_hint
    }

    /// The width in cells of the longest name the hint area shows.
    fn longest_cells(&self) -> usize {
        self.tabs
            .iter()
            .chain([&self.docs_hint])
            .map(|label| label.chars().count())
            .max()
            .unwrap_or(0)
    }
}

/// Width in pixels of `text` drawn on one line with `font`.
fn text_width(font: &BitmapFont, text: &str) -> i32 {
    let cells = i32::try_from(font.text_cells(text)).unwrap_or(i32::MAX);
    cells.saturating_mul(i32::from(font.metrics().cell_width()))
}

/// The longest start of `text` that takes at most `cells` of `font`'s
/// cells.
pub(crate) fn take_cells<'a>(font: &BitmapFont, text: &'a str, cells: usize) -> &'a str {
    let mut used = 0;
    for (at, ch) in text.char_indices() {
        used += font.cells(ch);
        if used > cells {
            return &text[..at];
        }
    }
    text
}

/// `name` as it fits in `cells` of `font`'s cells: whole, cut short with
/// [`ELLIPSIS`], or `None` when too little of it would show.
fn fit_name(font: &BitmapFont, name: &str, cells: usize) -> Option<String> {
    if font.text_cells(name) <= cells {
        return Some(name.to_owned());
    }
    let room = cells.checked_sub(font.text_cells(ELLIPSIS))?;
    let kept = take_cells(font, name, room);
    (font.text_cells(kept) >= MIN_NAME_CELLS).then(|| format!("{kept}{ELLIPSIS}"))
}

/// `text` broken into lines of at most `columns` of `font`'s cells, at
/// spaces where possible.
fn wrap(font: &BitmapFont, text: &str, columns: usize) -> Vec<String> {
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

/// What to tell the user when saving an asset file failed.
pub(super) fn write_advice(err: &CartridgeError) -> String {
    match err {
        CartridgeError::AssetReadOnly(asset) => ui_text::get_with(
            "workspace-is-read-only-make-it-writable-then-save-again-2",
            &[("path", Arg::Text(&asset.to_string()))],
        ),
        CartridgeError::AssetUnwritable(asset) => ui_text::get_with(
            "workspace-glaux-could-not-write-check-the-cartridge-folder-s-permissions-an-2",
            &[("path", Arg::Text(&asset.to_string()))],
        ),
        CartridgeError::AssetUnreadable(asset) => ui_text::get_with(
            "workspace-glaux-could-not-read-to-check-it-for-outside-changes-check-its-pe-2",
            &[("path", Arg::Text(&asset.to_string()))],
        ),
        CartridgeError::AssetNotAFile(asset) => ui_text::get_with(
            "workspace-is-not-a-regular-file-move-it-aside-then-save-again-2",
            &[("path", Arg::Text(&asset.to_string()))],
        ),
        other => crate::system::ui_errors::cartridge(other),
    }
}

/// What to tell the user when saving the script at `path` failed.
pub(super) fn source_write_advice(path: &SourcePath, err: &FsError) -> String {
    match err {
        FsError::ReadOnly(_) => ui_text::get_with(
            "workspace-is-read-only-make-it-writable-then-save-again-2",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        FsError::NotAFile(_) => ui_text::get_with(
            "workspace-is-not-a-regular-file-move-it-aside-then-save-again-2",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        FsError::Unreadable(_) => ui_text::get_with(
            "workspace-glaux-could-not-read-to-check-it-for-outside-changes-check-its-pe-2",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        FsError::NotFound(_) | FsError::NotADirectory(_) => ui_text::get_with(
            "workspace-save-missing-folder",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        FsError::Unwritable(_) | FsError::AlreadyExists(_) => ui_text::get_with(
            "workspace-glaux-could-not-write-check-the-cartridge-folder-s-permissions-an-2",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        FsError::Unavailable => ui_text::get("workspace-cartridge-storage-is-unavailable"),
    }
}

/// The line telling which files a save that stopped partway did write.
fn saved_line(written: &[SavedFile]) -> Option<String> {
    let names: Vec<String> = written
        .iter()
        .map(|file| file.asset.file_name().to_owned())
        .collect();
    saved_before_stopping(&names)
}

/// The line telling which of `names` a run of saves that stopped partway
/// did write, if any.
pub(super) fn saved_before_stopping(names: &[String]) -> Option<String> {
    (!names.is_empty()).then(|| {
        ui_text::get_with(
            "workspace-saved-before-stopping",
            &[("value", Arg::Text(&(names.join(", ").to_string())))],
        )
    })
}

/// `names` as one phrase, as in `a.luau, b.luau and graphics`.
fn phrase(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => ui_text::get_with(
            "workspace-and",
            &[
                ("value", Arg::Text(&(rest.join(", ").to_string()))),
                ("last", Arg::Text(&last.to_string())),
            ],
        ),
    }
}

/// Asks about unsaved edits before the pending command or exit.
/// Cancel is focused so a stray Enter loses nothing.
pub(super) fn unsaved_dialog(
    names: &[String],
    pending: &super::unsaved::Pending,
) -> Dialog<Choice> {
    use super::unsaved::Pending;
    let count = Arg::Number(names.len() as f64);
    let instructions = match pending {
        Pending::Command { action, .. } => ui_text::get_with(
            "workspace-unsaved-command",
            &[("count", count), ("action", Arg::Text(action))],
        ),
        Pending::Exit => ui_text::get_with("workspace-unsaved-exit", &[("count", count)]),
    };
    Dialog::new(
        ui_text::get("workspace-unsaved-title"),
        vec![
            ui_text::get_with(
                "workspace-unsaved-names",
                &[("names", Arg::Text(&phrase(names))), ("count", count)],
            ),
            instructions,
        ],
        vec![
            (&ui_text::get("workspace-save-all"), Choice::Save),
            (&ui_text::get("workspace-discard"), Choice::Discard),
            (&ui_text::get("workspace-cancel"), Choice::Cancel),
        ],
        2,
        Some(Choice::Cancel),
    )
}

/// Asks whether to save the unsaved edits to the script at `path` before
/// closing its tab. Cancel is focused so a stray Enter loses nothing.
fn close_dialog(path: &SourcePath) -> Dialog<Choice> {
    Dialog::new(
        ui_text::get("workspace-unsaved-title"),
        vec![
            ui_text::get_with(
                "workspace-has-unsaved-changes",
                &[("path", Arg::Text(&path.to_string()))],
            ),
            ui_text::get("workspace-close-unsaved-instructions"),
        ],
        vec![
            (&ui_text::get("workspace-save"), Choice::Save),
            (&ui_text::get("workspace-discard"), Choice::Discard),
            (&ui_text::get("workspace-cancel"), Choice::Cancel),
        ],
        2,
        Some(Choice::Cancel),
    )
}

/// Asks whether to delete the file at `path` of script document `id`.
/// `modified` is whether the document has unsaved edits, `external`
/// whether the file changed outside Glaux, and `seen` what the file holds
/// (`None` when it is gone). Cancel is focused so a stray Enter
/// deletes nothing.
fn delete_dialog(
    id: DocumentId,
    path: &SourcePath,
    modified: bool,
    external: bool,
    seen: Option<Vec<u8>>,
) -> (Dialog<Choice>, Target) {
    let mut lines = vec![ui_text::get_with(
        "workspace-delete-from-the-cartridge",
        &[("path", Arg::Text(&path.to_string()))],
    )];
    match &seen {
        None => lines.push(ui_text::get_with(
            "workspace-delete-already-missing",
            &[("path", Arg::Text(&path.to_string()))],
        )),
        Some(_) => lines.push(ui_text::get("workspace-the-file-cannot-be-recovered")),
    }
    if external {
        lines.push(ui_text::get("workspace-delete-external-warning"));
    }
    if modified {
        lines.push(ui_text::get("workspace-delete-unsaved-warning"));
    }
    let dialog = Dialog::new(
        ui_text::get("workspace-delete-script"),
        lines,
        vec![
            (&ui_text::get("workspace-delete"), Choice::Delete),
            (&ui_text::get("workspace-cancel"), Choice::Cancel),
        ],
        1,
        Some(Choice::Cancel),
    )
    .destructive(0);
    (dialog, Target::DeleteScript { id, seen })
}

/// What to tell the user when deleting or moving the script at `path`
/// failed.
fn edit_failure(path: &SourcePath, err: &SourceEditError) -> String {
    match err {
        SourceEditError::Fs(FsError::ReadOnly(_)) => ui_text::get_with(
            "workspace-is-read-only-make-it-writable-then-try-again",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        SourceEditError::Fs(FsError::AlreadyExists(existing)) => ui_text::get_with(
            "workspace-already-exists",
            &[("value", Arg::Text(existing.trim_start_matches('/')))],
        ),
        SourceEditError::Fs(FsError::Unwritable(_)) => ui_text::get_with(
            "workspace-edit-write-failed",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        err => crate::system::ui_errors::code::source_edit(err),
    }
}

/// Asks whether to overwrite the file of script document `id` in
/// `conflict` with the document's text or leave both as they are. `saved`,
/// if any, tells what was saved before the conflict was found. Cancel is
/// focused so a stray Enter loses nothing.
pub(super) fn script_conflict_dialog(
    id: DocumentId,
    conflict: SourceConflict,
    saved: Option<String>,
) -> (Dialog<Choice>, Target) {
    let SourceConflict { path, kind, disk } = conflict;
    let (what, replaces) = match kind {
        SourceConflictKind::Modified => (
            ui_text::get_with(
                "workspace-changed-outside-glaux",
                &[("path", Arg::Text(&path.to_string()))],
            ),
            &ui_text::get("workspace-overwrite-disk-replaces-the-file-with-your-edits"),
        ),
        SourceConflictKind::Deleted => (
            ui_text::get_with(
                "workspace-was-deleted-outside-glaux",
                &[("path", Arg::Text(&path.to_string()))],
            ),
            &ui_text::get("workspace-overwrite-disk-writes-your-edits-to-it-again"),
        ),
    };
    let mut lines = vec![what];
    lines.extend(saved);
    lines.push(ui_text::get("workspace-unsaved-edits-kept"));
    lines.push(replaces.to_owned());
    let dialog = Dialog::new(
        ui_text::get("workspace-file-conflict"),
        lines,
        vec![
            (&ui_text::get("workspace-overwrite-disk"), Choice::Overwrite),
            (&ui_text::get("workspace-cancel"), Choice::Cancel),
        ],
        1,
        Some(Choice::Cancel),
    );
    (dialog, Target::Script { id, seen: disk })
}

/// Asks whether to overwrite the asset files in `conflicts`, discard the
/// changes to them, or leave both as they are. `saved`, if any, tells what
/// was saved before the conflict was found. Cancel is focused so a stray
/// Enter loses nothing.
pub(super) fn asset_conflict_dialog(
    conflicts: Vec<Conflict>,
    saved: Option<String>,
) -> (Dialog<Choice>, Target) {
    let mut lines: Vec<String> = conflicts
        .iter()
        .map(|conflict| {
            let asset = conflict.asset();
            match conflict.kind {
                ConflictKind::Modified => ui_text::get_with(
                    "workspace-changed-outside-glaux",
                    &[("path", Arg::Text(&asset.to_string()))],
                ),
                ConflictKind::Deleted => ui_text::get_with(
                    "workspace-was-deleted-outside-glaux",
                    &[("path", Arg::Text(&asset.to_string()))],
                ),
                ConflictKind::Created => ui_text::get_with(
                    "workspace-a-new-appeared-outside-glaux",
                    &[("asset", Arg::Text(&asset.to_string()))],
                ),
            }
        })
        .collect();
    lines.extend(saved);
    lines.push(ui_text::get("workspace-unsaved-edits-kept"));
    lines.push(ui_text::get_with(
        "workspace-asset-conflict-instructions",
        &[("count", Arg::Number(conflicts.len() as f64))],
    ));
    let dialog = Dialog::new(
        ui_text::get("workspace-file-conflict"),
        lines,
        vec![
            (&ui_text::get("workspace-overwrite-disk"), Choice::Overwrite),
            (&ui_text::get("workspace-discard-reload"), Choice::Reload),
            (&ui_text::get("workspace-cancel"), Choice::Cancel),
        ],
        2,
        Some(Choice::Cancel),
    );
    let seen = conflicts
        .into_iter()
        .map(|conflict| conflict.disk)
        .collect();
    (dialog, Target::Assets(seen))
}

/// Shows `lines` under `title` until dismissed.
pub(super) fn message_dialog(title: &str, lines: Vec<String>) -> Dialog<Choice> {
    Dialog::new(
        title,
        lines,
        vec![(&ui_text::get("boot-ok"), Choice::Dismiss)],
        0,
        Some(Choice::Dismiss),
    )
}

/// Draws frame `frame` of `cursor` with its hotspot on `pointer`, if any.
/// Drawn over the finished frame, it covers everything below it.
pub fn draw_cursor(
    framebuffer: &mut Framebuffer,
    cursor: &Cursor,
    frame: usize,
    pointer: Option<Point>,
) {
    let (Some(at), Some(image)) = (pointer, cursor.frame(frame)) else {
        return;
    };
    let (hot_x, hot_y) = cursor.hotspot;
    draw_ink_image(
        framebuffer,
        image,
        i32::from(at.x) - i32::from(hot_x),
        i32::from(at.y) - i32::from(hot_y),
        &[OUTLINE, TEXT, TEXT],
    );
}

/// How the workspace shows the cartridge's colors: where the frame palette
/// holds each master color the object palettes use, or why the cartridge's
/// video state can't be shown.
#[derive(Debug)]
// Boxed: a frame palette would make every system state as large as this
// one.
pub struct WorkspaceVideo(Result<Box<PreparedVideo>, String>);

/// A frame palette with the palettes it was built from, to tell when it no
/// longer matches the document.
#[derive(Debug)]
struct PreparedVideo {
    preview: ObjPreviewPalette,
    /// The preview's palette with the sprite editor's dialog colors, if
    /// any, from [`sprites::SCRATCH_BASE`] on: what frames are presented
    /// with.
    frame: Palette,
    master: Palette,
    obj: ObjPalettes,
}

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
        self.0.as_ref().ok().map(|video| &video.frame)
    }

    /// What the sprite editor shows of `document`, or why it shows nothing.
    pub fn source<'a>(
        &'a self,
        document: Option<&'a SpriteDocument>,
    ) -> Result<SpriteSource<'a>, String> {
        let video = self.0.as_deref().map_err(Clone::clone)?;
        // The system only prepares a preview from a document it keeps.
        let document = document
            .ok_or_else(|| ui_text::get("workspace-the-cartridge-s-sprites-are-not-loaded"))?;
        Ok(SpriteSource {
            obj: document.obj(),
            master: &document.video().palette,
            preview: &video.preview,
            modified: document.is_modified(),
        })
    }

    /// Whether the frame palette was built from `document`'s palettes as
    /// they are now.
    fn matches(&self, document: &SpriteDocument) -> bool {
        self.0.as_ref().is_ok_and(|video| {
            video.master == document.video().palette && video.obj == *document.obj().palettes()
        })
    }
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

    /// Prepares to show `document` in the workspace's frame palette. It
    /// holds until the document's palettes change. See
    /// [`refresh_video`](Self::refresh_video).
    pub fn prepare_video(&self, document: &SpriteDocument) -> WorkspaceVideo {
        let video = document.video();
        match ObjPreviewPalette::new(
            &self.system_palette,
            SYSTEM_COLORS,
            &video.palette,
            video.obj.palettes(),
        ) {
            Ok(preview) => WorkspaceVideo(Ok(Box::new(PreparedVideo {
                frame: preview.palette().clone(),
                preview,
                master: video.palette.clone(),
                obj: video.obj.palettes().clone(),
            }))),
            Err(err) => WorkspaceVideo::unavailable(super::ui_errors::video::preview_palette(&err)),
        }
    }

    /// Prepares `video` again if `document`'s palettes changed since it was
    /// prepared, so the frame palette never shows stale colors, and gives
    /// the frame palette the colors of the sprite editor's open dialog.
    /// Video that could not be prepared stays as it is.
    pub fn refresh_video(&self, video: &mut WorkspaceVideo, document: &SpriteDocument) {
        if video.is_ready() && !video.matches(document) {
            *video = self.prepare_video(document);
        }
        if let Ok(prepared) = &mut video.0 {
            if let Some(palette) = self.sprites.import_frame_palette() {
                prepared.frame = palette.clone();
                return;
            }
            let mut frame = prepared.preview.palette().clone();
            if let Some(colors) = self.sprites.scratch_colors() {
                for (offset, color) in colors.into_iter().enumerate() {
                    // Scratch entries end at the palette's end, so fit u8.
                    frame.set((sprites::SCRATCH_BASE + offset) as u8, color);
                }
            }
            prepared.frame = frame;
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

    /// Applies `event`, letting the sprite editor change `sprites` and the
    /// Code view change `code`, and returns the file work it calls for, if
    /// any.
    ///
    /// While a dialog is open it takes every event, Escape included. So
    /// do the sprite editor's own dialogs and the Code view's overlays.
    /// Otherwise Ctrl+Tab and Ctrl+Shift+Tab cycle the tabs, a primary
    /// press on a tab selects it, and everything else goes to the open
    /// view's editor, if it has one.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        sprites: Option<&mut SpriteDocument>,
        code: Option<&mut CodeSession>,
    ) -> Option<WorkspaceRequest> {
        if let Some(modal) = &mut self.modal {
            let layout = modal.dialog.layout(
                &self.font,
                self.layout.view_area(),
                self.layout.cell_w,
                self.layout.cell_h,
            );
            let choice = modal.dialog.handle_input(event, &layout)?;
            let modal = self.modal.take()?;
            return match (choice, modal.target) {
                (Choice::Overwrite, Target::Assets(seen)) => {
                    Some(WorkspaceRequest::Overwrite(seen))
                }
                (Choice::Overwrite, Target::Script { id, seen }) => {
                    Some(WorkspaceRequest::OverwriteScript { id, seen })
                }
                (Choice::Reload, Target::Assets(seen)) => Some(WorkspaceRequest::Reload(
                    seen.iter().map(|disk| disk.asset).collect(),
                )),
                (Choice::Delete, Target::DeleteScript { id, seen }) => {
                    Some(WorkspaceRequest::DeleteScript { id, seen })
                }
                (Choice::Save, Target::CloseScript(id)) => {
                    Some(WorkspaceRequest::SaveAndCloseScript(id))
                }
                (Choice::Discard, Target::CloseScript(id)) => {
                    if let Some(code) = code {
                        code.close(id);
                        self.script_closed(code);
                    }
                    None
                }
                // Script conflicts offer no reload, and messages neither.
                _ => None,
            };
        }
        if self.sprites.has_dialog() {
            return self.sprite_input(event, sprites);
        }
        if self.code.has_overlay() {
            return self.code_input(event, code);
        }
        match event {
            InputEvent::NextView => {
                self.docs_open = false;
                self.switch_to(self.tab.next(), sprites);
            }
            InputEvent::PreviousView => {
                self.docs_open = false;
                self.switch_to(self.tab.previous(), sprites);
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } if self.layout.tab_at(at).is_some() => {
                if let Some(tab) = self.layout.tab_at(at) {
                    self.docs_open = false;
                    self.switch_to(tab, sprites);
                }
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } if self.docs_button().contains(at) => self.toggle_docs(sprites),
            // With a script open, the Code view decides what F1 shows.
            InputEvent::Docs if !self.docs_open && !(self.tab == Tab::Code && code.is_some()) => {
                self.toggle_docs(sprites)
            }
            _ if self.docs_open => return self.docs_input(event),
            _ if self.tab == Tab::Sprites => return self.sprite_input(event, sprites),
            _ if self.tab == Tab::Code => return self.code_input(event, code),
            _ => {}
        }
        None
    }

    /// The Docs button in the tab bar.
    fn docs_button(&self) -> Rect {
        self.layout.docs_button(self.tab_labels.docs(), &self.font)
    }

    /// Shows the documentation browser over the selected view, or hides it.
    /// Its tabs and history stay for the next time. Neither the Code view's
    /// documents nor the sprite editor's are touched.
    fn toggle_docs(&mut self, sprites: Option<&mut SpriteDocument>) {
        if self.docs_open {
            self.docs_open = false;
            return;
        }
        // A drag must not outlive the view it started in.
        self.end_stroke(sprites);
        self.dock_docs();
        self.docs.show(docs_source());
        self.docs_open = true;
    }

    /// Shows the browser on `target`, in a new tab or in the one that has
    /// the page already. Neither the Code view's documents nor the sprite
    /// editor's are touched.
    fn open_docs(&mut self, target: DocTarget) {
        let source = docs_source();
        if source.library.page(&target.id, source.chain).is_none() {
            self.notify(
                ui_text::get_with(
                    "code-no-documentation-for",
                    &[("value", Arg::Text(target.id.as_str()))],
                ),
                PALE,
            );
            return;
        }
        self.end_stroke(None);
        self.dock_docs();
        let refused = self.docs.show_target(source, target);
        self.docs_open = true;
        if let Some(DocsRequest::Notice(text)) = refused {
            self.notify(text, PALE);
        }
    }

    /// Shows the browser with its search open.
    fn search_docs(&mut self) {
        self.end_stroke(None);
        self.dock_docs();
        self.docs.show_search(docs_source());
        self.docs_open = true;
    }

    /// Puts the browser below the tab bar, where the workspace draws it.
    fn dock_docs(&mut self) {
        self.docs.set_top(self.layout.docs_metrics().top);
    }

    /// Prepares the browser to fill the screen, outside the workspace, on
    /// the front page or on `start`. The browser keeps its tabs and history
    /// from earlier openings.
    pub fn show_docs_apart(&mut self, start: DocsStart) {
        self.docs.set_top(0);
        let source = docs_source();
        match start {
            DocsStart::Home => self.docs.show(source),
            DocsStart::Page(id) => {
                self.docs.show_target(source, DocTarget::page(id));
            }
            DocsStart::Search(query) => self.docs.show_search_for(source, &query),
        }
    }

    /// Hands `event` to the browser shown apart from the workspace and
    /// carries out what it asks. The workspace's own keys do nothing here.
    pub fn docs_apart_input(&mut self, event: InputEvent) -> DocsApart {
        match self.docs.handle_input(event, docs_source()) {
            Some(DocsRequest::Close) => DocsApart::Close,
            Some(request) => self
                .docs_request(request)
                .map_or(DocsApart::Stay, DocsApart::Request),
            None => DocsApart::Stay,
        }
    }

    /// Draws the browser shown apart from the workspace.
    pub fn render_docs_apart(&self, framebuffer: &mut Framebuffer, pointer: Option<Point>) {
        let notice = self
            .notice
            .as_ref()
            .map(|notice| (notice.text.as_str(), notice.color));
        clear(framebuffer, BACKGROUND);
        self.docs
            .render(framebuffer, &self.font, docs_source(), pointer, notice);
    }

    /// Inserts the clipboard's `text` into the open search of the browser
    /// shown apart from the workspace.
    pub fn docs_apart_paste(&mut self, text: Option<String>) {
        self.docs.paste(docs_source(), text);
    }

    /// The pointer over the browser shown apart from the workspace.
    pub fn docs_apart_cursor(&self, at: Point) -> SystemCursor {
        self.docs.cursor(at, docs_source())
    }

    /// Hands `event` to the documentation browser and carries out what it
    /// asks.
    fn docs_input(&mut self, event: InputEvent) -> Option<WorkspaceRequest> {
        let request = self.docs.handle_input(event, docs_source())?;
        self.docs_request(request)
    }

    /// Carries out what the documentation browser asked for.
    fn docs_request(&mut self, request: DocsRequest) -> Option<WorkspaceRequest> {
        match request {
            DocsRequest::Close => {
                self.docs_open = false;
                None
            }
            DocsRequest::Notice(text) => {
                self.notify(text, PALE);
                None
            }
            DocsRequest::OpenExternal(url) => {
                self.notify(
                    ui_text::get_with("workspace-opening", &[("value", Arg::Text(url.as_str()))]),
                    PALE,
                );
                Some(WorkspaceRequest::OpenExternal(url))
            }
            DocsRequest::Copy(text) => Some(WorkspaceRequest::Copy(text)),
            DocsRequest::Paste => Some(WorkspaceRequest::Paste),
        }
    }

    /// Hands `event` to the Code view and passes on the file work it asks
    /// for.
    fn code_input(
        &mut self,
        event: InputEvent,
        mut code: Option<&mut CodeSession>,
    ) -> Option<WorkspaceRequest> {
        match self.code.handle_input(event, code.as_deref_mut())? {
            CodeRequest::ListScripts => Some(WorkspaceRequest::ListScripts),
            CodeRequest::Open(path) => Some(WorkspaceRequest::OpenScript(path)),
            CodeRequest::SaveSettings(settings) => {
                Some(WorkspaceRequest::SaveEditorSettings(settings))
            }
            CodeRequest::Save(id) => Some(WorkspaceRequest::SaveScript(id)),
            CodeRequest::Copy(text) => Some(WorkspaceRequest::Copy(text)),
            CodeRequest::Paste => Some(WorkspaceRequest::Paste),
            CodeRequest::NewFile => Some(WorkspaceRequest::NewScript),
            CodeRequest::Create(path) => Some(WorkspaceRequest::CreateScript(path)),
            CodeRequest::DeleteScript(id) => Some(WorkspaceRequest::PrepareDelete(id)),
            CodeRequest::Rename { id, to } => Some(WorkspaceRequest::RenameScript { id, to }),
            CodeRequest::Notice(text) => {
                self.notify(text, PALE);
                None
            }
            CodeRequest::OpenDocs(target) => {
                self.open_docs(target);
                None
            }
            CodeRequest::SearchDocs => {
                self.search_docs();
                None
            }
            CodeRequest::ConfirmClose(id) => {
                let document = code.as_deref().and_then(|code| code.document(id))?;
                self.code.end_drag();
                self.modal = Some(Modal {
                    dialog: close_dialog(document.path()),
                    target: Target::CloseScript(id),
                });
                None
            }
        }
    }

    /// Hands `event` to the sprite editor and carries out what it asks.
    fn sprite_input(
        &mut self,
        event: InputEvent,
        sprites: Option<&mut SpriteDocument>,
    ) -> Option<WorkspaceRequest> {
        match self.sprites.handle_input(event, sprites)? {
            SpriteRequest::Save => Some(WorkspaceRequest::Save),
            SpriteRequest::Import => Some(WorkspaceRequest::PickSpritesheet),
            SpriteRequest::Notice(text) => {
                self.notify(text, PALE);
                None
            }
        }
    }

    /// Whether a dialog, one of the sprite editor's, or one of the Code
    /// view's overlays is open and taking all input.
    pub fn has_modal(&self) -> bool {
        self.modal.is_some() || self.sprites.has_dialog() || self.code.has_overlay()
    }

    /// Whether Escape stays in the workspace, closing what is open there,
    /// rather than returning to the shell.
    pub fn claims_break(&self) -> bool {
        self.has_modal() || self.docs_open || (self.tab == Tab::Code && self.code.claims_break())
    }

    /// The pointer to show at `at`, with the Code view showing `code`, and
    /// the frame of it to draw.
    ///
    /// This is the one place that decides it: an open dialog's buttons
    /// and the tab plates take a pointing hand, and everything else is up
    /// to the view, whose drags keep their pointer even over the tab bar.
    pub fn cursor(&self, at: Point, code: Option<&CodeSession>) -> (SystemCursor, usize) {
        let frame = (self.ticks / CURSOR_FRAME_TICKS) as usize;
        let hand = |clickable: bool| {
            if clickable {
                SystemCursor::Hand
            } else {
                SystemCursor::Arrow
            }
        };
        let cursor = if self.sprites.awaits_file() {
            SystemCursor::Busy
        } else if let Some(modal) = &self.modal {
            let layout = modal.dialog.layout(
                &self.font,
                self.layout.view_area(),
                self.layout.cell_w,
                self.layout.cell_h,
            );
            hand(layout.button_at(at).is_some())
        } else if self.tab == Tab::Code && self.code.is_dragging() {
            self.code.cursor(at, code)
        } else if !self.has_modal()
            && (self.layout.tab_at(at).is_some() || self.docs_button().contains(at))
        {
            SystemCursor::Hand
        } else if self.docs_open {
            self.docs.cursor(at, docs_source())
        } else {
            match self.tab {
                Tab::Sprites => self.sprites.cursor(at),
                Tab::Code => self.code.cursor(at, code),
                _ => SystemCursor::Arrow,
            }
        };
        (cursor, frame)
    }

    /// Opens NEW FILE for a cartridge written in `language`.
    pub fn open_new_script(&mut self, language: Language) {
        self.code.open_new_file(language);
    }

    /// Reports how creating `path` in `code` went: the new script's tab
    /// shows once it is created and open, and NEW FILE says why otherwise.
    pub fn script_created(
        &mut self,
        code: &mut CodeSession,
        path: &SourcePath,
        result: Result<(), String>,
    ) {
        match &result {
            Ok(()) => {
                tracing::info!(file = %path, "script created");
                self.code.refresh_metrics(code);
                self.code.reveal_active(code);
                self.notify(
                    ui_text::get_with(
                        "workspace-created",
                        &[("path", Arg::Text(&path.to_string()))],
                    ),
                    PALE,
                );
            }
            Err(message) => {
                tracing::debug!(file = %path, error = %message, "script not created");
            }
        }
        self.code.script_created(result);
    }

    /// Tells the Code view which script is the cartridge's entry, as the
    /// manifest gives it.
    pub fn set_entry_script(&mut self, entry: &str) {
        self.code.set_entry(entry);
    }

    /// Opens DELETE SCRIPT for document `id`, given how its file compares
    /// with what the document was read from, or tells why that could not
    /// be read.
    pub fn delete_prompt(
        &mut self,
        code: &CodeSession,
        id: DocumentId,
        state: Result<DiskState, FsError>,
    ) {
        let Some(document) = code.document(id) else {
            return;
        };
        let path = document.path();
        let external = matches!(state, Ok(DiskState::Modified(_)));
        let seen = match state {
            Ok(DiskState::InSync(bytes) | DiskState::Modified(bytes)) => Some(bytes),
            Ok(DiskState::Missing) => None,
            Err(err) => {
                tracing::debug!(file = %path, %err, "script not readable for deletion");
                self.open_message(
                    &ui_text::get("workspace-delete-failed"),
                    vec![
                        ui_text::get_with(
                            "workspace-delete-read-failed",
                            &[("path", Arg::Text(&path.to_string()))],
                        ),
                        ui_text::get("workspace-nothing-was-deleted"),
                    ],
                );
                return;
            }
        };
        self.code.end_drag();
        let (dialog, target) = delete_dialog(id, path, document.is_modified(), external, seen);
        self.modal = Some(Modal { dialog, target });
    }

    /// Reports how deleting the file of script document `id` went. The
    /// tab closed if it was deleted. Otherwise a message says why not.
    pub fn script_deleted(
        &mut self,
        code: &mut CodeSession,
        path: &SourcePath,
        result: Result<(), SourceEditError>,
    ) {
        match result {
            Ok(()) => {
                tracing::info!(file = %path, "script deleted");
                self.code.refresh_metrics(code);
                self.script_closed(code);
                self.notify(
                    ui_text::get_with(
                        "workspace-deleted",
                        &[("path", Arg::Text(&path.to_string()))],
                    ),
                    PALE,
                );
            }
            Err(err) => {
                tracing::debug!(file = %path, %err, "script not deleted");
                self.open_message(
                    &ui_text::get("workspace-delete-failed"),
                    vec![
                        edit_failure(path, &err),
                        ui_text::get("workspace-nothing-was-deleted"),
                    ],
                );
            }
        }
    }

    /// Reports how moving the file of a script document to `to` went: the
    /// tab shows the new name once it moved, and RENAME SCRIPT says why
    /// otherwise.
    pub fn script_renamed(
        &mut self,
        code: &mut CodeSession,
        to: &SourcePath,
        result: Result<(), MoveScriptError>,
    ) {
        let result = result.map_err(|err| match &err {
            MoveScriptError::File(SourceEditError::Fs(fs)) => edit_failure(to, &fs.clone().into()),
            _ => crate::system::ui_errors::code::move_script(&err),
        });
        match &result {
            Ok(()) => {
                tracing::info!(file = %to, "script renamed");
                self.code.refresh_metrics(code);
                self.code.reveal_active(code);
                self.notify(
                    ui_text::get_with(
                        "workspace-renamed-to",
                        &[("to", Arg::Text(&to.to_string()))],
                    ),
                    PALE,
                );
            }
            Err(message) => {
                tracing::debug!(file = %to, error = %message, "script not renamed");
            }
        }
        self.code.script_renamed(result);
    }

    /// Hands the clipboard's `text` to the Code view, which inserts it
    /// where the keys are.
    pub fn paste(&mut self, code: &mut CodeSession, text: Option<String>) {
        if self.docs_open {
            self.docs.paste(docs_source(), text);
        } else if self.tab == Tab::Code {
            self.code.paste(text, Some(code));
        }
    }

    /// Opens OPEN FILE on the cartridge's scripts, or on why they could
    /// not be listed.
    pub fn scripts_listed(&mut self, listing: Result<ScriptListing, FsError>) {
        if let Ok(listing) = &listing
            && !listing.gaps.is_empty()
        {
            tracing::debug!(gaps = ?listing.gaps, "script listing is incomplete");
        }
        self.code.open_picker(listing);
    }

    /// Reports how opening `path` in `code` went, showing its tab if it
    /// opened.
    pub fn script_opened(
        &mut self,
        code: &mut CodeSession,
        path: &SourcePath,
        result: Result<(), CodeDocumentError>,
    ) {
        match result {
            Ok(()) => {
                self.code.refresh_metrics(code);
                self.code.reveal_active(code);
            }
            Err(err) => {
                tracing::debug!(file = %path, error = %err, "code document not opened");
                self.notify(
                    ui_text::get_with(
                        "workspace-cannot-open",
                        &[
                            ("path", Arg::Text(&path.to_string())),
                            (
                                "err",
                                Arg::Text(&crate::system::ui_errors::code::document(&err)),
                            ),
                        ],
                    ),
                    ERROR,
                );
            }
        }
    }

    /// Reports how opening the cartridge's entry script on the first visit
    /// to the Code view went. The reason it failed stays in the empty view.
    pub fn entry_opened(&mut self, code: &mut CodeSession, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.code.set_entry_error(None);
                self.code.refresh_metrics(code);
                self.code.reveal_active(code);
            }
            Err(message) => {
                tracing::debug!(error = %message, "entry script not opened");
                self.notify(
                    ui_text::get("workspace-cannot-open-the-entry-script"),
                    ERROR,
                );
                self.code.set_entry_error(Some(message));
            }
        }
    }

    /// Drops what the Code view shows of a cartridge that is no longer
    /// loaded.
    pub fn forget_code(&mut self) {
        self.code.forget();
    }

    /// Advances time-limited UI state by one tick, letting the Code view
    /// scroll `code` while a selection is dragged past its text.
    pub fn tick(&mut self, code: Option<&mut CodeSession>) {
        self.ticks = self.ticks.wrapping_add(1);
        self.sprites.tick();
        self.code.tick(code);
        if let Some(notice) = &mut self.notice {
            notice.ticks_left = notice.ticks_left.saturating_sub(1);
            if notice.ticks_left == 0 {
                self.notice = None;
            }
        }
    }

    /// Reports how a save, or an overwrite after a conflict, went.
    pub fn saved(&mut self, result: Result<Saved, SaveError>) {
        match result {
            Ok(Saved::Written(files)) => match files.as_slice() {
                [file] => self.notify(
                    ui_text::get_with(
                        "workspace-saved-bytes",
                        &[
                            ("value", Arg::Text(&file.asset.to_string())),
                            ("value-2", Arg::Text(&file.bytes.to_string())),
                        ],
                    ),
                    OK,
                ),
                files => self.notify(
                    ui_text::get_with(
                        "workspace-saved",
                        &[(
                            "value",
                            Arg::Text(&(file_list(files.iter().map(|file| file.asset)))),
                        )],
                    ),
                    OK,
                ),
            },
            Ok(Saved::NothingToSave) => {
                self.notify(ui_text::get("workspace-nothing-to-save"), PALE)
            }
            Err(SaveError { written, cause }) => {
                let saved = saved_line(&written);
                match cause {
                    SaveFailure::Conflict(conflicts) => self.open_conflict(conflicts, saved),
                    SaveFailure::Cartridge(err) => {
                        let lines = saved.into_iter().chain([write_advice(&err)]).collect();
                        self.open_error(&ui_text::get("workspace-save-failed"), lines);
                    }
                }
            }
        }
    }

    /// Reports how saving script document `id`, or overwriting its file
    /// after a conflict, went. A conflict asks whether to overwrite the
    /// file. The edits stay in memory whatever happened.
    pub fn script_saved(&mut self, id: DocumentId, result: Result<CodeSaved, CodeSaveError>) {
        match result {
            Ok(CodeSaved::Written { path, bytes }) => {
                self.notify(
                    ui_text::get_with(
                        "workspace-saved-bytes",
                        &[
                            ("value", Arg::Text(&path.to_string())),
                            ("value-2", Arg::Text(&bytes.to_string())),
                        ],
                    ),
                    OK,
                );
            }
            Ok(CodeSaved::NothingToSave(_)) => {
                self.notify(ui_text::get("workspace-nothing-to-save"), PALE)
            }
            Err(CodeSaveError::NotOpen) => {
                self.notify(ui_text::get("workspace-that-file-is-no-longer-open"), PALE)
            }
            Err(CodeSaveError::Conflict(conflict)) => self.open_script_conflict(id, conflict),
            Err(CodeSaveError::Failed { path, error }) => {
                tracing::debug!(file = %path, %error, "script not saved");
                self.open_error(
                    &ui_text::get("workspace-save-failed"),
                    vec![source_write_advice(&path, &error)],
                );
            }
        }
    }

    /// Shows the tabs left after a script document was closed outside the
    /// Code view's own input.
    pub fn script_closed(&mut self, code: &mut CodeSession) {
        self.code.reveal_active(code);
    }

    fn open_script_conflict(&mut self, id: DocumentId, conflict: SourceConflict) {
        let (dialog, target) = script_conflict_dialog(id, conflict, None);
        self.modal = Some(Modal { dialog, target });
    }

    /// Reports how discarding the changes to `assets` and reading their
    /// files again went.
    pub fn reloaded(&mut self, assets: &[VideoAsset], result: Result<(), String>) {
        match result {
            Ok(()) => self.notify(
                ui_text::get_with(
                    "workspace-reloaded-from-disk",
                    &[("value", Arg::Text(&(file_list(assets.iter().copied()))))],
                ),
                OK,
            ),
            Err(message) => {
                self.open_error(&ui_text::get("workspace-reload-failed"), vec![message])
            }
        }
    }

    /// Whether the host's file dialog was asked for a spritesheet and has
    /// not answered.
    pub fn awaits_file(&self) -> bool {
        self.sprites.awaits_file()
    }

    /// The host's file dialog closed without a file: it was cancelled, or
    /// it could not be shown.
    pub fn file_dialog_closed(&mut self) {
        self.sprites.stop_awaiting_file();
        self.notify(ui_text::get("workspace-no-file-chosen"), PALE);
    }

    /// The host could not open its file dialog, for the reason in
    /// `message`.
    pub fn file_dialog_failed(&mut self, message: &str) {
        self.sprites.stop_awaiting_file();
        self.notify(message.to_owned(), PALE);
    }

    /// Opens IMPORT SPRITES on the spritesheet read from a file called
    /// `name`, converted for `document`, or explains why it could not be
    /// read. Conversion starts from the System ROM palettes in `defaults`.
    pub fn sheet_read(
        &mut self,
        name: String,
        read: anyhow::Result<SheetSource>,
        document: &SpriteDocument,
        defaults: &CartridgeVideoDefaults,
    ) {
        self.sprites.stop_awaiting_file();
        match read {
            Ok(source) => self.sprites.open_import(ImportPreview::new(
                name,
                source,
                document,
                defaults.clone(),
                self.system_palette.clone(),
            )),
            Err(err) => self.open_message(
                &ui_text::get("workspace-cannot-import"),
                vec![
                    format!("{err:#}"),
                    ui_text::get("workspace-nothing-was-changed"),
                ],
            ),
        }
    }

    /// Reports what reopening the workspace found on disk.
    pub fn reopened(&mut self, reopened: Result<Reopened, String>) {
        let reopened = match reopened {
            Ok(reopened) => reopened,
            Err(message) => {
                return self
                    .open_error(&ui_text::get("workspace-cannot-read-files"), vec![message]);
            }
        };
        if reopened.history_trimmed {
            self.notify(
                ui_text::get_with(
                    "workspace-changed-on-disk-reloaded-undo-steps-dropped",
                    &[(
                        "value",
                        Arg::Text(&(file_list(reopened.reloaded.iter().copied()))),
                    )],
                ),
                PALE,
            );
        }
        if !reopened.conflicts.is_empty() {
            self.open_conflict(reopened.conflicts, None);
        }
    }

    fn open_conflict(&mut self, conflicts: Vec<Conflict>, saved: Option<String>) {
        let (dialog, target) = asset_conflict_dialog(conflicts, saved);
        self.modal = Some(Modal { dialog, target });
    }

    fn open_error(&mut self, title: &str, mut lines: Vec<String>) {
        lines.push(ui_text::get("workspace-your-edits-are-still-in-memory"));
        self.open_message(title, lines);
    }

    /// Shows `lines` under `title` until dismissed.
    fn open_message(&mut self, title: &str, lines: Vec<String>) {
        self.modal = Some(Modal {
            dialog: message_dialog(title, lines),
            target: Target::None,
        });
    }

    /// Lays `dialog` out over the views, or over the whole screen when
    /// `whole_screen`.
    pub(super) fn dialog_layout(
        &self,
        dialog: &Dialog<Choice>,
        whole_screen: bool,
    ) -> DialogLayout {
        let area = if whole_screen {
            Rect {
                x: 0,
                y: 0,
                width: WIDTH as i32,
                height: HEIGHT as i32,
            }
        } else {
            self.layout.view_area()
        };
        dialog.layout(&self.font, area, self.layout.cell_w, self.layout.cell_h)
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

    /// Ends any drag in progress, closing its edit in `sprites`. A drag
    /// must not outlive the view or the visit it started in.
    pub fn end_stroke(&mut self, sprites: Option<&mut SpriteDocument>) {
        self.sprites.end_stroke(sprites);
        self.code.end_drag();
    }

    fn switch_to(&mut self, tab: Tab, sprites: Option<&mut SpriteDocument>) {
        if tab != self.tab {
            self.end_stroke(sprites);
        }
        self.tab = tab;
    }

    /// Draws the tab bar and the selected view for `cartridge`, whose
    /// sprites the editor shows from `sprites` and whose scripts the Code
    /// view shows from `code`, with what is under `pointer` highlighted.
    /// The pointer itself is not drawn. See [`draw_cursor`].
    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is a separate source the frame is drawn from"
    )]
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        ui: &SystemUi,
        cartridge: &LoadedCartridge,
        sprites: Result<SpriteSource<'_>, &str>,
        code: Option<&CodeSession>,
        pointer: Option<Point>,
    ) {
        let layout = self.layout;
        clear(framebuffer, BACKGROUND);
        fill_rect(framebuffer, 0, 0, WIDTH as i32, layout.bar_height(), BAR);

        // Nothing under an open dialog reacts to the pointer, and only the
        // sprite editor's dialog does while one is open.
        let pointer = pointer.filter(|_| self.modal.is_none());
        let chrome_pointer = pointer
            .filter(|_| !self.sprites.has_dialog() && !self.code.has_overlay())
            .filter(|_| !self.docs.has_overlay() || !self.docs_open);
        let hovered = chrome_pointer.and_then(|at| layout.tab_at(at));
        let button = self.docs_button();
        let over_button = chrome_pointer.is_some_and(|at| button.contains(at));
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
            Some(tab) => (self.tab_labels.get(tab), PALE),
            None if over_button => (self.tab_labels.docs_hint(), PALE),
            None => (self.tab_labels.get(self.tab), DIM),
        };
        draw_text(
            framebuffer,
            font,
            hint,
            layout.hint_x(),
            layout.text_y(),
            color,
        );

        self.docs_button_face(framebuffer, font, over_button);

        let name = &cartridge.manifest().cartridge.name;
        let cells = layout.name_cells(&self.tab_labels, font);
        if let Some(name) = fit_name(font, name, cells) {
            let x = layout.name_right(&self.tab_labels, font) - text_width(font, &name);
            draw_text(framebuffer, font, &name, x, layout.text_y(), PALE);
        }

        let notice = self
            .notice
            .as_ref()
            .map(|notice| (notice.text.as_str(), notice.color));
        if self.docs_open {
            self.docs
                .render(framebuffer, font, docs_source(), pointer, notice);
        } else {
            match self.tab {
                Tab::Sprites => {
                    self.sprites
                        .render(framebuffer, font, ui, sprites, pointer, notice)
                }
                Tab::Code => self
                    .code
                    .render(framebuffer, font, ui, code, pointer, notice),
                _ => self.render_placeholder(framebuffer, font),
            }
        }
        if let Some(modal) = &self.modal {
            let dialog_layout =
                modal
                    .dialog
                    .layout(font, layout.view_area(), layout.cell_w, layout.cell_h);
            modal.dialog.render(framebuffer, font, &dialog_layout);
        }
    }

    /// The Docs button: a plate like the active tab's while the browser is
    /// open, and lit while the pointer is over it.
    fn docs_button_face(&self, framebuffer: &mut Framebuffer, font: &BitmapFont, hovered: bool) {
        let button = self.docs_button();
        let ink = if self.docs_open {
            fill_rect(
                framebuffer,
                button.x,
                button.y,
                button.width,
                button.height,
                BACKGROUND,
            );
            fill_rect(framebuffer, button.x, button.y, button.width, 1, ACCENT);
            ACCENT
        } else if hovered {
            PALE
        } else {
            DIM
        };
        draw_text(
            framebuffer,
            font,
            self.tab_labels.docs(),
            button.x + DOCS_PAD,
            self.layout.text_y(),
            ink,
        );
    }

    /// The selected view's stand-in until its editor exists.
    fn render_placeholder(&self, framebuffer: &mut Framebuffer, font: &BitmapFont) {
        let Layout { cell_w, cell_h } = self.layout;
        let x = cell_w;
        let mut y = self.layout.bar_height() + cell_h;
        draw_text_scaled(
            framebuffer,
            font,
            self.tab_labels.get(self.tab),
            x,
            y,
            2,
            ACCENT,
        );
        y += 3 * cell_h;
        for (text, color) in [
            (self.tab.purpose(), TEXT),
            (ui_text::get("workspace-not-available-yet"), DIM),
            (String::new(), DIM),
            (ui_text::get("workspace-esc-shell-ctrl-tab-switch-tab"), DIM),
        ] {
            draw_text(framebuffer, font, &text, x, y, color);
            y += cell_h;
        }
    }
}

/// Where the browser starts when it is opened apart from the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocsStart {
    Home,
    Page(DocId),
    /// SEARCH DOCS with this query already typed.
    Search(String),
}

/// What the browser shown apart from the workspace wants after an event.
#[derive(Debug, PartialEq, Eq)]
pub enum DocsApart {
    Stay,
    Close,
    /// Work for the system: a link to open or the clipboard.
    Request(WorkspaceRequest),
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
