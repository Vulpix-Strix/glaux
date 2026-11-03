//! The Map view, where the cartridge's maps are chosen, created, renamed and
//! deleted without leaving the workspace.
//!
//! The view keeps UI state only. The system lends it the open
//! [`MapDocument`] and answers its [`MapRequest`]s, as for the other
//! editors. The header holds a selector that lists the cartridge's maps and
//! the buttons for the map operations.

mod form;
mod layout;
mod render;
#[cfg(test)]
mod tests;

use glaux_input::{InputEvent, PointerButton};

use self::form::{CreateForm, FormInput};
use self::layout::{Action, Hit, Layout, first_visible};
use super::Rect;
use super::code::prompt::{Prompt, PromptInput};
use super::dialog::text_cells;
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
    /// Where the header's button labelled `label` is drawn.
    pub(in crate::system::workspace) fn button_rect(&self, label: &str) -> Rect {
        let action = Action::ALL
            .into_iter()
            .find(|action| ui_text::get(action.message_id()) == label)
            .expect("a header button has this label");
        self.layout().button(action)
    }

    pub(in crate::system::workspace) fn selector_rect(&self) -> Rect {
        self.layout().selector()
    }

    /// Where the list draws the row `slot` places below its top.
    pub(in crate::system::workspace) fn row_rect(&self, slot: usize) -> Rect {
        self.layout().row(slot)
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
        self.list_open
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
}

/// A window over the view.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Overlay {
    Create(CreateForm),
    Rename(Prompt),
}

/// The Map view's UI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MapEditor {
    metrics: Metrics,
    /// Width of each header button, which fits its label.
    button_w: [i32; 4],
    /// The cartridge's maps as last listed, or why they could not be.
    summaries: Option<Result<Vec<MapSummary>, String>>,
    /// Whether the maps should be listed again the next time the view shows.
    stale: bool,
    /// The map the user last had open, which opens again when none is.
    remembered: Option<MapId>,
    list_open: bool,
    highlighted: usize,
    overlay: Option<Overlay>,
}

impl MapEditor {
    pub(super) fn new(metrics: Metrics) -> Self {
        let button_w = Action::ALL
            .map(|action| (text_cells(&ui_text::get(action.message_id())) + 2) * metrics.cell_w);
        Self {
            metrics,
            button_w,
            summaries: None,
            stale: true,
            remembered: None,
            list_open: false,
            highlighted: 0,
            overlay: None,
        }
    }

    fn layout(&self) -> Layout {
        Layout::new(self.metrics, self.button_w)
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
        self.highlighted = self.highlighted.min(self.maps().len().saturating_sub(1));
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
        self.remembered = Some(id.clone());
        self.list_open = false;
    }

    /// Forgets that `id` was the map the user last had open.
    pub(super) fn deleted(&mut self, id: &MapId) {
        if self.remembered.as_ref() == Some(id) {
            self.remembered = None;
        }
    }

    /// Forgets everything of the cartridge that was replaced or unloaded.
    pub(super) fn forget(&mut self) {
        *self = Self::new(self.metrics);
    }

    pub(super) fn has_overlay(&self) -> bool {
        self.overlay.is_some()
    }

    /// Whether Escape closes something in the view.
    pub(super) fn claims_break(&self) -> bool {
        self.overlay.is_some() || self.list_open
    }

    /// Closes the windows and the list, as when the view is left.
    pub(super) fn close_overlays(&mut self) {
        self.overlay = None;
        self.list_open = false;
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
            Some(Overlay::Rename(prompt)) => prompt.paste(text),
            None => {}
        }
    }

    fn open_create(&mut self) {
        let existing = self
            .maps()
            .iter()
            .map(|summary| summary.id.clone())
            .collect();
        self.list_open = false;
        self.overlay = Some(Overlay::Create(CreateForm::new(existing)));
    }

    fn open_rename(&mut self, document: &MapDocument) {
        let label = ui_text::get_with(
            "maps-rename-label",
            &[("id", Arg::Text(&document.id().to_string()))],
        );
        let mut field = TextField::any(MAX_NAME);
        field.set_text(&document.meta().name);
        self.list_open = false;
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
        document: Option<&mut MapDocument>,
    ) -> Option<MapOut> {
        if self.overlay.is_some() {
            return self.overlay_input(event);
        }
        let layout = self.layout();
        if self.list_open {
            return self.list_input(event, &layout, document.as_deref());
        }
        let create_cells = self.create_label_cells(document.as_deref());
        match event {
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => match layout.hit(at, None, create_cells) {
                Hit::Selector => self.open_list(document.as_deref()),
                Hit::Action(action) => return self.act(action, document.as_deref()),
                Hit::CreateMap => self.open_create(),
                Hit::Row(_) | Hit::Nothing => {}
            },
            InputEvent::Enter | InputEvent::Down { .. } => self.open_list(document.as_deref()),
            InputEvent::NewFile => self.open_create(),
            InputEvent::Save => return self.act(Action::Save, document.as_deref()),
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

    fn open_list(&mut self, document: Option<&MapDocument>) {
        if self.maps().is_empty() {
            return;
        }
        self.highlighted = document
            .and_then(|document| self.maps().iter().position(|m| &m.id == document.id()))
            .unwrap_or(0);
        self.list_open = true;
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
            Action::Save => {
                let document = document?;
                Some(if document.is_modified() {
                    MapOut::Request(MapRequest::Save)
                } else {
                    MapOut::Notice(ui_text::get("maps-nothing-to-save"))
                })
            }
        }
    }

    fn list_input(
        &mut self,
        event: InputEvent,
        layout: &Layout,
        document: Option<&MapDocument>,
    ) -> Option<MapOut> {
        let count = self.maps().len();
        match event {
            InputEvent::Break => self.list_open = false,
            InputEvent::Up { .. } => self.highlighted = self.highlighted.saturating_sub(1),
            InputEvent::Down { .. } => {
                self.highlighted = (self.highlighted + 1).min(count.saturating_sub(1));
            }
            InputEvent::Enter => return self.choose(self.highlighted, document),
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                let first = first_visible(self.highlighted, layout.visible_rows());
                match layout.hit(at, Some((count, first)), None) {
                    Hit::Row(index) => return self.choose(index, document),
                    _ => self.list_open = false,
                }
            }
            _ => {}
        }
        None
    }

    /// Opens the map at `index` of the list, if it can be.
    fn choose(&mut self, index: usize, document: Option<&MapDocument>) -> Option<MapOut> {
        let summary = self.maps().get(index)?;
        let id = summary.id.clone();
        match &summary.meta {
            // A map that cannot be opened says why and keeps the list open.
            Err(err) => Some(MapOut::Notice(crate::system::ui_errors::map::map(err))),
            Ok(_) => {
                self.list_open = false;
                match document {
                    Some(open) if open.id() == &id => None,
                    Some(open) if open.is_modified() => Some(MapOut::ConfirmSwitch(id)),
                    _ => Some(MapOut::Request(MapRequest::Open(id))),
                }
            }
        }
    }

    fn overlay_input(&mut self, event: InputEvent) -> Option<MapOut> {
        let area = self.metrics.area();
        let (cell_w, cell_h) = (self.metrics.cell_w, self.metrics.cell_h);
        match self.overlay.as_mut()? {
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
