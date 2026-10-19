//! Shared error presentation for shell and workspace surfaces.

pub(super) mod code;
pub(super) mod runtime;
pub(super) mod video;

use crate::ui_text;
use glaux_i18n::Arg;

pub(super) fn fs(error: &crate::cartridge_fs::FsError) -> String {
    use crate::cartridge_fs::FsError;
    match error {
        FsError::NotFound(value) => {
            ui_text::get_with("error-fs-not-found", &[("path", Arg::Text(value))])
        }
        FsError::NotADirectory(value) => {
            ui_text::get_with("error-fs-not-a-directory", &[("path", Arg::Text(value))])
        }
        FsError::NotAFile(value) => {
            ui_text::get_with("error-fs-not-a-file", &[("path", Arg::Text(value))])
        }
        FsError::Unreadable(value) => {
            ui_text::get_with("error-fs-unreadable", &[("path", Arg::Text(value))])
        }
        FsError::AlreadyExists(value) => {
            ui_text::get_with("error-fs-already-exists", &[("path", Arg::Text(value))])
        }
        FsError::Unwritable(value) => {
            ui_text::get_with("error-fs-unwritable", &[("path", Arg::Text(value))])
        }
        FsError::ReadOnly(value) => {
            ui_text::get_with("error-fs-read-only", &[("path", Arg::Text(value))])
        }
        FsError::Unavailable => ui_text::get("error-fs-unavailable"),
    }
}

pub(super) fn cartridge(error: &crate::cartridge::CartridgeError) -> String {
    use crate::cartridge::CartridgeError;
    match error {
        CartridgeError::Fs(value) => fs(value),
        CartridgeError::MissingManifest(value) => ui_text::get_with(
            "error-cartridge-missing-manifest",
            &[("directory", Arg::Text(value))],
        ),
        CartridgeError::LegacyManifest => ui_text::get("error-cartridge-legacy-manifest"),
        CartridgeError::ManifestNotAFile => ui_text::get("error-cartridge-manifest-not-a-file"),
        CartridgeError::ManifestUnreadable => ui_text::get("error-cartridge-manifest-unreadable"),
        CartridgeError::Malformed { line, reason } => ui_text::get_with(
            "error-cartridge-malformed",
            &[
                ("line", Arg::Text(&line.to_string())),
                ("reason", Arg::Text(reason)),
            ],
        ),
        CartridgeError::UnsupportedFormat(value) => ui_text::get_with(
            "error-cartridge-unsupported-format",
            &[("format", Arg::Text(&value.to_string()))],
        ),
        CartridgeError::MissingField(value) => ui_text::get_with(
            "error-cartridge-missing-field",
            &[("field", Arg::Text(value))],
        ),
        CartridgeError::UnknownField(value) => ui_text::get_with(
            "error-cartridge-unknown-field",
            &[("field", Arg::Text(value))],
        ),
        CartridgeError::UnsupportedLanguage(value) => ui_text::get_with(
            "error-cartridge-unsupported-language",
            &[("language", Arg::Text(value))],
        ),
        CartridgeError::UnsupportedModel(value) => ui_text::get_with(
            "error-cartridge-unsupported-model",
            &[("model", Arg::Text(value))],
        ),
        CartridgeError::UnsupportedGraphics(value) => ui_text::get_with(
            "error-cartridge-unsupported-graphics",
            &[("profile", Arg::Text(value))],
        ),
        CartridgeError::EmptyName => ui_text::get("error-cartridge-empty-name"),
        CartridgeError::InvalidVersion => ui_text::get("error-cartridge-invalid-version"),
        CartridgeError::InvalidMinGlaux => ui_text::get("error-cartridge-invalid-min-glaux"),
        CartridgeError::RequiresGlaux { required, running } => ui_text::get_with(
            "error-cartridge-requires-glaux",
            &[
                ("required", Arg::Text(&required.to_string())),
                ("running", Arg::Text(&running.to_string())),
            ],
        ),
        CartridgeError::InvalidEntryPath => ui_text::get("error-cartridge-invalid-entry-path"),
        CartridgeError::EntryNotLuau => ui_text::get("error-cartridge-entry-not-luau"),
        CartridgeError::EntryNotRhai => ui_text::get("error-cartridge-entry-not-rhai"),
        CartridgeError::EntryNotFound(value) => ui_text::get_with(
            "error-cartridge-entry-not-found",
            &[("path", Arg::Text(value))],
        ),
        CartridgeError::EntryNotAFile(value) => ui_text::get_with(
            "error-cartridge-entry-not-a-file",
            &[("path", Arg::Text(value))],
        ),
        CartridgeError::EntryUnreadable(value) => ui_text::get_with(
            "error-cartridge-entry-unreadable",
            &[("path", Arg::Text(value))],
        ),
        CartridgeError::AssetNotAFile(value) => ui_text::get_with(
            "error-cartridge-asset-not-a-file",
            &[("asset", Arg::Text(&value.to_string()))],
        ),
        CartridgeError::AssetUnreadable(value) => ui_text::get_with(
            "error-fs-unreadable",
            &[("path", Arg::Text(&value.to_string()))],
        ),
        CartridgeError::AssetUnwritable(value) => ui_text::get_with(
            "error-fs-unwritable",
            &[("path", Arg::Text(&value.to_string()))],
        ),
        CartridgeError::AssetReadOnly(value) => ui_text::get_with(
            "error-cartridge-asset-read-only",
            &[("asset", Arg::Text(&value.to_string()))],
        ),
    }
}

#[cfg(test)]
mod tests;
