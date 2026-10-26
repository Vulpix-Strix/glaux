//! What the workspace shows once the system has answered one of its requests.

use super::modal::{Modal, delete_dialog, edit_failure, source_write_advice, write_advice};
use super::sprites::ImportPreview;
use super::tab::Tab;
use super::{Workspace, docs_source};
use crate::cartridge::source::{ScriptListing, SourceEditError, SourcePath};
use crate::cartridge::{Language, LoadedCartridge, VideoAsset};
use crate::cartridge_fs::FsError;
use crate::sprite_import::SheetSource;
use crate::system::code_document::{
    CodeDocumentError, CodeSaveError, CodeSaved, DiskState, DocumentId,
};
use crate::system::code_session::{CodeSession, Destination, Landing, MoveScriptError, Spot};
use crate::system::definition::Definition;
use crate::system::logs::SourceLocation;
use crate::system::screen::{ERROR, OK, PALE};
use crate::system::sprite_document::{
    Reopened, SaveError, SaveFailure, Saved, SavedFile, SpriteDocument, file_list,
};
use crate::system::video::CartridgeVideoDefaults;
use crate::ui_text;
use glaux_i18n::Arg;

/// The line telling which files a save that stopped partway did write.
fn saved_line(written: &[SavedFile]) -> Option<String> {
    let names: Vec<String> = written
        .iter()
        .map(|file| file.asset.file_name().to_owned())
        .collect();
    saved_before_stopping(&names)
}

/// The line telling which of `names` a run of saves that stopped partway
/// did write, if any.
pub(in crate::system) fn saved_before_stopping(names: &[String]) -> Option<String> {
    (!names.is_empty()).then(|| {
        ui_text::get_with(
            "workspace-saved-before-stopping",
            &[("value", Arg::Text(&(names.join(", ").to_string())))],
        )
    })
}

/// `names` as one phrase, as in `a.luau, b.luau and graphics`.
pub(super) fn phrase(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => ui_text::get_with(
            "workspace-and",
            &[
                ("value", Arg::Text(&(rest.join(", ").to_string()))),
                ("last", Arg::Text(&last.to_string())),
            ],
        ),
    }
}

impl Workspace {
    /// Shows `location` in the Code view: opens its script if it is not
    /// open, puts the caret at the start of the line and scrolls to it. The
    /// jump can be undone with Back.
    pub fn show_location(
        &mut self,
        code: &mut CodeSession,
        cartridge: &LoadedCartridge,
        location: &SourceLocation,
    ) {
        let target = Destination {
            path: location.path.clone(),
            at: Spot::Line(location.line.saturating_sub(1) as usize),
        };
        self.jump(code, cartridge, &target);
    }

    /// Moves the Code view to `target`, remembering where it was, and shows it.
    pub fn jump(
        &mut self,
        code: &mut CodeSession,
        cartridge: &LoadedCartridge,
        target: &Destination,
    ) {
        self.switch_to(Tab::Code, None);
        match code.jump(cartridge, target) {
            Ok(id) => self.code.show_arrival(code, id, None),
            Err(err) => self.script_opened(code, &target.path, Err(err)),
        }
    }

    /// Goes back to the previous place of the Code view, or forward again, and shows
    /// it. Says so when the place has changed since it was left.
    pub fn travel(&mut self, code: &mut CodeSession, cartridge: &LoadedCartridge, back: bool) {
        let arrival = if back {
            code.go_back(cartridge)
        } else {
            code.go_forward(cartridge)
        };
        let Some(arrival) = arrival else {
            return;
        };
        self.switch_to(Tab::Code, None);
        self.code.show_arrival(code, arrival.id, arrival.top_line);
        if arrival.landing == Landing::LineChanged {
            self.notify(ui_text::get("code-that-place-has-changed"), PALE);
        }
    }

    /// Tells the user, in the status bar, why a definition was not shown.
    pub fn definition_missed(&mut self, outcome: &Definition) {
        let text = match outcome {
            Definition::Found(_) => return,
            Definition::Ambiguous => ui_text::get("code-several-definitions-fit"),
            Definition::Unresolved => ui_text::get("code-no-definition-found"),
            Definition::ModuleMissing(name) => {
                ui_text::get_with("code-module-not-found", &[("value", Arg::Text(name))])
            }
        };
        self.notify(text, PALE);
    }

    /// Returns to the Code view after a run failed with `message`, showing
    /// where when the failure points at a line of a script.
    pub fn run_failed(
        &mut self,
        code: &mut CodeSession,
        cartridge: &LoadedCartridge,
        message: &str,
        location: Option<&SourceLocation>,
    ) {
        match location {
            Some(location) => self.show_location(code, cartridge, location),
            None => self.switch_to(Tab::Code, None),
        }
        self.notify(message.lines().next().unwrap_or_default(), ERROR);
    }

