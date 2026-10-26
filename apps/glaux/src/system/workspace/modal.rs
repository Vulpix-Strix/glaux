//! What the workspace asks in a modal dialog, and the advice the dialogs carry.

use super::answers::phrase;
use super::dialog::{Dialog, DialogLayout};
use super::{Rect, Route, Workspace};
use crate::cartridge::CartridgeError;
use crate::cartridge::source::{SourceEditError, SourcePath};
use crate::cartridge_fs::FsError;
use crate::system::code_document::{DocumentId, SourceConflict, SourceConflictKind};
use crate::system::sprite_document::{Conflict, ConflictKind, DiskAsset};
use crate::ui_text;
use glaux_i18n::Arg;
use glaux_machine::framebuffer::{HEIGHT, WIDTH};

/// What a dialog's buttons do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::system) enum Choice {
    /// Save unsaved edits, then go on with what asked.
    Save,
    /// Drop unsaved edits and go on with what asked.
    Discard,
    Overwrite,
    Reload,
    /// Delete the script's file.
    Delete,
    Cancel,
    /// Close a message that offers no decision.
    Dismiss,
}

/// The open dialog and what its choice acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Modal {
    pub(super) dialog: Dialog<Choice>,
    pub(super) target: Target,
}

/// What a dialog's choice acts on, as the dialog showed it: what Overwrite
/// may replace, Reload reads again, or Save and Discard close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::system) enum Target {
    /// The dialog acts on nothing.
    None,
    /// Asset files of the sprite document.
    Assets(Vec<DiskAsset>),
    /// The file of one script document, holding `seen` (`None` means absent).
    Script {
        id: DocumentId,
        seen: Option<Vec<u8>>,
    },
    /// The tab of a script document with unsaved edits.
    CloseScript(DocumentId),
    /// The file of a script document to delete, holding `seen` when the
    /// dialog was shown (`None` means absent).
    DeleteScript {
        id: DocumentId,
        seen: Option<Vec<u8>>,
    },
}

/// What to tell the user when saving an asset file failed.
pub(in crate::system) fn write_advice(err: &CartridgeError) -> String {
    match err {
        CartridgeError::AssetReadOnly(asset) => ui_text::get_with(
            "workspace-is-read-only-make-it-writable-then-save-again-2",
            &[("path", Arg::Text(&asset.to_string()))],
        ),
        CartridgeError::AssetUnwritable(asset) => ui_text::get_with(
            "workspace-glaux-could-not-write-check-the-cartridge-folder-s-permissions-an-2",
            &[("path", Arg::Text(&asset.to_string()))],
        ),
        CartridgeError::AssetUnreadable(asset) => ui_text::get_with(
            "workspace-glaux-could-not-read-to-check-it-for-outside-changes-check-its-pe-2",
            &[("path", Arg::Text(&asset.to_string()))],
        ),
        CartridgeError::AssetNotAFile(asset) => ui_text::get_with(
            "workspace-is-not-a-regular-file-move-it-aside-then-save-again-2",
            &[("path", Arg::Text(&asset.to_string()))],
        ),
        other => crate::system::ui_errors::cartridge(other),
    }
}

