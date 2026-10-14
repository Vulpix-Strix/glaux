//! The Code view's settings, kept across launches in Glaux's config
//! directory.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::config_file;
use crate::system::{EditorSettings, INDENT_SIZES, TAB_SIZES};

/// File name of the saved settings inside the config directory.
const FILE_NAME: &str = "editor-settings.json";

/// Path of the saved settings inside `config_dir`.
fn path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE_NAME)
}

/// Loads the saved settings from `config_dir`.
///
/// A missing, unreadable or malformed file is logged and gives the
/// defaults, and a tab or indentation size out of range gives its default, so
/// the file never stops Glaux from starting.
pub fn load(config_dir: &Path) -> EditorSettings {
    let path = path(config_dir);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            tracing::debug!(path = %path.display(), "no saved editor settings");
            return EditorSettings::default();
        }
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "failed to read the saved editor settings, using defaults");
            return EditorSettings::default();
        }
    };
    let mut settings: EditorSettings = match serde_json::from_str(&text) {
        Ok(settings) => settings,
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "saved editor settings are malformed, using defaults");
            return EditorSettings::default();
        }
    };
    if !TAB_SIZES.contains(&settings.tab_size) {
        tracing::warn!(
            path = %path.display(),
            tab_size = settings.tab_size,
            "saved tab size is out of range, using the default"
        );
        settings.tab_size = EditorSettings::default().tab_size;
    }
    if !INDENT_SIZES.contains(&settings.indent_size) {
        tracing::warn!(
            path = %path.display(),
            indent_size = settings.indent_size,
            "saved indentation size is out of range, using the default"
        );
        settings.indent_size = EditorSettings::default().indent_size;
    }
    tracing::debug!(?settings, "loaded saved editor settings");
    settings
}

/// Writes `settings` to `config_dir`, creating the directory if needed.
pub fn save(config_dir: &Path, settings: &EditorSettings) -> anyhow::Result<()> {
    let text =
        serde_json::to_string_pretty(settings).context("failed to encode the editor settings")?;
    config_file::write_atomically(&path(config_dir), &text)
}

#[cfg(test)]
mod tests;
