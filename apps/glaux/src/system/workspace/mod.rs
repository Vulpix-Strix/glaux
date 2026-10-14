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
mod sprites;
mod text_field;

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
use super::editor_settings::EditorSettings;
use super::sprite_document::{
    Conflict, ConflictKind, DiskAsset, Reopened, SaveError, SaveFailure, Saved, SavedFile,
    SpriteDocument, file_list,
};
use super::video::CartridgeVideoDefaults;
use super::{ACCENT, BACKGROUND, DIM, ERROR, OK, PALE, TEXT};
use crate::cartridge::source::{ScriptListing, SourceEditError, SourcePath};
use crate::cartridge::{CartridgeError, Language, LoadedCartridge, VideoAsset};
use crate::cartridge_fs::FsError;
use crate::rom::ICON_SIZE;
use crate::sprite_import::SheetSource;
use code::{CodeEditor, CodeRequest};
pub(super) use dialog::{Dialog, DialogLayout};
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

    pub fn label(self) -> &'static str {
        match self {
            Self::Code => "Code",
            Self::Sprites => "Sprites",
            Self::Map => "Map",
            Self::Sfx => "SFX",
            Self::Music => "Music",
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
    fn purpose(self) -> &'static str {
        match self {
            Self::Code => "Edit the cartridge's Luau and Rhai scripts.",
            Self::Sprites => "Draw object tiles and their palettes.",
            Self::Map => "Lay out tiles into maps.",
            Self::Sfx => "Shape sound effects for the PSG.",
            Self::Music => "Arrange sound effects into songs.",
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

    /// Cells available to the cartridge name, right of the longest hint.
    fn name_cells(self) -> usize {
        let longest = Tab::ALL
            .iter()
            .map(|tab| i32::try_from(tab.label().chars().count()).unwrap_or(0) * self.cell_w)
            .max()
            .unwrap_or(0);
        let start = self.hint_x() + longest + NAME_GAP;
        let space = WIDTH as i32 - NAME_PAD - start;
        usize::try_from(space / self.cell_w.max(1)).unwrap_or(0)
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
        CartridgeError::AssetReadOnly(asset) => {
            format!("{asset} is read-only. Make it writable, then save again.")
        }
        CartridgeError::AssetUnwritable(asset) => format!(
            "Glaux could not write {asset}. Check the cartridge folder's permissions and free space, then save again."
        ),
        CartridgeError::AssetUnreadable(asset) => format!(
            "Glaux could not read {asset} to check it for outside changes. Check its permissions, then save again."
        ),
        CartridgeError::AssetNotAFile(asset) => {
            format!("{asset} is not a regular file. Move it aside, then save again.")
        }
        other => other.to_string(),
    }
}

/// What to tell the user when saving the script at `path` failed.
pub(super) fn source_write_advice(path: &SourcePath, err: &FsError) -> String {
    match err {
        FsError::ReadOnly(_) => format!("{path} is read-only. Make it writable, then save again."),
        FsError::NotAFile(_) => {
            format!("{path} is not a regular file. Move it aside, then save again.")
        }
        FsError::Unreadable(_) => format!(
            "Glaux could not read {path} to check it for outside changes. Check its permissions, then save again."
        ),
        FsError::NotFound(_) | FsError::NotADirectory(_) => {
            format!("The folder holding {path} is gone. Recreate it, then save again.")
        }
        FsError::Unwritable(_) | FsError::AlreadyExists(_) => format!(
            "Glaux could not write {path}. Check the cartridge folder's permissions and free space, then save again."
        ),
        FsError::Unavailable => "Cartridge storage is unavailable.".to_owned(),
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
    (!names.is_empty()).then(|| format!("Saved {} before stopping.", names.join(", ")))
}

/// `names` as one phrase, as in `a.luau, b.luau and graphics`.
fn phrase(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Asks whether to save the unsaved edits to `names` before `what` goes
/// ahead, as in "runs load pong". Cancel is focused so a stray Enter
/// loses nothing.
pub(super) fn unsaved_dialog(names: &[String], what: &str) -> Dialog<Choice> {
    let (have, them) = match names {
        [_] => ("has", "it"),
        _ => ("have", "them"),
    };
    Dialog::new(
        "UNSAVED CHANGES",
        vec![
            format!("{} {have} unsaved changes.", phrase(names)),
            format!(
                "SAVE ALL saves {them}, then {what}. DISCARD throws {them} away and {what} anyway."
            ),
        ],
        vec![
            ("SAVE ALL", Choice::Save),
            ("DISCARD", Choice::Discard),
            ("CANCEL", Choice::Cancel),
        ],
        2,
        Some(Choice::Cancel),
    )
}

/// Asks whether to save the unsaved edits to the script at `path` before
/// closing its tab. Cancel is focused so a stray Enter loses nothing.
fn close_dialog(path: &SourcePath) -> Dialog<Choice> {
    Dialog::new(
        "UNSAVED CHANGES",
        vec![
            format!("{path} has unsaved changes."),
            "SAVE writes them to the file, then closes it. DISCARD closes it without saving."
                .to_owned(),
        ],
        vec![
            ("SAVE", Choice::Save),
            ("DISCARD", Choice::Discard),
            ("CANCEL", Choice::Cancel),
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
    let mut lines = vec![format!("Delete {path} from the cartridge?")];
    match &seen {
        None => lines.push(format!(
            "{path} was already deleted outside Glaux. DELETE only closes its tab."
        )),
        Some(_) => lines.push("The file cannot be recovered.".to_owned()),
    }
    if external {
        lines.push("It changed outside Glaux; those changes will be lost.".to_owned());
    }
    if modified {
        lines.push("Its unsaved changes will be lost.".to_owned());
    }
    let dialog = Dialog::new(
        "DELETE SCRIPT",
        lines,
        vec![("DELETE", Choice::Delete), ("CANCEL", Choice::Cancel)],
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
        SourceEditError::Fs(FsError::ReadOnly(_)) => {
            format!("{path} is read-only. Make it writable, then try again.")
        }
        SourceEditError::Fs(FsError::AlreadyExists(existing)) => {
            format!("{} already exists.", existing.trim_start_matches('/'))
        }
        SourceEditError::Fs(FsError::Unwritable(_)) => format!(
            "Glaux could not change {path}. Check the cartridge folder's permissions, and that it is a regular file."
        ),
        err => err.to_string(),
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
            format!("{path} changed outside Glaux."),
            "OVERWRITE DISK replaces the file with your edits.",
        ),
        SourceConflictKind::Deleted => (
            format!("{path} was deleted outside Glaux."),
            "OVERWRITE DISK writes your edits to it again.",
        ),
    };
    let mut lines = vec![what];
    lines.extend(saved);
    lines.push("Your unsaved edits are still in memory.".to_owned());
    lines.push(replaces.to_owned());
    let dialog = Dialog::new(
        "FILE CONFLICT",
        lines,
        vec![
            ("OVERWRITE DISK", Choice::Overwrite),
            ("CANCEL", Choice::Cancel),
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
                ConflictKind::Modified => format!("{asset} changed outside Glaux."),
                ConflictKind::Deleted => format!("{asset} was deleted outside Glaux."),
                ConflictKind::Created => format!("A new {asset} appeared outside Glaux."),
            }
        })
        .collect();
    lines.extend(saved);
    let (them, files) = match conflicts.as_slice() {
        [_] => ("it", "the file"),
        _ => ("them", "these files"),
    };
    lines.push("Your unsaved edits are still in memory.".to_owned());
    lines.push(format!(
        "OVERWRITE DISK replaces {files} with your edits. DISCARD & RELOAD throws away your unsaved edits to {them} and the undo steps involving {them}."
    ));
    let dialog = Dialog::new(
        "FILE CONFLICT",
        lines,
        vec![
            ("OVERWRITE DISK", Choice::Overwrite),
            ("DISCARD & RELOAD", Choice::Reload),
            ("CANCEL", Choice::Cancel),
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
        vec![("OK", Choice::Dismiss)],
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
    ) -> Result<SpriteSource<'a>, &'a str> {
        let video = self.0.as_deref().map_err(String::as_str)?;
        // The system only prepares a preview from a document it keeps.
        let document = document.ok_or("The cartridge's sprites are not loaded.")?;
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
    code: CodeEditor,
    sprites: SpriteEditor,
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
            code: CodeEditor::new(
                layout.code_metrics(),
                font.clone(),
                EditorSettings::default(),
            ),
            sprites: SpriteEditor::new(layout.sprite_metrics()),
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
            Err(err) => WorkspaceVideo::unavailable(err.to_string()),
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
            InputEvent::NextView => self.switch_to(self.tab.next(), sprites),
            InputEvent::PreviousView => self.switch_to(self.tab.previous(), sprites),
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } if self.layout.tab_at(at).is_some() => {
                if let Some(tab) = self.layout.tab_at(at) {
                    self.switch_to(tab, sprites);
                }
            }
            _ if self.tab == Tab::Sprites => return self.sprite_input(event, sprites),
            _ if self.tab == Tab::Code => return self.code_input(event, code),
            _ => {}
        }
        None
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
        self.has_modal() || (self.tab == Tab::Code && self.code.claims_break())
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
        } else if !self.has_modal() && self.layout.tab_at(at).is_some() {
            SystemCursor::Hand
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
                self.notify(format!("Created {path}"), PALE);
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
                    "DELETE FAILED",
                    vec![
                        format!("Glaux could not read {path} to check it for outside changes."),
                        "Nothing was deleted.".to_owned(),
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
                self.notify(format!("Deleted {path}"), PALE);
            }
            Err(err) => {
                tracing::debug!(file = %path, %err, "script not deleted");
                self.open_message(
                    "DELETE FAILED",
                    vec![edit_failure(path, &err), "Nothing was deleted.".to_owned()],
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
            _ => err.to_string(),
        });
        match &result {
            Ok(()) => {
                tracing::info!(file = %to, "script renamed");
                self.code.refresh_metrics(code);
                self.code.reveal_active(code);
                self.notify(format!("Renamed to {to}"), PALE);
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
        if self.tab == Tab::Code {
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
                self.notify(format!("Cannot open {path}: {err}"), ERROR);
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
                self.notify("Cannot open the entry script", ERROR);
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
                [file] => self.notify(format!("Saved {} ({} bytes)", file.asset, file.bytes), OK),
                files => self.notify(
                    format!("Saved {}", file_list(files.iter().map(|file| file.asset))),
                    OK,
                ),
            },
            Ok(Saved::NothingToSave) => self.notify("Nothing to save", PALE),
            Err(SaveError { written, cause }) => {
                let saved = saved_line(&written);
                match cause {
                    SaveFailure::Conflict(conflicts) => self.open_conflict(conflicts, saved),
                    SaveFailure::Cartridge(err) => {
                        let lines = saved.into_iter().chain([write_advice(&err)]).collect();
                        self.open_error("SAVE FAILED", lines);
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
                self.notify(format!("Saved {path} ({bytes} bytes)"), OK);
            }
            Ok(CodeSaved::NothingToSave(_)) => self.notify("Nothing to save", PALE),
            Err(CodeSaveError::NotOpen) => self.notify("That file is no longer open", PALE),
            Err(CodeSaveError::Conflict(conflict)) => self.open_script_conflict(id, conflict),
            Err(CodeSaveError::Failed { path, error }) => {
                tracing::debug!(file = %path, %error, "script not saved");
                self.open_error("SAVE FAILED", vec![source_write_advice(&path, &error)]);
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
                format!("Reloaded {} from disk", file_list(assets.iter().copied())),
                OK,
            ),
            Err(message) => self.open_error("RELOAD FAILED", vec![message]),
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
        self.notify("No file chosen", PALE);
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
                "CANNOT IMPORT",
                vec![format!("{err:#}"), "Nothing was changed.".to_owned()],
            ),
        }
    }

    /// Reports what reopening the workspace found on disk.
    pub fn reopened(&mut self, reopened: Result<Reopened, String>) {
        let reopened = match reopened {
            Ok(reopened) => reopened,
            Err(message) => return self.open_error("CANNOT READ FILES", vec![message]),
        };
        if reopened.history_trimmed {
            self.notify(
                format!(
                    "{} changed on disk: reloaded, undo steps dropped",
                    file_list(reopened.reloaded.iter().copied())
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
        lines.push("Your edits are still in memory.".to_owned());
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
        let hovered = pointer
            .filter(|_| !self.sprites.has_dialog() && !self.code.has_overlay())
            .and_then(|at| layout.tab_at(at));
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
            Some(tab) => (tab, PALE),
            None => (self.tab, DIM),
        };
        draw_text(
            framebuffer,
            font,
            hint.label(),
            layout.hint_x(),
            layout.text_y(),
            color,
        );

        let name = &cartridge.manifest().cartridge.name;
        if let Some(name) = fit_name(font, name, layout.name_cells()) {
            let x = WIDTH as i32 - NAME_PAD - text_width(font, &name);
            draw_text(framebuffer, font, &name, x, layout.text_y(), PALE);
        }

        let notice = self
            .notice
            .as_ref()
            .map(|notice| (notice.text.as_str(), notice.color));
        match self.tab {
            Tab::Sprites => self
                .sprites
                .render(framebuffer, font, ui, sprites, pointer, notice),
            Tab::Code => self
                .code
                .render(framebuffer, font, ui, code, pointer, notice),
            _ => self.render_placeholder(framebuffer, font),
        }
        if let Some(modal) = &self.modal {
            let dialog_layout =
                modal
                    .dialog
                    .layout(font, layout.view_area(), layout.cell_w, layout.cell_h);
            modal.dialog.render(framebuffer, font, &dialog_layout);
        }
    }

    /// The selected view's stand-in until its editor exists.
    fn render_placeholder(&self, framebuffer: &mut Framebuffer, font: &BitmapFont) {
        let Layout { cell_w, cell_h } = self.layout;
        let x = cell_w;
        let mut y = self.layout.bar_height() + cell_h;
        draw_text_scaled(framebuffer, font, self.tab.label(), x, y, 2, ACCENT);
        y += 3 * cell_h;
        for (text, color) in [
            (self.tab.purpose(), TEXT),
            ("Not available yet.", DIM),
            ("", DIM),
            ("Esc shell   Ctrl+Tab switch tab", DIM),
        ] {
            draw_text(framebuffer, font, text, x, y, color);
            y += cell_h;
        }
    }
}

#[cfg(test)]
pub(super) mod tests_support;

#[cfg(test)]
mod tests;
