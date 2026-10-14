//! A window asking for one line of text, shared by GO TO LINE and NEW
//! FILE. The window holding it decides what the text means.

use glaux_input::{InputEvent, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::draw_text;

use crate::system::workspace::Rect;
use crate::system::workspace::dialog::{
    AREA_MARGIN, BODY, INK, PAD, SHADOW_OFFSET, draw_window, fill, text_cells,
};
use crate::system::workspace::text_field::{FieldColors, FieldInput, TextField};

/// Cells of text across the window.
const WINDOW_CELLS: i32 = 40;

/// What an input did to a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptInput {
    /// Nothing changed that the owner must act on.
    None,
    /// The text changed.
    Edited,
    /// Enter was pressed.
    Submit,
    /// Escape, or a press outside the window.
    Close,
    /// Put this text on the clipboard.
    Copy(String),
    /// Paste the clipboard into the field.
    Paste,
}

/// The window's title, a line above the field, an optional fixed text
/// before the field, and the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    title: &'static str,
    label: String,
    prefix: &'static str,
    field: TextField,
}

/// Where a prompt's parts sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptLayout {
    pub window: Rect,
    pub title_bar: Rect,
    pub text_x: i32,
    pub label_y: i32,
    pub field: Rect,
    pub message_y: i32,
    pub cell_w: i32,
    /// Cells of text a line holds.
    pub columns: usize,
}

impl Prompt {
    pub fn new(title: &'static str, label: String, prefix: &'static str, field: TextField) -> Self {
        Self {
            title,
            label,
            prefix,
            field,
        }
    }

    pub fn text(&self) -> String {
        self.field.text()
    }

    pub fn paste(&mut self, text: &str) {
        self.field.paste(text);
    }

    /// Applies `event`. Every input is taken while the prompt is open.
    pub fn handle_input(&mut self, event: InputEvent, layout: &PromptLayout) -> PromptInput {
        match self.field.handle_input(event) {
            FieldInput::Edited => return PromptInput::Edited,
            FieldInput::Kept => return PromptInput::None,
            FieldInput::Copy(text) => return PromptInput::Copy(text),
            FieldInput::Paste => return PromptInput::Paste,
            FieldInput::Unhandled => {}
        }
        match event {
            InputEvent::Enter => PromptInput::Submit,
            InputEvent::Break => PromptInput::Close,
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } if !layout.window.contains(at) => PromptInput::Close,
            _ => PromptInput::None,
        }
    }

    /// The window centred in `area`.
    pub fn layout(&self, area: Rect, cell_w: i32, cell_h: i32) -> PromptLayout {
        let cell_w = cell_w.max(1);
        let inner = (WINDOW_CELLS * cell_w).min(area.width - 2 * AREA_MARGIN - 2 * PAD - 2);
        let width = inner + 2 * PAD + 2;
        let title_h = cell_h + 2;
        let field_h = cell_h + 4;
        let height = 1 + title_h + PAD + cell_h + 2 + field_h + 2 + cell_h + PAD + 1;
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
        let label_y = title_bar.bottom() + PAD;
        let prefix = text_cells(self.prefix) * cell_w;
        let field = Rect {
            x: text_x + prefix,
            y: label_y + cell_h + 2,
            width: inner - prefix,
            height: field_h,
        };
        PromptLayout {
            window,
            title_bar,
            text_x,
            label_y,
            field,
            message_y: field.bottom() + 2,
            cell_w,
            columns: usize::try_from(inner / cell_w).unwrap_or(0),
        }
    }

    /// Draws the prompt with `message` under the field in its color.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        layout: &PromptLayout,
        message: (&str, u8),
        lit: bool,
    ) {
        draw_window(
            framebuffer,
            font,
            layout.window,
            layout.title_bar,
            self.title,
            layout.cell_w,
        );
        let clip = |text: &str| {
            crate::system::workspace::take_cells(font, text, layout.columns).to_owned()
        };
        draw_text(
            framebuffer,
            font,
            &clip(&self.label),
            layout.text_x,
            layout.label_y,
            INK,
        );
        let field_text_y = layout.field.y + 2;
        draw_text(
            framebuffer,
            font,
            self.prefix,
            layout.text_x,
            field_text_y,
            INK,
        );
        fill(framebuffer, layout.field, INK);
        self.field.render(
            framebuffer,
            font,
            layout.field.inset(1),
            FieldColors {
                ground: BODY,
                ink: INK,
                selection: INK,
                selected_ink: BODY,
            },
            lit,
            layout.cell_w,
        );
        let (text, color) = message;
        draw_text(
            framebuffer,
            font,
            &clip(text),
            layout.text_x,
            layout.message_y,
            color,
        );
    }
}
