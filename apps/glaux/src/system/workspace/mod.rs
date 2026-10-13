//! The editor workspace: a tab bar over one view per cartridge resource.
//!
//! The workspace works on the loaded cartridge without starting it. The
//! system lends it the cartridge and the documents the editors change for
//! each tick and frame; the workspace keeps only its own UI state, so the
//! selected tab and each editor's view survive tab switches and trips to the
//! shell. File work the editors ask for goes back to the system as a
//! [`WorkspaceRequest`], and its outcome comes back as a brief notice or a
//! modal [`Dialog`].

mod code;
mod dialog;
mod sprites;

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::{BitmapFont, FontMetrics};
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use glaux_machine::ink_image::INK_SLOTS;
use glaux_machine::machine::TICK_RATE;
use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, OBJ_PALETTE_COLORS, ObjPalettes};
use glaux_machine::palette::Palette;
use glaux_machine::system_rom::{Cursor, SystemIcon, SystemUi};
use glaux_video::draw::{clear, draw_ink_image, draw_text, draw_text_scaled, fill_rect};
use glaux_video::preview::ObjPreviewPalette;

use super::code_document::CodeDocumentError;
use super::code_session::CodeSession;
use super::editor_settings::EditorSettings;
use super::sprite_document::{
    Conflict, ConflictKind, DiskAsset, Reopened, SaveError, SaveFailure, Saved, SavedFile,
    SpriteDocument, file_list,
};
use super::video::CartridgeVideoDefaults;
use super::{ACCENT, BACKGROUND, DIM, ERROR, OK, PALE, TEXT};
use crate::cartridge::source::{ScriptListing, SourcePath};
use crate::cartridge::{CartridgeError, LoadedCartridge, VideoAsset};
use crate::cartridge_fs::FsError;
use crate::rom::ICON_SIZE;
use crate::sprite_import::SheetSource;
use code::{CodeEditor, CodeRequest};
use dialog::Dialog;
#[cfg(test)]
pub use sprites::ImportPreview;
#[cfg(not(test))]
use sprites::ImportPreview;
pub use sprites::SpriteSource;
use sprites::{SpriteEditor, SpriteRequest};

/// Entries of the workspace's frame palette that keep the system palette's
/// colors. Every UI color index is below it; the cartridge's object colors
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
    /// List the cartridge's scripts for OPEN FILE; answer with
    /// [`Workspace::scripts_listed`].
    ListScripts,
    /// Open this script in a tab, or show its tab if it has one; answer
    /// with [`Workspace::script_opened`].
    OpenScript(SourcePath),
    /// Keep the Code view's settings for the next launch.
    SaveEditorSettings(EditorSettings),
}

/// What a workspace dialog's buttons do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Overwrite,
    Reload,
    Cancel,
    /// Close a message that offers no decision.
    Dismiss,
}

/// The open dialog and what its choice acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Modal {
    dialog: Dialog<Choice>,
    /// The files a conflict dialog showed, as it showed them: what
    /// Overwrite may replace and Reload reads again.
    conflict: Vec<DiskAsset>,
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
    fn new(metrics: FontMetrics) -> Self {
        Self {
            cell_w: i32::from(metrics.cell_width()),
            cell_h: i32::from(metrics.cell_height()),
        }
    }

    /// Height of the tab bar: an icon, or one text row with a pixel above
    /// and below, whichever is taller.
    fn bar_height(self) -> i32 {
        (self.cell_h + 2).max(ICON_SIZE as i32)
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
fn write_advice(err: &CartridgeError) -> String {
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

/// The line telling which files a save that stopped partway did write.
fn saved_line(written: &[SavedFile]) -> Option<String> {
    (!written.is_empty()).then(|| {
        format!(
            "Saved {} before stopping.",
            file_list(written.iter().map(|file| file.asset))
        )
    })
}

/// Draws `cursor` with its hotspot on `pointer`, if any. Drawn over the
/// finished frame, it covers everything below it.
pub fn draw_cursor(framebuffer: &mut Framebuffer, cursor: &Cursor, pointer: Option<Point>) {
    let Some(at) = pointer else {
        return;
    };
    let (hot_x, hot_y) = cursor.hotspot;
    draw_ink_image(
        framebuffer,
        &cursor.image,
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
                code::Metrics {
                    cell_w: layout.cell_w,
                    cell_h: layout.cell_h,
                    top: layout.bar_height(),
                },
                font.clone(),
                EditorSettings::default(),
            ),
            sprites: SpriteEditor::new(sprites::Metrics {
                cell_w: layout.cell_w,
                cell_h: layout.cell_h,
                top: layout.bar_height(),
            }),
            modal: None,
            notice: None,
            system_palette,
        }
    }

    /// Prepares to show `document` in the workspace's frame palette. It
    /// holds until the document's palettes change; see
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
    /// While a dialog is open it takes every event, Escape included; so
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
            return match choice {
                Choice::Overwrite => Some(WorkspaceRequest::Overwrite(modal.conflict)),
                Choice::Reload => Some(WorkspaceRequest::Reload(
                    modal.conflict.iter().map(|disk| disk.asset).collect(),
                )),
                Choice::Cancel | Choice::Dismiss => None,
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
        code: Option<&mut CodeSession>,
    ) -> Option<WorkspaceRequest> {
        match self.code.handle_input(event, code)? {
            CodeRequest::ListScripts => Some(WorkspaceRequest::ListScripts),
            CodeRequest::Open(path) => Some(WorkspaceRequest::OpenScript(path)),
            CodeRequest::SaveSettings(settings) => {
                Some(WorkspaceRequest::SaveEditorSettings(settings))
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

    /// Advances time-limited UI state by one tick.
    pub fn tick(&mut self) {
        self.sprites.tick();
        self.code.tick();
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

    /// Asks whether to overwrite the files in `conflicts`, discard the
    /// changes to them, or leave both as they are. `saved`, if any, tells
    /// what a save wrote before it found the conflict. Cancel is focused so
    /// a stray Enter loses nothing.
    fn open_conflict(&mut self, conflicts: Vec<Conflict>, saved: Option<String>) {
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
        self.modal = Some(Modal {
            dialog,
            conflict: conflicts
                .into_iter()
                .map(|conflict| conflict.disk)
                .collect(),
        });
    }

    fn open_error(&mut self, title: &str, mut lines: Vec<String>) {
        lines.push("Your edits are still in memory.".to_owned());
        self.open_message(title, lines);
    }

    /// Shows `lines` under `title` until dismissed.
    fn open_message(&mut self, title: &str, lines: Vec<String>) {
        let dialog = Dialog::new(
            title,
            lines,
            vec![("OK", Choice::Dismiss)],
            0,
            Some(Choice::Dismiss),
        );
        self.modal = Some(Modal {
            dialog,
            conflict: Vec::new(),
        });
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
    /// The pointer itself is not drawn; see [`draw_cursor`].
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
