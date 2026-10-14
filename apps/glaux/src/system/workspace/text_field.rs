//! A one-line text field for the workspace's windows and bars.
//!
//! The field is the only thing that reads keys while it has focus: it
//! takes the keys that edit a line, the clipboard commands and Select All,
//! and hands everything else back to the window it sits in. That keeps a
//! command from acting on the field and on the document behind it at once.

use glaux_input::InputEvent;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{draw_text, fill_rect};

use super::Rect;
use crate::system::line_editor::LineEditor;

/// What a field did with an input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldInput {
    /// The text changed.
    Edited,
    /// Taken, without changing the text: a move, or a command that has
    /// nothing to act on.
    Kept,
    /// Put this text on the clipboard. A cut has already removed it.
    Copy(String),
    /// Ask for the clipboard's text, to hand back with
    /// [`TextField::paste`].
    Paste,
    /// Not a field input; the window holding the field decides.
    Unhandled,
}

/// Colors a field is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldColors {
    pub ground: u8,
    pub ink: u8,
    pub selection: u8,
    pub selected_ink: u8,
}

/// The characters a field takes, control characters never among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accepts {
    Any,
    Digits,
}

impl Accepts {
    fn allows(self, ch: char) -> bool {
        !ch.is_control()
            && match self {
                Self::Any => true,
                Self::Digits => ch.is_ascii_digit(),
            }
    }
}

/// An editable line holding at most `max_chars` characters of the kind
/// `accepts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextField {
    editor: LineEditor,
    max_chars: usize,
    accepts: Accepts,
}

impl TextField {
    pub fn new(max_chars: usize, accepts: Accepts) -> Self {
        Self {
            editor: LineEditor::default(),
            max_chars,
            accepts,
        }
    }

    /// A field accepting any character that is not a control character.
    pub fn any(max_chars: usize) -> Self {
        Self::new(max_chars, Accepts::Any)
    }

    pub fn text(&self) -> String {
        self.editor.text()
    }

    /// Replaces the text with what of `text` the field accepts, selecting
    /// it all so typing replaces it.
    pub fn set_text(&mut self, text: &str) {
        self.editor.set_text("");
        self.insert(text);
        self.editor.select_all();
    }

    /// Inserts the clipboard's `text` over the selection: its first line,
    /// as far as the field accepts it.
    pub fn paste(&mut self, text: &str) {
        let line = text.split(['\r', '\n']).next().unwrap_or("");
        self.insert(line);
    }

    /// Inserts what of `text` the field accepts over the selection, up to
    /// its length limit.
    fn insert(&mut self, text: &str) {
        let selected = self.editor.selection().map_or(0, |range| range.len());
        let room = self
            .max_chars
            .saturating_sub(self.editor.chars().len() - selected);
        let accepted: String = text
            .chars()
            .filter(|&ch| self.accepts.allows(ch))
            .take(room)
            .collect();
        self.editor.insert_str(&accepted);
    }

    /// Applies `event` if it is a field input.
    ///
    /// Undo and Redo are taken and do nothing: a field keeps no history,
    /// and they must not reach the document behind it.
    pub fn handle_input(&mut self, event: InputEvent) -> FieldInput {
        match event {
            InputEvent::Text(ch) => {
                if !self.accepts.allows(ch)
                    || (self.editor.selection().is_none()
                        && self.editor.chars().len() >= self.max_chars)
                {
                    return FieldInput::Kept;
                }
                self.editor.apply(event);
                FieldInput::Edited
            }
            InputEvent::Backspace | InputEvent::Delete => {
                let before = self.editor.chars().len();
                self.editor.apply(event);
                if self.editor.chars().len() == before {
                    FieldInput::Kept
                } else {
                    FieldInput::Edited
                }
            }
            InputEvent::Left { .. }
            | InputEvent::Right { .. }
            | InputEvent::Home { .. }
            | InputEvent::End { .. } => {
                self.editor.apply(event);
                FieldInput::Kept
            }
            // A field is one line: word and document moves go to its ends.
            InputEvent::WordLeft { select } | InputEvent::DocumentStart { select } => {
                self.editor.apply(InputEvent::Home { select });
                FieldInput::Kept
            }
            InputEvent::WordRight { select } | InputEvent::DocumentEnd { select } => {
                self.editor.apply(InputEvent::End { select });
                FieldInput::Kept
            }
            InputEvent::SelectAll => {
                self.editor.select_all();
                FieldInput::Kept
            }
            InputEvent::Copy => self
                .editor
                .selected_text()
                .map_or(FieldInput::Kept, FieldInput::Copy),
            InputEvent::Cut => self
                .editor
                .cut_selection()
                .map_or(FieldInput::Kept, FieldInput::Copy),
            InputEvent::Paste => FieldInput::Paste,
            InputEvent::Undo | InputEvent::Redo => FieldInput::Kept,
            _ => FieldInput::Unhandled,
        }
    }

    /// Draws the field in `rect`, with its cursor when `cursor` is set.
    /// Text longer than the field scrolls to keep the cursor in view.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        rect: Rect,
        colors: FieldColors,
        cursor: bool,
        cell_w: i32,
    ) {
        fill_rect(
            framebuffer,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            colors.ground,
        );
        let cell_w = cell_w.max(1);
        let cell_h = i32::from(font.metrics().cell_height());
        // One cell stays free after the text for the cursor.
        let room = usize::try_from((rect.width - 2) / cell_w - 1).unwrap_or(0);
        let chars = self.editor.chars();
        let cursor_at = self.editor.cursor();
        // The first character shown: as far left as keeps the cursor in.
        let mut first = cursor_at;
        let mut used = 0;
        while first > 0 && used + font.cells(chars[first - 1]) <= room {
            first -= 1;
            used += font.cells(chars[first]);
        }
        let y = rect.y + (rect.height - cell_h) / 2;
        let selection = self.editor.selection().unwrap_or(0..0);
        let mut x = rect.x + 1;
        let mut cursor_x = x;
        let right = rect.x + 1 + room as i32 * cell_w;
        for (position, &ch) in chars.iter().enumerate().skip(first) {
            if position == cursor_at {
                cursor_x = x;
            }
            let width = font.cells(ch) as i32 * cell_w;
            if x + width > right {
                break;
            }
            let ink = if selection.contains(&position) {
                fill_rect(framebuffer, x, y, width, cell_h, colors.selection);
                colors.selected_ink
            } else {
                colors.ink
            };
            let mut buffer = [0; 4];
            draw_text(framebuffer, font, ch.encode_utf8(&mut buffer), x, y, ink);
            x += width;
        }
        if cursor_at >= chars.len() {
            cursor_x = x;
        }
        if cursor {
            fill_rect(framebuffer, cursor_x, y, 1, cell_h, colors.ink);
        }
    }
}

#[cfg(test)]
mod tests;
