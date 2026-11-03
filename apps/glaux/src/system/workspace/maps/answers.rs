//! What the workspace shows once the system has answered one of the Map
//! view's requests.

use crate::cartridge::map::{MapDeleteError, MapError, MapFiles, MapId, MapSummary};
use crate::cartridge_fs::FsError;
use crate::system::map_document::{Reopened, SaveError, SaveFailure, Saved, file_list};
use crate::system::screen::{ERROR, OK, PALE};
use crate::system::ui_errors;
use crate::system::workspace::answers::saved_before_stopping;
use crate::system::workspace::modal::{
    Modal, map_conflict_dialog, map_delete_dialog, message_dialog,
};
use crate::system::workspace::request::MapRequest;
use crate::system::workspace::{Tab, Workspace};
use crate::ui_text;
use glaux_i18n::Arg;

impl Workspace {
    /// Returns the request to list the maps when the Map view needs them,
    /// which is once per visit to the view.
    pub fn take_map_request(&mut self) -> Option<MapRequest> {
        (self.tab == Tab::Map && self.maps.take_stale()).then_some(MapRequest::List)
    }

    /// Asks for the maps to be listed again the next time the view shows,
    /// as when the workspace opens.
    pub fn refresh_maps(&mut self) {
        self.maps.mark_stale();
    }

    /// Forgets the maps of a cartridge that was replaced or unloaded.
    pub fn forget_maps(&mut self) {
        self.maps.forget();
    }

    /// The map to open when none is open yet.
    pub fn map_to_open(&self) -> Option<MapId> {
        self.maps.to_open()
    }

    pub fn maps_listed(&mut self, listing: Result<Vec<MapSummary>, FsError>) {
        self.maps.listed(listing.map_err(|err| ui_errors::fs(&err)));
    }

    /// Reports what checking the open map against its files found.
    pub fn map_reopened(&mut self, reopened: Result<Reopened, MapError>) {
        let reopened = match reopened {
            Ok(reopened) => reopened,
            Err(err) => return self.notify(ui_errors::map::map(&err), ERROR),
        };
        if !reopened.reloaded.is_empty() {
            let files = file_list(reopened.reloaded.iter().copied());
            let key = if reopened.history_trimmed {
                "workspace-changed-on-disk-reloaded-undo-steps-dropped"
            } else {
                "maps-reloaded-files"
            };
            self.notify(
                ui_text::get_with(key, &[("value", Arg::Text(&files))]),
                PALE,
            );
        }
        if !reopened.conflicts.is_empty() {
            let (dialog, target) = map_conflict_dialog(reopened.conflicts, None, true);
            self.modal = Some(Modal { dialog, target });
        }
    }

    /// Reports how opening or reloading map `id` went.
    pub fn map_opened(&mut self, id: &MapId, result: Result<(), MapError>, reloaded: bool) {
        match result {
            Ok(()) => {
                self.maps.opened(id);
                if reloaded {
                    self.notify(
                        ui_text::get_with("maps-reloaded", &[("id", Arg::Text(&id.to_string()))]),
                        OK,
                    );
                }
            }
            Err(err) => {
                let title =
                    ui_text::get_with("maps-open-failed", &[("id", Arg::Text(&id.to_string()))]);
                self.open_message(&title, vec![ui_errors::map::map(&err)]);
            }
        }
    }

    /// Reports how creating map `id` went. A failure stays in the window.
    /// `opened` is whether the new map is now the open one.
    pub fn map_created(&mut self, id: &MapId, result: Result<bool, String>) {
        match result {
            Ok(opened) => {
                self.maps.close_create();
                if opened {
                    self.maps.opened(id);
                }
                self.notify(
                    ui_text::get_with("maps-created", &[("id", Arg::Text(&id.to_string()))]),
                    OK,
                );
                self.maps.mark_stale();
            }
            Err(message) => self.maps.create_failed(message),
        }
    }

    /// Reports what saving the open map did.
    pub fn map_saved(&mut self, result: Result<Saved, SaveError>) {
        match result {
            Ok(Saved::Written(written)) => {
                let files = file_list(written.iter().map(|saved| saved.file));
                self.notify(
                    ui_text::get_with("maps-saved", &[("files", Arg::Text(&files))]),
                    OK,
                );
            }
            Ok(Saved::NothingToSave) => {
                self.notify(ui_text::get("maps-nothing-to-save"), PALE);
            }
            Err(SaveError { written, cause }) => {
                let names: Vec<String> = written
                    .iter()
                    .map(|saved| saved.file.file_name().to_owned())
                    .collect();
                let saved = saved_before_stopping(&names);
                match cause {
                    SaveFailure::Conflict(conflicts) => {
                        let (dialog, target) = map_conflict_dialog(conflicts, saved, true);
                        self.modal = Some(Modal { dialog, target });
                    }
                    SaveFailure::Map(err) => {
                        let mut lines = vec![ui_errors::map::map(&err)];
                        lines.extend(saved);
                        self.open_error(&ui_text::get("system-save-failed"), lines);
                    }
                }
            }
        }
    }

    /// Reports that the open map was named `name`.
    pub fn map_renamed(&mut self, name: &str) {
        self.notify(
            ui_text::get_with("maps-renamed", &[("name", Arg::Text(name))]),
            OK,
        );
    }

    /// Asks whether to delete map `id`, whose files hold `seen`, or shows
    /// why they could not be read. `modified` is whether it is the open map
    /// and has unsaved edits.
    pub fn map_delete_prompt(
        &mut self,
        id: &MapId,
        result: Result<MapFiles, MapError>,
        modified: bool,
    ) {
        match result {
            Ok(seen) => {
                let (dialog, target) = map_delete_dialog(id, seen, modified);
                self.modal = Some(Modal { dialog, target });
            }
            Err(err) => {
                let title = ui_text::get("maps-delete-title");
                self.open_message(&title, vec![ui_errors::map::map(&err)]);
            }
        }
    }

    /// Reports how deleting map `id` went.
    pub fn map_deleted(&mut self, id: &MapId, result: Result<(), MapDeleteError>) {
        self.maps.mark_stale();
        match result {
            Ok(()) => {
                self.maps.deleted(id);
                self.notify(
                    ui_text::get_with("maps-deleted", &[("id", Arg::Text(&id.to_string()))]),
                    OK,
                );
            }
            Err(err) => {
                let title = ui_text::get("maps-delete-title");
                self.modal = Some(Modal {
                    dialog: message_dialog(&title, vec![ui_errors::map::delete(&err)]),
                    target: crate::system::workspace::Target::None,
                });
            }
        }
    }
}
