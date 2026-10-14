use super::*;
use pretty_assertions::assert_eq;

fn path(typed: &str, language: Language) -> Result<String, String> {
    path_for(typed, language).map(|path| path.as_str().to_owned())
}

#[test]
fn the_manifest_language_decides_the_extension() {
    assert_eq!(
        path("player", Language::Luau),
        Ok("code/player.luau".into())
    );
    assert_eq!(
        path("player", Language::Rhai),
        Ok("code/player.rhai".into())
    );
}

#[test]
fn nested_paths_stay_under_code() {
    assert_eq!(
        path("enemies/bosses/owl", Language::Luau),
        Ok("code/enemies/bosses/owl.luau".into())
    );
    // Dots inside a name are kept; only the extension is added.
    assert_eq!(path("v1.2", Language::Luau), Ok("code/v1.2.luau".into()));
}

#[test]
fn a_typed_extension_is_refused_rather_than_doubled() {
    assert!(
        path("player.luau", Language::Luau)
            .unwrap_err()
            .contains("Leave out .luau")
    );
    assert!(
        path("player.rhai", Language::Luau)
            .unwrap_err()
            .contains("This cartridge is luau")
    );
}

#[test]
fn paths_that_leave_code_or_are_not_portable_are_refused() {
    for typed in [
        "", "../main", "a//b", "/abs", "a\\b", "c:x", "a/./b", "dir/",
    ] {
        assert!(path(typed, Language::Luau).is_err(), "{typed:?}");
    }
}

#[test]
fn creating_asks_only_for_a_valid_path_and_a_failure_shows_until_edited() {
    let mut dialog = NewFile::new(Language::Luau);
    let layout = dialog.prompt().layout(
        crate::system::workspace::Rect {
            x: 0,
            y: 16,
            width: 320,
            height: 200,
        },
        6,
        12,
    );
    assert_eq!(
        dialog.handle_input(InputEvent::Enter, &layout),
        NewFileAction::None
    );
    for ch in "hud".chars() {
        dialog.handle_input(InputEvent::Text(ch), &layout);
    }
    assert_eq!(dialog.message().0, "Creates code/hud.luau");
    let expected = SourcePath::parse("code/hud.luau").unwrap();
    assert_eq!(
        dialog.handle_input(InputEvent::Enter, &layout),
        NewFileAction::Create(expected)
    );
    dialog.set_failure("Already exists: /code/hud.luau".into());
    assert_eq!(
        dialog.message(),
        ("Already exists: /code/hud.luau".into(), ERROR)
    );
    dialog.handle_input(InputEvent::Text('2'), &layout);
    assert_eq!(dialog.message().0, "Creates code/hud2.luau");
}
