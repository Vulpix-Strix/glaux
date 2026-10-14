use super::*;
use pretty_assertions::assert_eq;

#[test]
fn missing_fields_take_their_defaults_and_unknown_ones_are_ignored() {
    let settings: EditorSettings =
        serde_json::from_str(r#"{ "word_wrap": true, "theme": "dark" }"#).unwrap();
    assert_eq!(
        settings,
        EditorSettings {
            word_wrap: true,
            ..EditorSettings::default()
        }
    );
}

#[test]
fn settings_survive_a_round_trip() {
    let settings = EditorSettings {
        word_wrap: true,
        tab_size: 2,
        line_numbers: false,
        ligatures: false,
        indent: IndentStyle::Tabs,
        indent_size: 2,
    };
    let text = serde_json::to_string(&settings).unwrap();
    assert_eq!(
        serde_json::from_str::<EditorSettings>(&text).unwrap(),
        settings
    );
}

#[test]
fn stepping_the_tab_size_stays_within_range() {
    let mut settings = EditorSettings::default();
    settings.step_tab_size(1);
    assert_eq!(settings.tab_size, 5);
    for _ in 0..10 {
        settings.step_tab_size(1);
    }
    assert_eq!(settings.tab_size, *TAB_SIZES.end());
    for _ in 0..10 {
        settings.step_tab_size(-1);
    }
    assert_eq!(settings.tab_size, *TAB_SIZES.start());
}

#[test]
fn the_indent_style_is_written_by_name() {
    let settings: EditorSettings = serde_json::from_str(r#"{ "indent": "spaces" }"#).unwrap();
    assert_eq!(settings.indent, IndentStyle::Spaces);
}

#[test]
fn stepping_the_indent_style_wraps_around() {
    assert_eq!(IndentStyle::Detect.stepped(1), IndentStyle::Spaces);
    assert_eq!(IndentStyle::Tabs.stepped(1), IndentStyle::Detect);
    assert_eq!(IndentStyle::Detect.stepped(-1), IndentStyle::Tabs);
}

#[test]
fn stepping_the_indent_size_stays_within_range() {
    let mut settings = EditorSettings::default();
    for _ in 0..10 {
        settings.step_indent_size(1);
    }
    assert_eq!(settings.indent_size, *INDENT_SIZES.end());
    for _ in 0..10 {
        settings.step_indent_size(-1);
    }
    assert_eq!(settings.indent_size, *INDENT_SIZES.start());
}