    /// Opens NEW FILE for a cartridge written in `language`.
    pub fn open_new_script(&mut self, language: Language) {
        self.code.open_new_file(language);
    }

    /// Reports how creating `path` in `code` went: the new script's tab
    /// shows once it is created and open, and NEW FILE says why otherwise.
    pub fn script_created(
        &mut self,
        code: &mut CodeSession,
        path: &SourcePath,
        result: Result<(), String>,
    ) {
        match &result {
            Ok(()) => {
                tracing::info!(file = %path, "script created");
                self.code.refresh_metrics(code);
                self.code.reveal_active(code);
                self.notify(
                    ui_text::get_with(
                        "workspace-created",
                        &[("path", Arg::Text(&path.to_string()))],
                    ),
                    PALE,
                );
            }
            Err(message) => {
                tracing::debug!(file = %path, error = %message, "script not created");
            }
        }
        self.code.script_created(result);
    }

    /// Opens DELETE SCRIPT for document `id`, given how its file compares
    /// with what the document was read from, or tells why that could not
    /// be read.
    pub fn delete_prompt(
        &mut self,
        code: &CodeSession,
        id: DocumentId,
        state: Result<DiskState, FsError>,
    ) {
        let Some(document) = code.document(id) else {
            return;
        };
        let path = document.path();
        let external = matches!(state, Ok(DiskState::Modified(_)));
        let seen = match state {
            Ok(DiskState::InSync(bytes) | DiskState::Modified(bytes)) => Some(bytes),
            Ok(DiskState::Missing) => None,
            Err(err) => {
                tracing::debug!(file = %path, %err, "script not readable for deletion");
                self.open_message(
                    &ui_text::get("workspace-delete-failed"),
                    vec![
                        ui_text::get_with(
                            "workspace-delete-read-failed",
                            &[("path", Arg::Text(&path.to_string()))],
                        ),
                        ui_text::get("workspace-nothing-was-deleted"),
                    ],
                );
                return;
            }
        };
        self.code.end_drag();
        let (dialog, target) = delete_dialog(id, path, document.is_modified(), external, seen);
        self.modal = Some(Modal { dialog, target });
    }

    /// Reports how deleting the file of script document `id` went. The
    /// tab closed if it was deleted. Otherwise a message says why not.
    pub fn script_deleted(
        &mut self,
        code: &mut CodeSession,
        path: &SourcePath,
        result: Result<(), SourceEditError>,
    ) {
        match result {
            Ok(()) => {
                tracing::info!(file = %path, "script deleted");
                self.code.refresh_metrics(code);
                self.script_closed(code);
                self.notify(
                    ui_text::get_with(
                        "workspace-deleted",
                        &[("path", Arg::Text(&path.to_string()))],
                    ),
                    PALE,
                );
            }
            Err(err) => {
                tracing::debug!(file = %path, %err, "script not deleted");
                self.open_message(
                    &ui_text::get("workspace-delete-failed"),
                    vec![
                        edit_failure(path, &err),
                        ui_text::get("workspace-nothing-was-deleted"),
                    ],
                );
            }
        }
    }

    /// Reports how moving the file of a script document to `to` went: the
    /// tab shows the new name once it moved, and RENAME SCRIPT says why
    /// otherwise.
    pub fn script_renamed(
        &mut self,
        code: &mut CodeSession,
        to: &SourcePath,
        result: Result<(), MoveScriptError>,
    ) {
        let result = result.map_err(|err| match &err {
            MoveScriptError::File(SourceEditError::Fs(fs)) => edit_failure(to, &fs.clone().into()),
            _ => crate::system::ui_errors::code::move_script(&err),
        });
        match &result {
            Ok(()) => {
                tracing::info!(file = %to, "script renamed");
                self.code.refresh_metrics(code);
                self.code.reveal_active(code);
                self.notify(
                    ui_text::get_with(
                        "workspace-renamed-to",
                        &[("to", Arg::Text(&to.to_string()))],
                    ),
                    PALE,
                );
            }
            Err(message) => {
                tracing::debug!(file = %to, error = %message, "script not renamed");
            }
        }
        self.code.script_renamed(result);
    }

