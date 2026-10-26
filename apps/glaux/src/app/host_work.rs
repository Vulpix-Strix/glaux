//! The work the system asks of the host, carried out between ticks.

use std::path::Path;

use glaux_host_raylib::RaylibHost;

use crate::desktop::external;
use crate::desktop::file_picker::FilePicker;
use crate::system::{Clipboard, HostRequest, PickedFile, System};
use crate::{editor_settings_file, logging, ui_text};

/// The desktop clipboard, as the system reads and writes it.
pub(super) struct HostClipboard<'a>(pub(super) &'a mut RaylibHost);

impl Clipboard for HostClipboard<'_> {
    fn text(&mut self) -> Option<String> {
        self.0.clipboard_text()
    }

    fn set_text(&mut self, text: &str) {
        self.0.set_clipboard_text(text);
    }
}

/// Carries out the work the system asked of the host during its last tick.
pub(super) fn serve_host_requests(
    system: &mut System,
    picker: &mut FilePicker,
    config_dir: Option<&Path>,
) {
    while let Some(request) = system.take_host_request() {
        match request {
            HostRequest::PickPng { directory } => {
                if !picker.open_png(&directory) {
                    system.file_picked(PickedFile::Unavailable(ui_text::get(
                        "desktop-file-dialog-already-open",
                    )));
                }
            }
            HostRequest::OpenExternal(url) => external::open_url(&url),
            HostRequest::SaveEditorSettings(settings) => {
                let Some(dir) = config_dir else {
                    tracing::debug!("no config directory, editor settings not saved");
                    continue;
                };
                match editor_settings_file::save(dir, &settings) {
                    Ok(()) => tracing::debug!(?settings, "editor settings saved"),
                    Err(err) => tracing::warn!(
                        target: logging::DIAGNOSTIC_TARGET,
                        error = %format!("{err:#}"),
                        "failed to save the editor settings"
                    ),
                }
            }
        }
    }
}