/// What to tell the user when saving the script at `path` failed.
pub(in crate::system) fn source_write_advice(path: &SourcePath, err: &FsError) -> String {
    match err {
        FsError::ReadOnly(_) => ui_text::get_with(
            "workspace-is-read-only-make-it-writable-then-save-again-2",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        FsError::NotAFile(_) => ui_text::get_with(
            "workspace-is-not-a-regular-file-move-it-aside-then-save-again-2",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        FsError::Unreadable(_) => ui_text::get_with(
            "workspace-glaux-could-not-read-to-check-it-for-outside-changes-check-its-pe-2",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        FsError::NotFound(_) | FsError::NotADirectory(_) => ui_text::get_with(
            "workspace-save-missing-folder",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        FsError::Unwritable(_) | FsError::AlreadyExists(_) => ui_text::get_with(
            "workspace-glaux-could-not-write-check-the-cartridge-folder-s-permissions-an-2",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        FsError::Unavailable => ui_text::get("workspace-cartridge-storage-is-unavailable"),
    }
}

/// Asks about unsaved edits before the pending command or exit.
/// Cancel is focused so a stray Enter loses nothing.
pub(in crate::system) fn unsaved_dialog(
    names: &[String],
    pending: &super::super::unsaved::Pending,
) -> Dialog<Choice> {
    use super::super::unsaved::Pending;
    let count = Arg::Number(names.len() as f64);
    let instructions = match pending {
        Pending::Command { action, .. } => ui_text::get_with(
            "workspace-unsaved-command",
            &[("count", count), ("action", Arg::Text(action))],
        ),
        Pending::Exit => ui_text::get_with("workspace-unsaved-exit", &[("count", count)]),
    };
    Dialog::new(
        ui_text::get("workspace-unsaved-title"),
        vec![
            ui_text::get_with(
                "workspace-unsaved-names",
                &[("names", Arg::Text(&phrase(names))), ("count", count)],
            ),
            instructions,
        ],
        vec![
            (&ui_text::get("workspace-save-all"), Choice::Save),
            (&ui_text::get("workspace-discard"), Choice::Discard),
            (&ui_text::get("workspace-cancel"), Choice::Cancel),
        ],
        2,
        Some(Choice::Cancel),
    )
}

/// Asks whether to save the unsaved edits to the script at `path` before
/// closing its tab. Cancel is focused so a stray Enter loses nothing.
pub(super) fn close_dialog(path: &SourcePath) -> Dialog<Choice> {
    Dialog::new(
        ui_text::get("workspace-unsaved-title"),
        vec![
            ui_text::get_with(
                "workspace-has-unsaved-changes",
                &[("path", Arg::Text(&path.to_string()))],
            ),
            ui_text::get("workspace-close-unsaved-instructions"),
        ],
        vec![
            (&ui_text::get("workspace-save"), Choice::Save),
            (&ui_text::get("workspace-discard"), Choice::Discard),
            (&ui_text::get("workspace-cancel"), Choice::Cancel),
        ],
        2,
        Some(Choice::Cancel),
    )
}

/// Asks whether to delete the file at `path` of script document `id`.
/// `modified` is whether the document has unsaved edits, `external`
/// whether the file changed outside Glaux, and `seen` what the file holds
/// (`None` when it is gone). Cancel is focused so a stray Enter
/// deletes nothing.
pub(super) fn delete_dialog(
    id: DocumentId,
    path: &SourcePath,
    modified: bool,
    external: bool,
    seen: Option<Vec<u8>>,
) -> (Dialog<Choice>, Target) {
    let mut lines = vec![ui_text::get_with(
        "workspace-delete-from-the-cartridge",
        &[("path", Arg::Text(&path.to_string()))],
    )];
    match &seen {
        None => lines.push(ui_text::get_with(
            "workspace-delete-already-missing",
            &[("path", Arg::Text(&path.to_string()))],
        )),
        Some(_) => lines.push(ui_text::get("workspace-the-file-cannot-be-recovered")),
    }
    if external {
        lines.push(ui_text::get("workspace-delete-external-warning"));
    }
    if modified {
        lines.push(ui_text::get("workspace-delete-unsaved-warning"));
    }
    let dialog = Dialog::new(
        ui_text::get("workspace-delete-script"),
        lines,
        vec![
            (&ui_text::get("workspace-delete"), Choice::Delete),
            (&ui_text::get("workspace-cancel"), Choice::Cancel),
        ],
        1,
        Some(Choice::Cancel),
    )
    .destructive(0);
    (dialog, Target::DeleteScript { id, seen })
}

/// What to tell the user when deleting or moving the script at `path`
/// failed.
pub(super) fn edit_failure(path: &SourcePath, err: &SourceEditError) -> String {
    match err {
        SourceEditError::Fs(FsError::ReadOnly(_)) => ui_text::get_with(
            "workspace-is-read-only-make-it-writable-then-try-again",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        SourceEditError::Fs(FsError::AlreadyExists(existing)) => ui_text::get_with(
            "workspace-already-exists",
            &[("value", Arg::Text(existing.trim_start_matches('/')))],
        ),
        SourceEditError::Fs(FsError::Unwritable(_)) => ui_text::get_with(
            "workspace-edit-write-failed",
            &[("path", Arg::Text(&path.to_string()))],
        ),
        err => crate::system::ui_errors::code::source_edit(err),
    }
}

/// Asks whether to overwrite the file of script document `id` in
/// `conflict` with the document's text or leave both as they are. `saved`,
/// if any, tells what was saved before the conflict was found. Cancel is
/// focused so a stray Enter loses nothing.
pub(in crate::system) fn script_conflict_dialog(
    id: DocumentId,
    conflict: SourceConflict,
    saved: Option<String>,
) -> (Dialog<Choice>, Target) {
    let SourceConflict { path, kind, disk } = conflict;
    let (what, replaces) = match kind {
        SourceConflictKind::Modified => (
            ui_text::get_with(
                "workspace-changed-outside-glaux",
                &[("path", Arg::Text(&path.to_string()))],
            ),
            &ui_text::get("workspace-overwrite-disk-replaces-the-file-with-your-edits"),
        ),
        SourceConflictKind::Deleted => (
            ui_text::get_with(
                "workspace-was-deleted-outside-glaux",
                &[("path", Arg::Text(&path.to_string()))],
            ),
            &ui_text::get("workspace-overwrite-disk-writes-your-edits-to-it-again"),
        ),
    };
    let mut lines = vec![what];
    lines.extend(saved);
    lines.push(ui_text::get("workspace-unsaved-edits-kept"));
    lines.push(replaces.to_owned());
    let dialog = Dialog::new(
        ui_text::get("workspace-file-conflict"),
        lines,
        vec![
            (&ui_text::get("workspace-overwrite-disk"), Choice::Overwrite),
            (&ui_text::get("workspace-cancel"), Choice::Cancel),
        ],
        1,
        Some(Choice::Cancel),
    );
    (dialog, Target::Script { id, seen: disk })
}

/// Asks whether to overwrite the asset files in `conflicts`, discard the
/// changes to them, or leave both as they are. `saved`, if any, tells what
/// was saved before the conflict was found. Cancel is focused so a stray
/// Enter loses nothing.
pub(in crate::system) fn asset_conflict_dialog(
    conflicts: Vec<Conflict>,
    saved: Option<String>,
) -> (Dialog<Choice>, Target) {
    let mut lines: Vec<String> = conflicts
        .iter()
        .map(|conflict| {
            let asset = conflict.asset();
            match conflict.kind {
                ConflictKind::Modified => ui_text::get_with(
                    "workspace-changed-outside-glaux",
                    &[("path", Arg::Text(&asset.to_string()))],
                ),
                ConflictKind::Deleted => ui_text::get_with(
                    "workspace-was-deleted-outside-glaux",
                    &[("path", Arg::Text(&asset.to_string()))],
                ),
                ConflictKind::Created => ui_text::get_with(
                    "workspace-a-new-appeared-outside-glaux",
                    &[("asset", Arg::Text(&asset.to_string()))],
                ),
            }
        })
        .collect();
    lines.extend(saved);
    lines.push(ui_text::get("workspace-unsaved-edits-kept"));
    lines.push(ui_text::get_with(
        "workspace-asset-conflict-instructions",
        &[("count", Arg::Number(conflicts.len() as f64))],
    ));
    let dialog = Dialog::new(
        ui_text::get("workspace-file-conflict"),
        lines,
        vec![
            (&ui_text::get("workspace-overwrite-disk"), Choice::Overwrite),
            (&ui_text::get("workspace-discard-reload"), Choice::Reload),
            (&ui_text::get("workspace-cancel"), Choice::Cancel),
        ],
        2,
        Some(Choice::Cancel),
    );
    let seen = conflicts
        .into_iter()
        .map(|conflict| conflict.disk)
        .collect();
    (dialog, Target::Assets(seen))
}

/// Shows `lines` under `title` until dismissed.
pub(in crate::system) fn message_dialog(title: &str, lines: Vec<String>) -> Dialog<Choice> {
    Dialog::new(
        title,
        lines,
        vec![(&ui_text::get("boot-ok"), Choice::Dismiss)],
        0,
        Some(Choice::Dismiss),
    )
}

impl Workspace {
    pub(super) fn open_conflict(&mut self, conflicts: Vec<Conflict>, saved: Option<String>) {
        let (dialog, target) = asset_conflict_dialog(conflicts, saved);
        self.modal = Some(Modal { dialog, target });
    }

    pub(super) fn open_error(&mut self, title: &str, mut lines: Vec<String>) {
        lines.push(ui_text::get("workspace-your-edits-are-still-in-memory"));
        self.open_message(title, lines);
    }

    /// Shows `lines` under `title` until dismissed.
    pub(super) fn open_message(&mut self, title: &str, lines: Vec<String>) {
        self.modal = Some(Modal {
            dialog: message_dialog(title, lines),
            target: Target::None,
        });
    }

    /// Lays `dialog` out over the views, or over the whole screen when
    /// `whole_screen`.
    pub(in crate::system) fn dialog_layout(
        &self,
        dialog: &Dialog<Choice>,
        whole_screen: bool,
    ) -> DialogLayout {
        let area = if whole_screen {
            Rect {
                x: 0,
                y: 0,
                width: WIDTH as i32,
                height: HEIGHT as i32,
            }
        } else {
            self.layout.view_area()
        };
        dialog.layout(&self.font, area, self.layout.cell_w, self.layout.cell_h)
    }

    /// Whether a dialog, one of the sprite editor's, or one of the Code
    /// view's overlays is open and taking all input.
    pub fn has_modal(&self) -> bool {
        matches!(
            self.route(),
            Route::Modal | Route::SpriteDialog | Route::CodeOverlay
        )
    }

    pub(super) fn open_script_conflict(&mut self, id: DocumentId, conflict: SourceConflict) {
        let (dialog, target) = script_conflict_dialog(id, conflict, None);
        self.modal = Some(Modal { dialog, target });
    }
}
