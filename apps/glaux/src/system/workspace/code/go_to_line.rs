//! GO TO LINE moves the caret to a line typed by number.

use glaux_input::InputEvent;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;

use super::prompt::{Prompt, PromptInput, PromptLayout};
use crate::system::ERROR;
use crate::system::workspace::dialog::INK;
use crate::system::workspace::text_field::{Accepts, TextField};

/// Most digits a line number takes.
const MAX_DIGITS: usize = 9;

/// What the window asks of the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoToAction {
    None,
    /// Move to this line, counted from 0, and close.
    Go(usize),
    Close,
    Copy(String),
    Paste,
}

/// The open window, for a document of `lines` lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoToLine {
    prompt: Prompt,
    lines: usize,
    /// The last Enter named no line of the document.
    invalid: bool,
}

impl GoToLine {
    /// The window for a document of `lines` lines, the field holding
    /// `current`, the caret's line counted from 1, selected.
    pub fn new(lines: usize, current: usize) -> Self {
        let mut field = TextField::new(MAX_DIGITS, Accepts::Digits);
        field.set_text(&current.to_string());
        Self {
            prompt: Prompt::new("GO TO LINE", format!("Line number (1-{lines}):"), "", field),
            lines,
            invalid: false,
        }
    }

    pub fn prompt(&self) -> &Prompt {
        &self.prompt
    }

    pub fn paste(&mut self, text: &str) {
        self.prompt.paste(text);
        self.invalid = false;
    }

    /// The line the field names, counted from 0, if the document has it.
    fn line(&self) -> Option<usize> {
        let number: usize = self.prompt.text().parse().ok()?;
        (1..=self.lines).contains(&number).then(|| number - 1)
    }

    pub fn handle_input(&mut self, event: InputEvent, layout: &PromptLayout) -> GoToAction {
        match self.prompt.handle_input(event, layout) {
            PromptInput::None => GoToAction::None,
            PromptInput::Edited => {
                self.invalid = false;
                GoToAction::None
            }
            PromptInput::Submit => match self.line() {
                Some(line) => GoToAction::Go(line),
                None => {
                    self.invalid = true;
                    GoToAction::None
                }
            },
            PromptInput::Close => GoToAction::Close,
            PromptInput::Copy(text) => GoToAction::Copy(text),
            PromptInput::Paste => GoToAction::Paste,
        }
    }

    /// The line under the field.
    pub fn message(&self) -> (String, u8) {
        if self.invalid {
            (format!("Enter a line from 1 to {}.", self.lines), ERROR)
        } else {
            ("Enter: go  Esc: cancel".to_owned(), INK)
        }
    }

    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        layout: &PromptLayout,
        lit: bool,
    ) {
        let (message, color) = self.message();
        self.prompt
            .render(framebuffer, font, layout, (&message, color), lit);
    }
}
