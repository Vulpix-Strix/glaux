//! NEW FILE: creates a script under the cartridge's `code/` folder.
//!
//! The user types the path below `code/` without an extension; the
//! cartridge's language, from its manifest, decides the extension.

use glaux_input::InputEvent;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;

use super::prompt::{Prompt, PromptInput, PromptLayout};
use crate::cartridge::Language;
use crate::cartridge::source::{SourcePath, SourcePathError};
use crate::system::ERROR;
use crate::system::workspace::dialog::INK;
use crate::system::workspace::text_field::TextField;

/// The folder new scripts go in.
pub const CODE_DIR: &str = "code/";
/// Most characters of the path typed.
const MAX_PATH: usize = 96;

/// What the window asks of the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewFileAction {
    None,
    /// Create this script and open it.
    Create(SourcePath),
    Close,
    Copy(String),
    Paste,
}

/// The open window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFile {
    prompt: Prompt,
    language: Language,
    /// Why creating the file last failed, until the path is edited.
    failure: Option<String>,
}

impl NewFile {
    /// The window for a cartridge written in `language`.
    pub fn new(language: Language) -> Self {
        let label = format!("New .{} script in {CODE_DIR}", language.extension());
        Self {
            prompt: Prompt::new("NEW FILE", label, CODE_DIR, TextField::any(MAX_PATH)),
            language,
            failure: None,
        }
    }

    pub fn prompt(&self) -> &Prompt {
        &self.prompt
    }

    pub fn paste(&mut self, text: &str) {
        self.prompt.paste(text);
        self.failure = None;
    }

    /// Shows why creating the file failed; the window stays open.
    pub fn set_failure(&mut self, message: String) {
        self.failure = Some(message);
    }

    /// The script the typed path names: `code/`, the path, and the
    /// language's extension. The error says what is wrong with the path.
    pub fn path(&self) -> Result<SourcePath, String> {
        path_for(&self.prompt.text(), self.language)
    }

    pub fn handle_input(&mut self, event: InputEvent, layout: &PromptLayout) -> NewFileAction {
        match self.prompt.handle_input(event, layout) {
            PromptInput::None => NewFileAction::None,
            PromptInput::Edited => {
                self.failure = None;
                NewFileAction::None
            }
            PromptInput::Submit => match self.path() {
                Ok(path) => NewFileAction::Create(path),
                Err(_) => NewFileAction::None,
            },
            PromptInput::Close => NewFileAction::Close,
            PromptInput::Copy(text) => NewFileAction::Copy(text),
            PromptInput::Paste => NewFileAction::Paste,
        }
    }

    /// The line under the field: what would be created, or why nothing
    /// can be.
    pub fn message(&self) -> (String, u8) {
        if let Some(failure) = &self.failure {
            return (failure.clone(), ERROR);
        }
        if self.prompt.text().is_empty() {
            return (
                format!("Type a name; .{} is added.", self.language.extension()),
                INK,
            );
        }
        match self.path() {
            Ok(path) => (format!("Creates {path}"), INK),
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

/// The script `typed` names under [`CODE_DIR`] in `language`.
pub fn path_for(typed: &str, language: Language) -> Result<SourcePath, String> {
    if typed.is_empty() {
        return Err("Type a name for the script.".to_owned());
    }
    if let Some(given) = Language::ALL
        .into_iter()
        .find(|given| typed.ends_with(&format!(".{}", given.extension())))
    {
        return Err(if given == language {
            format!("Leave out .{}; it is added.", language.extension())
        } else {
            format!(
                "This cartridge is {}; .{} is added.",
                language.name(),
                language.extension()
            )
        });
    }
    let path = format!("{CODE_DIR}{typed}.{}", language.extension());
    SourcePath::parse(&path).map_err(|err| match err {
        SourcePathError::Invalid(_) => "Use names joined by /, without .., \\ or :".to_owned(),
        SourcePathError::UnsupportedExtension(_) => {
            format!("Not a valid .{} file name.", language.extension())
        }
    })
}

#[cfg(test)]
mod tests;
