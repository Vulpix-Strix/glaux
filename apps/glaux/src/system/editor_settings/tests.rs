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
