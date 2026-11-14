//! The Map view, where the cartridge's maps are chosen, created, renamed and
//! deleted without leaving the workspace.
//!
//! The view keeps UI state only. The system lends it the open
//! [`MapDocument`] and answers its [`MapRequest`]s, as for the other
//! editors. The selector in the options bar opens a menu that lists the
//! cartridge's maps and ends with the map operations.

mod actions;
mod attributes;
mod canvas;
mod compose;
mod drawer;
mod editing;
#[cfg(test)]
mod fixture;
mod form;
mod frame;
mod layout;
mod pattern;
mod picker;
mod render;
#[cfg(test)]
mod tests;
mod tools;
mod view;

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::tilemap::{BgBank, BgLayer};

use self::editing::{Edit, LayerAction, TileAction};
use self::form::{CreateForm, FormInput};
use self::layout::{Action, Hit, Layout};
use super::Rect;
use super::code::prompt::{Prompt, PromptInput};
use super::context_menu::{ContextMenu, ItemStyle, MenuEntry, MenuOutcome};
use super::dialog::{Dialog, text_cells};
use super::request::MapRequest;
use super::text_field::TextField;
use crate::cartridge::map::{MapId, MapSummary};
use crate::system::map_document::MapDocument;
use crate::ui_text;
use glaux_i18n::Arg;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};

pub(super) mod answers;

/// Longest map name the rename window takes.
const MAX_NAME: usize = 40;

/// The cell size and top of the Map view, below the tab bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Metrics {
    pub(super) cell_w: i32,
    pub(super) cell_h: i32,
    pub(super) top: i32,
}

impl Metrics {
    fn area(self) -> Rect {
        Rect {
            x: 0,
            y: self.top,
            width: WIDTH as i32,
            height: HEIGHT as i32 - self.top,
        }
    }
}

#[cfg(test)]
impl MapEditor {
    pub(in crate::system::workspace) fn button_rect(&self, label: &str) -> Rect {
        let position = Action::ALL
            .into_iter()
            .position(|action| ui_text::get(action.message_id()) == label)
            .expect("an operation has this label");
        self.row_rect(self.maps().len() + 1 + position)
    }

    pub(in crate::system::workspace) fn selector_rect(&self) -> Rect {
        self.frame().selector
    }

    pub(in crate::system::workspace) fn canvas_rect(&self) -> Rect {
        self.frame().canvas
    }

    pub(in crate::system::workspace) fn edit_button_rect(&self) -> Rect {
        self.frame().edit_button.expect("the drawer is expanded")
    }

    pub(in crate::system::workspace) fn drawer_rect(&self) -> Rect {
        self.frame().drawer
    }

    pub(in crate::system::workspace) fn row_rect(&self, slot: usize) -> Rect {
        let Some(Overlay::Maps(menu)) = &self.overlay else {
            panic!("the map menu is open");
        };
        let layout = menu.layout(&self.font, self.metrics.area(), self.metrics.cell_w, self.metrics.cell_h);
        layout.rows[slot - layout.first]
    }

    /// Where the CREATE MAP button of the empty state is drawn.
    pub(in crate::system::workspace) fn create_map_rect(&self) -> Rect {
        self.layout()
            .create_map(text_cells(&ui_text::get("maps-create-map")))
    }

    /// The line under the create window's fields, while it is open.
    pub(in crate::system::workspace) fn form_message(&self) -> Option<String> {
        match &self.overlay {
            Some(Overlay::Create(form)) => Some(form.message().0),
            _ => None,
        }
    }

    pub(in crate::system::workspace) fn list_is_open(&self) -> bool {
        matches!(self.overlay, Some(Overlay::Maps(_)))
    }
}

/// What the view asks of the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MapOut {
    Request(MapRequest),
    /// Open this map. The open map has unsaved edits, so the user is asked
    /// first.
    ConfirmSwitch(MapId),
    Notice(String),
    Copy(String),
    Paste,
    EditTile {
        tile: u16,
        bank: BgBank,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Confirm {
    Replace { from: u16, to: u16 },
    Clear(BgLayer),
}

/// A window over the view.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Overlay {
    Create(CreateForm),
    Rename(Prompt),
    RenameLayer(BgLayer, Prompt),
    LayerMenu(BgLayer, ContextMenu<LayerAction>),
    /// The map selector's menu. Its actions index the maps, then the
    /// operations that follow them.
    Maps(ContextMenu<usize>),
    TileMenu(u16, ContextMenu<TileAction>),
    Confirm(Confirm, Dialog<bool>),
}

