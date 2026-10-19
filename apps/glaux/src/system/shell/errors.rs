use crate::ui_text;
use glaux_i18n::Arg;

pub(super) fn create(error: &crate::cartridge_template::CreateError) -> String {
    use crate::cartridge_template::CreateError;
    match error {
        CreateError::InvalidId(value) => {
            ui_text::get_with("error-create-invalid-id", &[("id", Arg::Text(value))])
        }
        CreateError::EmptyName => ui_text::get("error-create-empty-name"),
        CreateError::InvalidName => ui_text::get("error-create-invalid-name"),
        CreateError::NameTooLong => {
            let max_text_len = crate::cartridge_template::MAX_TEXT_LEN;
            ui_text::get_with(
                "error-create-name-too-long",
                &[("max-text-len", Arg::Text(&max_text_len.to_string()))],
            )
        }
        CreateError::InvalidVersion(value) => ui_text::get_with(
            "error-create-invalid-version",
            &[("version", Arg::Text(value))],
        ),
        CreateError::InvalidAuthor => {
            let max_text_len = crate::cartridge_template::MAX_TEXT_LEN;
            ui_text::get_with(
                "error-create-invalid-author",
                &[("max-text-len", Arg::Text(&max_text_len.to_string()))],
            )
        }
        CreateError::UnsupportedLanguage(value) => ui_text::get_with(
            "error-create-unsupported-language",
            &[("language", Arg::Text(value))],
        ),
        CreateError::Exists(value) => {
            ui_text::get_with("error-create-exists", &[("path", Arg::Text(value))])
        }
        CreateError::Fs(value) => crate::system::ui_errors::fs(value),
        CreateError::Invalid(value) => crate::system::ui_errors::cartridge(value),
    }
}

pub(super) fn option(error: &crate::system::shell::parser::OptionError) -> String {
    use crate::system::shell::parser::OptionError;
    match error {
        OptionError::Unknown(value) => {
            ui_text::get_with("error-option-unknown", &[("option", Arg::Text(value))])
        }
        OptionError::MissingValue(value) => ui_text::get_with(
            "error-option-missing-value",
            &[("option", Arg::Text(value))],
        ),
        OptionError::Repeated(value) => {
            ui_text::get_with("error-option-repeated", &[("option", Arg::Text(value))])
        }
    }
}

pub(super) fn parse(error: &crate::system::shell::parser::ParseError) -> String {
    use crate::system::shell::parser::ParseError;
    match error {
        ParseError::UnterminatedQuote { quote, .. } => ui_text::get_with(
            "error-parse-unterminated-quote",
            &[("quote", Arg::Text(&quote.to_string()))],
        ),
    }
}
