//! Asking what to do with unsaved edits before something drops them:
//! quitting Glaux, or a shell command that replaces the cartridge.
//!
//! One dialog lists every unsaved script and the graphics. SAVE ALL saves
//! them one file at a time, each save complete on its own: a save that
//! fails stops the run, keeps whatever was already written, and the action
//! that asked does not go ahead. A file changed outside Glaux is settled
//! in the same dialog, after which saving carries on.

use std::collections::VecDeque;

use glaux_input::InputEvent;

use super::code_document::{CodeSaveError, CodeSaved, DocumentId};
use super::code_session::CodeSession;
use super::sprite_document::{SaveError, SaveFailure, Saved, SpriteDocument};
use super::video::CartridgeVideoDefaults;
use super::workspace::{
    Choice, Dialog, DialogLayout, Target, asset_conflict_dialog, message_dialog,
    saved_before_stopping, script_conflict_dialog, source_write_advice, unsaved_dialog,
    write_advice,
};
use crate::cartridge::LoadedCartridge;

/// What waits on the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Pending {
    /// Run `line`, a shell command that drops unsaved edits without asking.
    /// `action` is the command as the user typed it.
    Command { action: String, line: String },
    /// Quit Glaux.
    Exit,
}

impl Pending {
    /// What going ahead does, as in "runs load pong".
    fn what(&self) -> String {
        match self {
            Self::Command { action, .. } => format!("runs {action}"),
            Self::Exit => "quits Glaux".to_owned(),
        }
    }

    /// The line telling that it did not go ahead.
    fn stopped(&self) -> String {
        match self {
            Self::Command { action, .. } => format!("{action} did not run."),
            Self::Exit => "Glaux did not quit.".to_owned(),
        }
    }
}

/// One file, or set of files, SAVE ALL writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Script(DocumentId),
    /// Every modified asset of the sprite document.
    Graphics,
}

/// What the dialog on screen asks.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    /// Whether to save, discard or cancel.
    Ask,
    /// What to do about a file that changed outside Glaux.
    Conflict(Target),
    /// Nothing; it tells why saving stopped.
    Failed,
}

/// How the flow ended, or that it goes on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Outcome {
    Open,
    /// The edits were saved or discarded: carry out the pending action.
    Proceed(Pending),
    /// The pending action does not go ahead, with a line to report, if
    /// there is anything to say.
    Stopped(Option<String>),
}

/// The edits the flow saves, and what saving them needs.
pub(super) struct Edits<'a> {
    pub(super) cartridge: &'a LoadedCartridge,
    pub(super) code: &'a mut CodeSession,
    pub(super) sprites: Option<&'a mut SpriteDocument>,
    pub(super) defaults: &'a CartridgeVideoDefaults,
}

/// The open question about unsaved edits and the action waiting on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UnsavedFlow {
    pending: Pending,
    dialog: Dialog<Choice>,
    stage: Stage,
    /// What SAVE ALL has yet to write, next first.
    queue: VecDeque<Item>,
    /// What SAVE ALL has written so far.
    saved: Vec<String>,
}

impl UnsavedFlow {
    /// Asks about the unsaved edits in `code` and `sprites` before
    /// `pending`, or `None` when there are none.
    pub(super) fn new(
        pending: Pending,
        code: Option<&CodeSession>,
        sprites: Option<&SpriteDocument>,
    ) -> Option<Self> {
        let mut names: Vec<String> = code
            .map(|code| {
                code.modified()
                    .into_iter()
                    .map(|document| document.path().to_string())
                    .collect()
            })
            .unwrap_or_default();
        if sprites.is_some_and(SpriteDocument::is_modified) {
            names.push("graphics".to_owned());
        }
        if names.is_empty() {
            return None;
        }
        Some(Self {
            dialog: unsaved_dialog(&names, &pending.what()),
            pending,
            stage: Stage::Ask,
            queue: VecDeque::new(),
            saved: Vec::new(),
        })
    }

    pub(super) fn dialog(&self) -> &Dialog<Choice> {
        &self.dialog
    }