/// The Map view's UI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MapEditor {
    metrics: Metrics,
    /// The cartridge's maps as last listed, or why they could not be.
    summaries: Option<Result<Vec<MapSummary>, String>>,
    /// Whether the maps should be listed again the next time the view shows.
    stale: bool,
    /// The map the user last had open, which opens again when none is.
    remembered: Option<MapId>,
    overlay: Option<Overlay>,
    font: BitmapFont,
    edit: Edit,
}

impl MapEditor {
    pub(super) fn new(metrics: Metrics, font: BitmapFont) -> Self {
        Self {
            metrics,
            summaries: None,
            stale: true,
            remembered: None,
            overlay: None,
            font,
            edit: Edit::default(),
        }
    }

    fn layout(&self) -> Layout {
        Layout::new(self.metrics)
    }

    fn maps(&self) -> &[MapSummary] {
        match &self.summaries {
            Some(Ok(maps)) => maps,
            _ => &[],
        }
    }

    /// Asks for the maps to be listed again.
    pub(super) fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// Returns whether the maps should be listed now, and clears the request.
    pub(super) fn take_stale(&mut self) -> bool {
        std::mem::take(&mut self.stale)
    }

    pub(super) fn listed(&mut self, listing: Result<Vec<MapSummary>, String>) {
        self.summaries = Some(listing);
    }

    /// Picks the map to open when none is open. This is the remembered map if
    /// it can still be opened, and otherwise the first map that can.
    pub(super) fn to_open(&self) -> Option<MapId> {
        let openable = |summary: &&MapSummary| summary.meta.is_ok();
        self.remembered
            .as_ref()
            .and_then(|id| self.maps().iter().find(|summary| &summary.id == id))
            .filter(|summary| summary.meta.is_ok())
            .or_else(|| self.maps().iter().find(openable))
            .map(|summary| summary.id.clone())
    }

    pub(super) fn opened(&mut self, id: &MapId) {
        // A map opened again after a reload keeps its view.
        if self.remembered.as_ref() != Some(id) {
            self.edit.reset_for_new_map();
        }
        self.remembered = Some(id.clone());
        self.close_list();
    }

    /// Forgets that `id` was the map the user last had open.
    pub(super) fn deleted(&mut self, id: &MapId) {
        if self.remembered.as_ref() == Some(id) {
            self.remembered = None;
        }
    }

    /// Forgets everything of the cartridge that was replaced or unloaded.
    pub(super) fn forget(&mut self) {
        *self = Self::new(self.metrics, self.font.clone());
    }

    /// Whether a window is over the view. The selector's menu is not one, so
    /// the tab bar and the shortcuts keep working while it is open.
    pub(super) fn has_overlay(&self) -> bool {
        self.overlay
            .as_ref()
            .is_some_and(|overlay| !matches!(overlay, Overlay::Maps(_)))
    }

    /// Whether Escape closes something in the view.
    pub(super) fn claims_break(&self) -> bool {
        self.overlay.is_some()
            || self.edit.picker.is_some()
            || self.edit.attrs.is_some()
            || self.has_selection()
    }

    /// Closes the windows and the list, as when the view is left.
    pub(super) fn close_overlays(&mut self) {
        self.overlay = None;
        self.close_picker();
        self.edit.attrs = None;
        // A drag must not outlive the visit it started in. Its edit, if
        // open, closes with the next undo, redo or save.
        self.edit.stroke = None;
    }

    pub(super) fn close_create(&mut self) {
        if matches!(self.overlay, Some(Overlay::Create(_))) {
            self.overlay = None;
        }
    }

    /// Shows why creating the map failed, in the window.
    pub(super) fn create_failed(&mut self, message: String) {
        if let Some(Overlay::Create(form)) = &mut self.overlay {
            form.set_failure(message);
        }
    }

    pub(super) fn paste(&mut self, text: &str) {
        match &mut self.overlay {
            Some(Overlay::Create(form)) => form.paste(text),
            Some(Overlay::Rename(prompt) | Overlay::RenameLayer(_, prompt)) => prompt.paste(text),
            Some(
                Overlay::LayerMenu(..)
                | Overlay::Maps(_)
                | Overlay::TileMenu(..)
                | Overlay::Confirm(..),
            )
            | None => {}
        }
    }

    fn open_create(&mut self) {
        let existing = self
            .maps()
            .iter()
            .map(|summary| summary.id.clone())
            .collect();
        self.overlay = Some(Overlay::Create(CreateForm::new(existing)));
    }

    fn open_rename(&mut self, document: &MapDocument) {
        let label = ui_text::get_with(
            "maps-rename-label",
            &[("id", Arg::Text(&document.id().to_string()))],
        );
        let mut field = TextField::any(MAX_NAME);
        field.set_text(&document.meta().name);
        self.overlay = Some(Overlay::Rename(Prompt::new(
            &ui_text::get("maps-rename-title"),
            label,
            "",
            field,
        )));
    }