    /// Hands the clipboard's `text` to the Code view, which inserts it
    /// where the keys are.
    pub fn paste(&mut self, code: &mut CodeSession, text: Option<String>) {
        if self.docs_open {
            self.docs.paste(docs_source(), text);
        } else if self.tab == Tab::Code {
            self.code.paste(text, Some(code));
        }
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
                self.notify(
                    ui_text::get_with(
                        "workspace-cannot-open",
                        &[
                            ("path", Arg::Text(&path.to_string())),
                            (
                                "err",
                                Arg::Text(&crate::system::ui_errors::code::document(&err)),
                            ),
                        ],
                    ),
                    ERROR,
                );
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
                self.notify(
                    ui_text::get("workspace-cannot-open-the-entry-script"),
                    ERROR,
                );
                self.code.set_entry_error(Some(message));
            }
        }
    }

    /// Drops what the Code view shows of a cartridge that is no longer
    /// loaded.
    pub fn forget_code(&mut self) {
        self.code.forget();
    }

    /// Reports how a save, or an overwrite after a conflict, went.
    pub fn saved(&mut self, result: Result<Saved, SaveError>) {
        match result {
            Ok(Saved::Written(files)) => match files.as_slice() {
                [file] => self.notify(
                    ui_text::get_with(
                        "workspace-saved-bytes",
                        &[
                            ("value", Arg::Text(&file.asset.to_string())),
                            ("value-2", Arg::Text(&file.bytes.to_string())),
                        ],
                    ),
                    OK,
                ),
                files => self.notify(
                    ui_text::get_with(
                        "workspace-saved",
                        &[(
                            "value",
                            Arg::Text(&(file_list(files.iter().map(|file| file.asset)))),
                        )],
                    ),
                    OK,
                ),
            },
            Ok(Saved::NothingToSave) => {
                self.notify(ui_text::get("workspace-nothing-to-save"), PALE)
            }
            Err(SaveError { written, cause }) => {
                let saved = saved_line(&written);
                match cause {
                    SaveFailure::Conflict(conflicts) => self.open_conflict(conflicts, saved),
                    SaveFailure::Cartridge(err) => {
                        let lines = saved.into_iter().chain([write_advice(&err)]).collect();
                        self.open_error(&ui_text::get("workspace-save-failed"), lines);
                    }
                }
            }
        }
    }

    /// Reports how saving script document `id`, or overwriting its file
    /// after a conflict, went. A conflict asks whether to overwrite the
    /// file. The edits stay in memory whatever happened.
    pub fn script_saved(&mut self, id: DocumentId, result: Result<CodeSaved, CodeSaveError>) {
        match result {
            Ok(CodeSaved::Written { path, bytes }) => {
                self.notify(
                    ui_text::get_with(
                        "workspace-saved-bytes",
                        &[
                            ("value", Arg::Text(&path.to_string())),
                            ("value-2", Arg::Text(&bytes.to_string())),
                        ],
                    ),
                    OK,
                );
            }
            Ok(CodeSaved::NothingToSave(_)) => {
                self.notify(ui_text::get("workspace-nothing-to-save"), PALE)
            }
            Err(CodeSaveError::NotOpen) => {
                self.notify(ui_text::get("workspace-that-file-is-no-longer-open"), PALE)
            }
            Err(CodeSaveError::Conflict(conflict)) => self.open_script_conflict(id, conflict),
            Err(CodeSaveError::Failed { path, error }) => {
                tracing::debug!(file = %path, %error, "script not saved");
                self.open_error(
                    &ui_text::get("workspace-save-failed"),
                    vec![source_write_advice(&path, &error)],
                );
            }
        }
    }

    /// Shows the tabs left after a script document was closed outside the
    /// Code view's own input.
    pub fn script_closed(&mut self, code: &mut CodeSession) {
        self.code.reveal_active(code);
    }

    /// Reports how discarding the changes to `assets` and reading their
    /// files again went.
    pub fn reloaded(&mut self, assets: &[VideoAsset], result: Result<(), String>) {
        match result {
            Ok(()) => self.notify(
                ui_text::get_with(
                    "workspace-reloaded-from-disk",
                    &[("value", Arg::Text(&(file_list(assets.iter().copied()))))],
                ),
                OK,
            ),
            Err(message) => {
                self.open_error(&ui_text::get("workspace-reload-failed"), vec![message])
            }
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
        self.notify(ui_text::get("workspace-no-file-chosen"), PALE);
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
                &ui_text::get("workspace-cannot-import"),
                vec![
                    format!("{err:#}"),
                    ui_text::get("workspace-nothing-was-changed"),
                ],
            ),
        }
    }

    /// Reports what reopening the workspace found on disk.
    pub fn reopened(&mut self, reopened: Result<Reopened, String>) {
        let reopened = match reopened {
            Ok(reopened) => reopened,
            Err(message) => {
                return self
                    .open_error(&ui_text::get("workspace-cannot-read-files"), vec![message]);
            }
        };
        if reopened.history_trimmed {
            self.notify(
                ui_text::get_with(
                    "workspace-changed-on-disk-reloaded-undo-steps-dropped",
                    &[(
                        "value",
                        Arg::Text(&(file_list(reopened.reloaded.iter().copied()))),
                    )],
                ),
                PALE,
            );
        }
        if !reopened.conflicts.is_empty() {
            self.open_conflict(reopened.conflicts, None);
        }
    }
}
