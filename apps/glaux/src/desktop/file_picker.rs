//! The host's native file dialog, shown beside the Glaux window without
//! holding up its loop where the platform allows.
//!
//! The dialog belongs to the host. What it returns is a host path, which
//! only the system's import reads. Cartridges never reach it.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::system::PickedFile;
use crate::ui_text;

/// A native dialog choosing one PNG file, at most one open at a time.
#[derive(Debug, Default)]
pub struct FilePicker {
    /// Where the open dialog's answer arrives.
    pending: Option<Receiver<Option<PathBuf>>>,
}

impl FilePicker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Shows a dialog choosing one PNG file, opening in `directory`.
    /// Returns false, showing nothing, while an earlier dialog has not
    /// answered.
    ///
    /// On macOS the dialog must run on the main thread, so this returns
    /// once it closes. The panel keeps the window's events flowing
    /// meanwhile. Elsewhere it runs on a thread of its own, so the window
    /// keeps redrawing and answering the compositor while it is open.
    pub fn open_png(&mut self, directory: &Path) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let dialog = rfd::FileDialog::new()
            .set_title(ui_text::get("desktop-import-spritesheet"))
            .add_filter(ui_text::get("desktop-png-image"), &["png", "PNG"])
            .set_directory(directory);
        let (answer, pending) = mpsc::channel();
        if cfg!(target_os = "macos") {
            // The receiver is held right here, so the send cannot fail.
            let _ = answer.send(dialog.pick_file());
        } else {
            std::thread::spawn(move || {
                // The picker may be gone by the time the user answers, in
                // which case the answer is simply dropped.
                let _ = answer.send(dialog.pick_file());
            });
        }
        self.pending = Some(pending);
        true
    }

    /// The open dialog's answer, once it has one.
    ///
    /// rfd answers `None` both when the user cancels and when no dialog
    /// could be shown (on Linux, neither an XDG desktop portal nor zenity
    /// is available), so both read as [`PickedFile::Cancelled`].
    pub fn poll(&mut self) -> Option<PickedFile> {
        let answer = match self.pending.as_ref()?.try_recv() {
            Ok(Some(path)) => PickedFile::Chosen(path),
            Ok(None) => {
                tracing::info!("file dialog closed without a file");
                PickedFile::Cancelled
            }
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                tracing::error!("file dialog thread ended without an answer");
                PickedFile::Unavailable(ui_text::get(
                    "desktop-the-file-dialog-stopped-unexpectedly",
                ))
            }
        };
        self.pending = None;
        Some(answer)
    }
}