    /// Applies `event`, letting undo and redo change `document`.
    pub(super) fn handle_input(
        &mut self,
        event: InputEvent,
        mut document: Option<&mut MapDocument>,
    ) -> Option<MapOut> {
        if self.overlay.is_some() {
            return self.overlay_input(event, document);
        }
        let layout = self.layout();
        if let Some(document) = document.as_deref_mut()
            && let Some(out) = self.editor_input(event, document)
        {
            return out;
        }
        let create_cells = self.create_label_cells(document.as_deref());
        match event {
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                if self.frame().selector.contains(at) {
                    self.open_list(document.as_deref());
                } else if layout.hit(at, create_cells) == Hit::CreateMap {
                    self.open_create();
                }
            }
            InputEvent::Enter | InputEvent::Down { .. } => self.open_list(document.as_deref()),
            InputEvent::NewFile => self.open_create(),
            InputEvent::Save => return self.save(document.as_deref()),
            InputEvent::Undo => {
                if let Some(document) = document {
                    document.undo();
                }
            }
            InputEvent::Redo => {
                if let Some(document) = document {
                    document.redo();
                }
            }
            _ => {}
        }
        None
    }

    /// Cells of the CREATE MAP label while the view has no map to show.
    fn create_label_cells(&self, document: Option<&MapDocument>) -> Option<i32> {
        (document.is_none() && self.maps().is_empty())
            .then(|| text_cells(&ui_text::get("maps-create-map")))
    }

    fn close_list(&mut self) {
        if matches!(self.overlay, Some(Overlay::Maps(_))) {
            self.overlay = None;
        }
    }

    fn open_list(&mut self, document: Option<&MapDocument>) {
        let frame = self.frame();
        let anchor = Point::new(
            (frame.selector.x - 2).max(0) as u16,
            frame.options.bottom() as u16,
        );
        let open = document.map(MapDocument::id);
        let maps = self.maps();
        let mut entries: Vec<_> = maps
            .iter()
            .enumerate()
            .map(|(index, summary)| {
                let current = open == Some(&summary.id);
                let (label, style) = row_label(summary, current, document);
                MenuEntry::item(&label, None, index).styled(style)
            })
            .collect();
        if !maps.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        for (position, action) in Action::ALL.into_iter().enumerate() {
            let enabled = action == Action::New || document.is_some();
            let label = ui_text::get(action.message_id());
            entries.push(MenuEntry::when(enabled, &label, None, maps.len() + position));
        }
        let current = open
            .and_then(|id| maps.iter().position(|summary| &summary.id == id))
            .unwrap_or(0);
        let room = frame.status.y - i32::from(anchor.y);
        self.overlay = Some(Overlay::Maps(
            ContextMenu::new(entries, anchor)
                .limited_to(room)
                .focused_on(current),
        ));
    }

    fn act(&mut self, action: Action, document: Option<&MapDocument>) -> Option<MapOut> {
        match action {
            Action::New => {
                self.open_create();
                None
            }
            Action::Rename => {
                self.open_rename(document?);
                None
            }
            Action::Delete => Some(MapOut::Request(MapRequest::PrepareDelete(
                document?.id().clone(),
            ))),
        }
    }

    fn save(&self, document: Option<&MapDocument>) -> Option<MapOut> {
        let document = document?;
        Some(if document.is_modified() {
            MapOut::Request(MapRequest::Save)
        } else {
            MapOut::Notice(ui_text::get("maps-nothing-to-save"))
        })
    }

    fn choose(&mut self, index: usize, document: Option<&MapDocument>) -> Option<MapOut> {
        let Some(summary) = self.maps().get(index) else {
            let action = *Action::ALL.get(index - self.maps().len())?;
            if action != Action::New && document.is_none() {
                return None;
            }
            self.close_list();
            return self.act(action, document);
        };
        let id = summary.id.clone();
        match &summary.meta {
            // A map that cannot be opened says why and keeps the list open.
            Err(err) => Some(MapOut::Notice(crate::system::ui_errors::map::map(err))),
            Ok(_) => {
                self.close_list();
                match document {
                    Some(open) if open.id() == &id => None,
                    Some(open) if open.is_modified() => Some(MapOut::ConfirmSwitch(id)),
                    _ => Some(MapOut::Request(MapRequest::Open(id))),
                }
            }
        }
    }

    fn overlay_input(
        &mut self,
        event: InputEvent,
        document: Option<&mut MapDocument>,
    ) -> Option<MapOut> {
        let area = self.metrics.area();
        let (cell_w, cell_h) = (self.metrics.cell_w, self.metrics.cell_h);
        match self.overlay.as_mut()? {
            Overlay::Maps(menu) => {
                let layout = menu.layout(&self.font, area, cell_w, cell_h);
                if event == InputEvent::Save {
                    self.overlay = None;
                    return self.save(document.as_deref());
                }
                match menu.handle_input(event, &layout) {
                    MenuOutcome::Stay => None,
                    MenuOutcome::Close | MenuOutcome::Outside => {
                        self.overlay = None;
                        None
                    }
                    MenuOutcome::Run(index) => self.choose(index, document.as_deref()),
                }
            }
            Overlay::TileMenu(tile, menu) => {
                let tile = *tile;
                let layout = menu.layout(&self.font, area, cell_w, cell_h);
                match menu.handle_input(event, &layout) {
                    MenuOutcome::Stay => None,
                    MenuOutcome::Close | MenuOutcome::Outside => {
                        self.overlay = None;
                        None
                    }
                    MenuOutcome::Run(action) => {
                        self.overlay = None;
                        self.run_tile_action(tile, action, document.as_deref())
                    }
                }
            }
            Overlay::Confirm(confirm, dialog) => {
                let confirm = *confirm;
                let layout = dialog.layout(&self.font, area, cell_w, cell_h);
                match dialog.handle_input(event, &layout) {
                    None => None,
                    Some(answer) => {
                        self.overlay = None;
                        match (answer, document) {
                            (true, Some(document)) => self.run_confirm(confirm, document),
                            _ => None,
                        }
                    }
                }
            }
            Overlay::LayerMenu(layer, menu) => {
                let layer = *layer;
                let layout = menu.layout(&self.font, area, cell_w, cell_h);
                match menu.handle_input(event, &layout) {
                    MenuOutcome::Stay => {}
                    MenuOutcome::Close | MenuOutcome::Outside => self.overlay = None,
                    MenuOutcome::Run(action) => {
                        self.overlay = None;
                        self.run_layer_action(layer, action, document.as_deref());
                    }
                }
                None
            }
            Overlay::RenameLayer(layer, prompt) => {
                let layer = *layer;
                let layout = prompt.layout(area, cell_w, cell_h);
                match prompt.handle_input(event, &layout) {
                    PromptInput::None | PromptInput::Edited => None,
                    PromptInput::Submit => {
                        let name = prompt.text().trim().to_owned();
                        self.overlay = None;
                        if let Some(document) = document {
                            document.set_layer_name(layer, (!name.is_empty()).then_some(&name));
                        }
                        None
                    }
                    PromptInput::Close => {
                        self.overlay = None;
                        None
                    }
                    PromptInput::Copy(text) => Some(MapOut::Copy(text)),
                    PromptInput::Paste => Some(MapOut::Paste),
                }
            }
            Overlay::Create(form) => {
                let layout = form.layout(area, cell_w, cell_h);
                match form.handle_input(event, &layout) {
                    FormInput::None => None,
                    FormInput::Create(id, meta) => {
                        Some(MapOut::Request(MapRequest::Create { id, meta }))
                    }
                    FormInput::Close => {
                        self.overlay = None;
                        None
                    }
                    FormInput::Copy(text) => Some(MapOut::Copy(text)),
                    FormInput::Paste => Some(MapOut::Paste),
                }
            }
            Overlay::Rename(prompt) => {
                let layout = prompt.layout(area, cell_w, cell_h);
                match prompt.handle_input(event, &layout) {
                    PromptInput::None | PromptInput::Edited => None,
                    PromptInput::Submit => {
                        let name = prompt.text().trim().to_owned();
                        if name.is_empty() {
                            return None;
                        }
                        self.overlay = None;
                        Some(MapOut::Request(MapRequest::Rename { name }))
                    }
                    PromptInput::Close => {
                        self.overlay = None;
                        None
                    }
                    PromptInput::Copy(text) => Some(MapOut::Copy(text)),
                    PromptInput::Paste => Some(MapOut::Paste),
                }
            }
        }
    }
}

fn row_label(
    summary: &MapSummary,
    current: bool,
    document: Option<&MapDocument>,
) -> (String, ItemStyle) {
    match &summary.meta {
        Ok(meta) => {
            let unsaved = current && document.is_some_and(MapDocument::is_modified);
            let label = format!(
                "{}{}  {}  {}x{}",
                meta.name,
                if unsaved { "*" } else { "" },
                summary.id,
                meta.width,
                meta.height
            );
            (label, if current { ItemStyle::Current } else { ItemStyle::Plain })
        }
        Err(_) => (
            format!("{}  ({})", summary.id, ui_text::get("maps-broken")),
            ItemStyle::Danger,
        ),
    }
}