    /// Applies `event` to the dialog laid out as `layout`, saving `edits`
    /// as the choice asks.
    pub(super) fn handle_input(
        &mut self,
        event: InputEvent,
        layout: &DialogLayout,
        mut edits: Edits<'_>,
    ) -> Outcome {
        let Some(choice) = self.dialog.handle_input(event, layout) else {
            return Outcome::Open;
        };
        let stage = std::mem::replace(&mut self.stage, Stage::Failed);
        match (stage, choice) {
            (Stage::Ask, Choice::Save) => {
                self.queue = edits
                    .code
                    .modified()
                    .into_iter()
                    .map(|document| Item::Script(document.id()))
                    .collect();
                if edits
                    .sprites
                    .as_deref()
                    .is_some_and(SpriteDocument::is_modified)
                {
                    self.queue.push_back(Item::Graphics);
                }
                self.run(edits)
            }
            (Stage::Ask, Choice::Discard) => Outcome::Proceed(self.pending.clone()),
            (Stage::Conflict(Target::Script { id, seen }), Choice::Overwrite) => {
                let result = edits.code.overwrite(edits.cartridge, id, seen.as_deref());
                match self.script_saved(id, result) {
                    Ok(()) => self.run(edits),
                    Err(outcome) => outcome,
                }
            }
            (Stage::Conflict(Target::Assets(seen)), Choice::Overwrite) => {
                if let Some(sprites) = edits.sprites.as_deref_mut() {
                    let result = sprites.overwrite(edits.cartridge, &seen);
                    if let Err(outcome) = self.graphics_saved(result) {
                        return outcome;
                    }
                }
                self.run(edits)
            }
            (Stage::Conflict(Target::Assets(seen)), Choice::Reload) => {
                if let Some(sprites) = edits.sprites.as_deref_mut() {
                    let assets: Vec<_> = seen.iter().map(|disk| disk.asset).collect();
                    if let Err(err) = sprites.reload(edits.cartridge, &assets, edits.defaults) {
                        self.fail("RELOAD FAILED", err.to_string());
                        return Outcome::Open;
                    }
                    // Assets outside the conflict may still be unsaved.
                    self.queue.push_front(Item::Graphics);
                }
                self.run(edits)
            }
            _ => self.stop(),
        }
    }

    /// Saves what is left in the queue, in order, until it is empty or a
    /// save does not complete.
    fn run(&mut self, mut edits: Edits<'_>) -> Outcome {
        while let Some(item) = self.queue.pop_front() {
            let done = match item {
                Item::Script(id) => {
                    let result = edits.code.save(edits.cartridge, id);
                    self.script_saved(id, result)
                }
                Item::Graphics => match edits.sprites.as_deref_mut() {
                    Some(sprites) => {
                        let result = sprites.save(edits.cartridge);
                        self.graphics_saved(result)
                    }
                    None => Ok(()),
                },
            };
            if let Err(outcome) = done {
                return outcome;
            }
        }
        Outcome::Proceed(self.pending.clone())
    }

    /// Notes what saving script `id` wrote, or shows why it stopped.
    fn script_saved(
        &mut self,
        id: DocumentId,
        result: Result<CodeSaved, CodeSaveError>,
    ) -> Result<(), Outcome> {
        match result {
            Ok(CodeSaved::Written { path, .. }) => {
                self.saved.push(path.to_string());
                Ok(())
            }
            // Closed or already saved: nothing of it is left to lose.
            Ok(CodeSaved::NothingToSave(_)) | Err(CodeSaveError::NotOpen) => Ok(()),
            Err(CodeSaveError::Conflict(conflict)) => {
                let saved = saved_before_stopping(&self.saved);
                let (dialog, target) = script_conflict_dialog(id, conflict, saved);
                self.show(dialog, Stage::Conflict(target));
                Err(Outcome::Open)
            }
            Err(CodeSaveError::Failed { path, error }) => {
                tracing::debug!(file = %path, %error, "script not saved");
                self.fail("SAVE FAILED", source_write_advice(&path, &error));
                Err(Outcome::Open)
            }
        }
    }

    /// Notes what saving the graphics wrote, or shows why it stopped.
    fn graphics_saved(&mut self, result: Result<Saved, SaveError>) -> Result<(), Outcome> {
        let (written, cause) = match result {
            Ok(Saved::Written(written)) => (written, None),
            Ok(Saved::NothingToSave) => (Vec::new(), None),
            Err(SaveError { written, cause }) => (written, Some(cause)),
        };
        self.saved
            .extend(written.iter().map(|file| file.asset.file_name().to_owned()));
        match cause {
            None => Ok(()),
            Some(SaveFailure::Conflict(conflicts)) => {
                let saved = saved_before_stopping(&self.saved);
                let (dialog, target) = asset_conflict_dialog(conflicts, saved);
                self.show(dialog, Stage::Conflict(target));
                Err(Outcome::Open)
            }
            Some(SaveFailure::Cartridge(err)) => {
                self.fail("SAVE FAILED", write_advice(&err));
                Err(Outcome::Open)
            }
        }
    }

    fn show(&mut self, dialog: Dialog<Choice>, stage: Stage) {
        self.dialog = dialog;
        self.stage = stage;
    }

    /// Shows that saving stopped, for the reason in `advice`.
    fn fail(&mut self, title: &str, advice: String) {
        let mut lines: Vec<String> = saved_before_stopping(&self.saved).into_iter().collect();
        lines.push(advice);
        lines.push("Your other edits are still in memory.".to_owned());
        lines.push(self.pending.stopped());
        self.show(message_dialog(title, lines), Stage::Failed);
    }

    /// Gives up on the pending action, telling what was saved first.
    fn stop(&self) -> Outcome {
        Outcome::Stopped(
            saved_before_stopping(&self.saved)
                .map(|saved| format!("{saved} {}", self.pending.stopped())),
        )
    }
}
