use crate::cartridge::map::{
    MAP_MAX_SIZE, MAX_ID_LEN, MapCreateError, MapDeleteError, MapError, MapIdError,
};
use crate::system::map_document::{SaveFailure, file_list};
use crate::ui_text;
use glaux_i18n::Arg;

pub(in crate::system) fn map(error: &MapError) -> String {
    match error {
        MapError::Malformed { line, reason } => ui_text::get_with(
            "error-map-malformed",
            &[
                ("line", Arg::Text(&line.to_string())),
                ("reason", Arg::Text(reason)),
            ],
        ),
        MapError::UnsupportedFormat(format) => ui_text::get_with(
            "error-map-unsupported-format",
            &[("format", Arg::Text(&format.to_string()))],
        ),
        MapError::MissingField(field) => {
            ui_text::get_with("error-map-missing-field", &[("field", Arg::Text(field))])
        }
        MapError::UnknownField(field) => {
            ui_text::get_with("error-map-unknown-field", &[("field", Arg::Text(field))])
        }
        MapError::BlankName => ui_text::get("error-map-blank-name"),
        MapError::BlankLayerName(layer) => ui_text::get_with(
            "error-map-blank-layer-name",
            &[("layer", Arg::Text(&layer.to_string()))],
        ),
        MapError::InvalidSize { width, height } => ui_text::get_with(
            "error-map-size",
            &[
                ("width", Arg::Text(&width.to_string())),
                ("height", Arg::Text(&height.to_string())),
                ("max", Arg::Text(&MAP_MAX_SIZE.to_string())),
            ],
        ),
        MapError::Layer { layer, source } => ui_text::get_with(
            "error-map-layer",
            &[
                ("layer", Arg::Text(&layer.to_string())),
                ("detail", Arg::Text(&source.to_string())),
            ],
        ),
        MapError::MissingMeta(id) => ui_text::get_with(
            "error-map-missing-meta",
            &[("id", Arg::Text(&id.to_string()))],
        ),
        MapError::NotAFile { id, file } => ui_text::get_with(
            "error-map-not-a-file",
            &[
                ("file", Arg::Text(&file.to_string())),
                ("id", Arg::Text(&id.to_string())),
            ],
        ),
        MapError::Fs(error) => super::fs(error),
    }
}

/// Describes why a map save stopped. A conflict names the files that
/// changed outside Glaux.
pub(in crate::system) fn map_save(failure: &SaveFailure) -> String {
    match failure {
        SaveFailure::Conflict(conflicts) => {
            let files = file_list(conflicts.iter().map(|conflict| conflict.file()));
            ui_text::get_with(
                "workspace-changed-outside-glaux",
                &[("path", Arg::Text(&files))],
            )
        }
        SaveFailure::Map(error) => map(error),
    }
}

pub(in crate::system) fn id(error: &MapIdError) -> String {
    ui_text::get_with(
        "error-map-id-invalid",
        &[
            ("id", Arg::Text(error.id())),
            ("max", Arg::Text(&MAX_ID_LEN.to_string())),
        ],
    )
}

pub(in crate::system) fn create(error: &MapCreateError) -> String {
    match error {
        MapCreateError::AlreadyExists(id) => {
            ui_text::get_with("error-map-exists", &[("id", Arg::Text(&id.to_string()))])
        }
        MapCreateError::Fs(error) => super::fs(error),
    }
}

pub(in crate::system) fn delete(error: &MapDeleteError) -> String {
    match error {
        MapDeleteError::Missing(id) => ui_text::get_with(
            "error-map-delete-missing",
            &[("id", Arg::Text(&id.to_string()))],
        ),
        MapDeleteError::Changed { id, file, .. } => ui_text::get_with(
            "error-map-delete-changed",
            &[
                ("file", Arg::Text(&file.to_string())),
                ("id", Arg::Text(&id.to_string())),
            ],
        ),
        MapDeleteError::UnknownEntries { id, names } => ui_text::get_with(
            "error-map-delete-unknown",
            &[
                ("id", Arg::Text(&id.to_string())),
                ("names", Arg::Text(&names.join(", "))),
            ],
        ),
        MapDeleteError::Map(error) => map(error),
        MapDeleteError::Incomplete { id, source } => ui_text::get_with(
            "error-map-delete-incomplete",
            &[
                ("id", Arg::Text(&id.to_string())),
                ("detail", Arg::Text(&super::fs(source))),
            ],
        ),
    }
}
