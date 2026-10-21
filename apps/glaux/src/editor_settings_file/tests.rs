use super::*;
use crate::scratch_dir::ScratchDir;
use crate::system::IndentStyle;
use pretty_assertions::assert_eq;

#[test]
fn saved_settings_load_back() {
    let dir = ScratchDir::new("editor-settings-round-trip");
    let settings = EditorSettings {
        word_wrap: true,
        tab_size: 2,
        line_numbers: false,
        ligatures: false,
        indent: IndentStyle::Spaces,
        indent_size: 3,
        save_before_running: false,
    };
    save(&dir.join("nested"), &settings).unwrap();
    assert_eq!(load(&dir.join("nested")), settings);
}

#[test]
fn a_missing_or_malformed_file_gives_the_defaults() {
    let dir = ScratchDir::new("editor-settings-bad");
    assert_eq!(load(&dir), EditorSettings::default());
    fs::write(path(&dir), "{ not json").unwrap();
    assert_eq!(load(&dir), EditorSettings::default());
}

#[test]
fn an_out_of_range_tab_size_falls_back_alone() {
    let dir = ScratchDir::new("editor-settings-tab");
    fs::write(path(&dir), r#"{ "word_wrap": true, "tab_size": 0 }"#).unwrap();
    assert_eq!(
        load(&dir),
        EditorSettings {
            word_wrap: true,
            ..EditorSettings::default()
        }
    );
}

#[test]
fn an_out_of_range_indent_size_falls_back_alone() {
    let dir = ScratchDir::new("editor-settings-indent");
    fs::write(path(&dir), r#"{ "indent": "tabs", "indent_size": 99 }"#).unwrap();
    assert_eq!(
        load(&dir),
        EditorSettings {
            indent: IndentStyle::Tabs,
            ..EditorSettings::default()
        }
    );
}
