//! The window that asks for a new map's name, id and size.

use glaux_input::{InputEvent, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::draw_text;

use crate::cartridge::map::{MAX_ID_LEN, MapError, MapId, MapMeta};
use crate::system::screen::ERROR;
use crate::system::ui_errors;
use crate::system::workspace::Rect;
use crate::system::workspace::dialog::{
    AREA_MARGIN, BODY, INK, PAD, SHADOW_OFFSET, draw_button, draw_window, fill, text_cells,
};
use crate::system::workspace::take_cells;
use crate::system::workspace::text_field::{Accepts, FieldColors, FieldInput, TextField};
use crate::ui_text;
use glaux_i18n::Arg;

const WINDOW_CELLS: i32 = 44;
const LABEL_CELLS: i32 = 8;
const MAX_NAME: usize = 40;
const MAX_SIZE_DIGITS: usize = 3;
/// A map the size of one screen, in tiles.
const DEFAULT_WIDTH: &str = "40";
const DEFAULT_HEIGHT: &str = "30";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Id,
    Width,
    Height,
}

impl Field {
    const ALL: [Self; 4] = [Self::Name, Self::Id, Self::Width, Self::Height];

    fn index(self) -> usize {
        self as usize
    }

    fn label_id(self) -> &'static str {
        match self {
            Self::Name => "maps-field-name",
            Self::Id => "maps-field-id",
            Self::Width => "maps-field-width",
            Self::Height => "maps-field-height",
        }
    }
}

/// What the window asks of the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FormInput {
    None,
    Create(MapId, MapMeta),
    Close,
    Copy(String),
    Paste,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CreateForm {
    fields: [TextField; 4],
    focus: Field,
    /// Whether the user typed in the id, which stops it following the name.
    id_edited: bool,
    existing: Vec<MapId>,
    /// Why creating the map last failed, until a field is edited.
    failure: Option<String>,
}

/// Where the window's parts sit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FormLayout {
    pub(super) window: Rect,
    pub(super) title_bar: Rect,
    pub(super) text_x: i32,
    /// Top of each label row, in the order of the fields.
    pub(super) label_y: [i32; 4],
    pub(super) fields: [Rect; 4],
    pub(super) message_y: i32,
    pub(super) create: Rect,
    pub(super) cancel: Rect,
    pub(super) cell_w: i32,
    pub(super) cell_h: i32,
    pub(super) columns: usize,
}

impl CreateForm {
    pub(super) fn new(existing: Vec<MapId>) -> Self {
        let mut width = TextField::new(MAX_SIZE_DIGITS, Accepts::Digits);
        width.set_text(DEFAULT_WIDTH);
        let mut height = TextField::new(MAX_SIZE_DIGITS, Accepts::Digits);
        height.set_text(DEFAULT_HEIGHT);
        Self {
            fields: [
                TextField::any(MAX_NAME),
                TextField::any(MAX_ID_LEN),
                width,
                height,
            ],
            focus: Field::Name,
            id_edited: false,
            existing,
            failure: None,
        }
    }

    pub(super) fn paste(&mut self, text: &str) {
        self.fields[self.focus.index()].paste(text);
        self.edited();
    }

    /// Shows why creating the map failed. The window stays open.
    pub(super) fn set_failure(&mut self, message: String) {
        self.failure = Some(message);
    }

    fn edited(&mut self) {
        self.failure = None;
        match self.focus {
            Field::Name if !self.id_edited => {
                let id = slug(&self.fields[Field::Name.index()].text());
                self.fields[Field::Id.index()].set_text(&id);
            }
            Field::Id => self.id_edited = true,
            _ => {}
        }
    }

    /// The map the fields describe, or what is wrong with them.
    pub(super) fn validate(&self) -> Result<(MapId, MapMeta), String> {
        let text = |field: Field| self.fields[field.index()].text();
        let name = text(Field::Name);
        let name = name.trim();
        if name.is_empty() {
            return Err(ui_errors::map::map(&MapError::BlankName));
        }
        let id = MapId::parse(&text(Field::Id)).map_err(|err| ui_errors::map::id(&err))?;
        if self.existing.contains(&id) {
            return Err(ui_text::get_with(
                "error-map-exists",
                &[("id", Arg::Text(&id.to_string()))],
            ));
        }
        let size = |field: Field| {
            text(field)
                .parse::<usize>()
                .map_err(|_| ui_text::get("maps-size-number"))
        };
        let (width, height) = (size(Field::Width)?, size(Field::Height)?);
        let meta = MapMeta::new(name, width, height).map_err(|err| ui_errors::map::map(&err))?;
        Ok((id, meta))
    }

    /// The line under the fields and its color. It says what is wrong with
    /// them, or shows a hint when nothing is.
    pub(super) fn message(&self) -> (String, u8) {
        if let Some(failure) = &self.failure {
            return (failure.clone(), ERROR);
        }
        match self.validate() {
            Ok(_) => (ui_text::get("maps-id-hint"), INK),
            Err(_) if self.fields[Field::Name.index()].is_empty() => {
                (ui_text::get("maps-id-hint"), INK)
            }
            Err(message) => (message, ERROR),
        }
    }

