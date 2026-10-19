use crate::ui_text;
use glaux_i18n::Arg;

pub(in crate::system) fn document(
    error: &crate::system::code_document::CodeDocumentError,
) -> String {
    use crate::system::code_document::CodeDocumentError;
    match error {
        CodeDocumentError::NotFound(value) => ui_text::get_with(
            "error-document-not-found",
            &[("path", Arg::Text(value.as_str()))],
        ),
        CodeDocumentError::NotAFile(value) => ui_text::get_with(
            "error-fs-not-a-file",
            &[("path", Arg::Text(value.as_str()))],
        ),
        CodeDocumentError::Unreadable(value) => ui_text::get_with(
            "error-fs-unreadable",
            &[("path", Arg::Text(value.as_str()))],
        ),
        CodeDocumentError::InvalidUtf8 { path, line } => ui_text::get_with(
            "error-document-invalid-utf8",
            &[
                ("path", Arg::Text(path.as_str())),
                ("line", Arg::Text(&line.to_string())),
            ],
        ),
    }
}

pub(in crate::system) fn rename(error: &crate::system::code_document::CodeRenameError) -> String {
    use crate::system::code_document::CodeRenameError;
    match error {
        CodeRenameError::NotOpen => ui_text::get("error-rename-not-open"),
        CodeRenameError::PathTaken(value) => ui_text::get_with(
            "error-rename-path-taken",
            &[("path", Arg::Text(value.as_str()))],
        ),
        CodeRenameError::Language(value) => ui_text::get_with(
            "error-rename-language",
            &[("path", Arg::Text(value.as_str()))],
        ),
    }
}

pub(in crate::system) fn source_edit(error: &crate::cartridge::source::SourceEditError) -> String {
    use crate::cartridge::source::SourceEditError;
    match error {
        SourceEditError::Entry(value) => ui_text::get_with(
            "error-source-edit-entry",
            &[("path", Arg::Text(value.as_str()))],
        ),
        SourceEditError::Language { from, to } => ui_text::get_with(
            "error-source-edit-language",
            &[
                ("from", Arg::Text(from.as_str())),
                ("to", Arg::Text(to.as_str())),
            ],
        ),
        SourceEditError::Changed(value) => ui_text::get_with(
            "error-source-edit-changed",
            &[("path", Arg::Text(value.as_str()))],
        ),
        SourceEditError::Fs(value) => super::fs(value),
    }
}

pub(in crate::system) fn move_script(
    error: &crate::system::code_session::MoveScriptError,
) -> String {
    use crate::system::code_session::MoveScriptError;
    match error {
        MoveScriptError::Document(value) => rename(value),
        MoveScriptError::Changed(value) => ui_text::get_with(
            "error-move-script-changed",
            &[("path", Arg::Text(value.as_str()))],
        ),
        MoveScriptError::Missing(value) => ui_text::get_with(
            "error-move-script-missing",
            &[("path", Arg::Text(value.as_str()))],
        ),
        MoveScriptError::File(value) => source_edit(value),
    }
}

pub(in crate::system) fn source_path(error: &crate::cartridge::source::SourcePathError) -> String {
    use crate::cartridge::source::SourcePathError;
    match error {
        SourcePathError::Invalid(value) => {
            ui_text::get_with("error-source-path-invalid", &[("path", Arg::Text(value))])
        }
        SourcePathError::UnsupportedExtension(value) => ui_text::get_with(
            "error-source-path-unsupported-extension",
            &[("path", Arg::Text(value))],
        ),
    }
}
