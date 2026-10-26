//! RENAME SCRIPT moves a script to another path under `code/`.
//!
//! The user edits the path below `code/` without an extension. The script
//! keeps the language, and so the extension, it already has.

use glaux_i18n::Arg;
use glaux_input::InputEvent;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;

use super::new_file::{CODE_DIR, path_for};
use super::prompt::{Prompt, PromptInput, PromptLayout};
use crate::cartridge::source::SourcePath;
use crate::system::code_document::DocumentId;
use crate::system::screen::ERROR;
use crate::system::workspace::dialog::INK;
use crate::system::workspace::text_field::TextField;
use crate::ui_text;

/// Most characters of the path typed.
const MAX_PATH: usize = 96;

/// What the window asks of the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameAction {
    None,
    /// Move this document's file to this path.
    Rename {
        id: DocumentId,
        to: SourcePath,
    },
    Close,
    Copy(String),
    Paste,
}

/// The open window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameScript {
    prompt: Prompt,
    id: DocumentId,
    from: SourcePath,
    /// Why moving the file last failed, until the path is edited.
    failure: Option<String>,
}

impl RenameScript {
    /// The window for document `id`, whose file is `from`. `None` if
    /// `from` is not under [`CODE_DIR`].
    pub fn new(id: DocumentId, from: SourcePath) -> Option<Self> {
        let extension = format!(".{}", from.language().extension());
        let typed = from
            .as_str()
            .strip_prefix(CODE_DIR)?
            .strip_suffix(&extension)?;
        let mut field = TextField::any(MAX_PATH);
        field.set_text(typed);
        let label = ui_text::get_with(
            "code-rename-description",
            &[
                ("from", Arg::Text(&from.to_string())),
                ("directory", Arg::Text(CODE_DIR)),
            ],
        );
        Some(Self {
            prompt: Prompt::new(
                &ui_text::get("code-rename-script-2"),
                label,
                CODE_DIR,
                field,
            ),
            id,
            from,
            failure: None,
        })
    }

    pub fn prompt(&self) -> &Prompt {
        &self.prompt
    }

    pub fn paste(&mut self, text: &str) {
        self.prompt.paste(text);
        self.failure = None;
    }

    /// Shows why moving the file failed. The window stays open.
    pub fn set_failure(&mut self, message: String) {
        self.failure = Some(message);
    }

    /// The script the typed path names, in the language the script has.
    pub fn path(&self) -> Result<SourcePath, String> {
        path_for(&self.prompt.text(), self.from.language())
    }

    pub fn handle_input(&mut self, event: InputEvent, layout: &PromptLayout) -> RenameAction {
        match self.prompt.handle_input(event, layout) {
            PromptInput::None => RenameAction::None,
            PromptInput::Edited => {
                self.failure = None;
                RenameAction::None
            }
            PromptInput::Submit => match self.path() {
                // Nowhere to move it: nothing to ask the filesystem.
                Ok(to) if to == self.from => RenameAction::Close,
                Ok(to) => RenameAction::Rename { id: self.id, to },
                Err(_) => RenameAction::None,
            },
            PromptInput::Close => RenameAction::Close,
            PromptInput::Copy(text) => RenameAction::Copy(text),
            PromptInput::Paste => RenameAction::Paste,
        }
    }

    /// where the script would go, or why it can't.
    pub fn message(&self) -> (String, u8) {
        if let Some(failure) = &self.failure {
            return (failure.clone(), ERROR);
        }
        match self.path() {
            Ok(path) if path == self.from => (ui_text::get("code-type-a-new-path"), INK),
            Ok(path) => (
                ui_text::get_with(
                    "code-moves-it-to",
                    &[("path", Arg::Text(&path.to_string()))],
                ),
                INK,
            ),
            Err(message) => (message, ERROR),
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