    pub(super) fn handle_input(&mut self, event: InputEvent, layout: &FormLayout) -> FormInput {
        match event {
            InputEvent::Tab { reverse } => {
                let step = if reverse { Field::ALL.len() - 1 } else { 1 };
                self.focus = Field::ALL[(self.focus.index() + step) % Field::ALL.len()];
                return FormInput::None;
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                if !layout.window.contains(at) {
                    return FormInput::Close;
                }
                if layout.cancel.contains(at) {
                    return FormInput::Close;
                }
                if layout.create.contains(at) {
                    return self.submit();
                }
                if let Some(field) = Field::ALL
                    .into_iter()
                    .find(|field| layout.fields[field.index()].contains(at))
                {
                    self.focus = field;
                }
                return FormInput::None;
            }
            _ => {}
        }
        match self.fields[self.focus.index()].handle_input(event) {
            FieldInput::Edited => {
                self.edited();
                return FormInput::None;
            }
            FieldInput::Kept => return FormInput::None,
            FieldInput::Copy(text) => return FormInput::Copy(text),
            FieldInput::Paste => return FormInput::Paste,
            FieldInput::Unhandled => {}
        }
        match event {
            InputEvent::Enter => self.submit(),
            InputEvent::Break => FormInput::Close,
            _ => FormInput::None,
        }
    }

    fn submit(&self) -> FormInput {
        match self.validate() {
            Ok((id, meta)) => FormInput::Create(id, meta),
            Err(_) => FormInput::None,
        }
    }

    /// The window centred in `area`.
    pub(super) fn layout(&self, area: Rect, cell_w: i32, cell_h: i32) -> FormLayout {
        let cell_w = cell_w.max(1);
        let inner = (WINDOW_CELLS * cell_w).min(area.width - 2 * AREA_MARGIN - 2 * PAD - 2);
        let width = inner + 2 * PAD + 2;
        let title_h = cell_h + 2;
        let row_h = cell_h + 6;
        let button_h = cell_h + 6;
        let height = 1 + title_h + PAD + 4 * row_h + cell_h + 4 + button_h + PAD + 1;
        let window = Rect {
            x: area.x + (area.width - width - SHADOW_OFFSET) / 2,
            y: area.y + (area.height - height - SHADOW_OFFSET) / 2,
            width,
            height,
        };
        let title_bar = Rect {
            x: window.x + 1,
            y: window.y + 1,
            width: width - 2,
            height: title_h,
        };
        let text_x = window.x + 1 + PAD;
        let first_row = title_bar.bottom() + PAD;
        let label_w = LABEL_CELLS * cell_w;
        let mut label_y = [0; 4];
        let mut fields = [window; 4];
        for field in Field::ALL {
            let top = first_row + field.index() as i32 * row_h;
            label_y[field.index()] = top + (row_h - 2 - cell_h) / 2 + 1;
            fields[field.index()] = Rect {
                x: text_x + label_w,
                y: top,
                width: inner - label_w,
                height: row_h - 2,
            };
        }
        let message_y = first_row + 4 * row_h + 2;
        let button_cells = [ui_text::get("maps-create"), ui_text::get("maps-cancel")]
            .iter()
            .map(|label| text_cells(label))
            .max()
            .unwrap_or(0);
        let button_w = (button_cells + 2) * cell_w;
        let buttons_y = message_y + cell_h + 4;
        let cancel = Rect {
            x: text_x + inner - button_w,
            y: buttons_y,
            width: button_w,
            height: button_h,
        };
        let create = Rect {
            x: cancel.x - 3 - button_w,
            ..cancel
        };
        FormLayout {
            window,
            title_bar,
            text_x,
            label_y,
            fields,
            message_y,
            create,
            cancel,
            cell_w,
            cell_h,
            columns: usize::try_from(inner / cell_w).unwrap_or(0),
        }
    }

    pub(super) fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        layout: &FormLayout,
        lit: bool,
    ) {
        draw_window(
            framebuffer,
            font,
            layout.window,
            layout.title_bar,
            &ui_text::get("maps-new-title"),
            layout.cell_w,
        );
        for field in Field::ALL {
            let i = field.index();
            draw_text(
                framebuffer,
                font,
                &ui_text::get(field.label_id()),
                layout.text_x,
                layout.label_y[i],
                INK,
            );
            fill(framebuffer, layout.fields[i], INK);
            self.fields[i].render(
                framebuffer,
                font,
                layout.fields[i].inset(1),
                // Only the focused field shows what is selected.
                if field == self.focus {
                    FieldColors {
                        ground: BODY,
                        ink: INK,
                        selection: INK,
                        selected_ink: BODY,
                    }
                } else {
                    FieldColors {
                        ground: BODY,
                        ink: INK,
                        selection: BODY,
                        selected_ink: INK,
                    }
                },
                lit && field == self.focus,
                layout.cell_w,
            );
        }
        let (message, color) = self.message();
        draw_text(
            framebuffer,
            font,
            take_cells(font, &message, layout.columns),
            layout.text_x,
            layout.message_y,
            color,
        );
        // CREATE is drawn lit once the fields describe a map it can create.
        let valid = self.validate().is_ok();
        draw_button(
            framebuffer,
            font,
            layout.create,
            &ui_text::get("maps-create"),
            valid,
            layout.cell_w,
            layout.cell_h,
        );
        draw_button(
            framebuffer,
            font,
            layout.cancel,
            &ui_text::get("maps-cancel"),
            false,
            layout.cell_w,
            layout.cell_h,
        );
    }
}

/// Derives the id a new map starts with from its name. Letters and digits
/// are lowercased and every other run of characters becomes a single `-`.
pub(super) fn slug(name: &str) -> String {
    let mut slug = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let trimmed = slug.trim_end_matches('-');
    trimmed
        .chars()
        .take(MAX_ID_LEN)
        .collect::<String>()
        .trim_end_matches('-')
        .to_owned()
}
