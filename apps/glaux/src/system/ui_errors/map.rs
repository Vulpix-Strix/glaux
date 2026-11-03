use crate::cartridge::map::{MAP_MAX_SIZE, MapError};
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
