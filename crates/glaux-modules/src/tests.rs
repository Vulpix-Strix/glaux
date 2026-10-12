use super::*;
use pretty_assertions::assert_eq;

fn luau_path(name: &str) -> (String, String) {
    let path = ModuleName::parse(name).unwrap().path("luau");
    (path.as_str().to_owned(), path.in_root().to_owned())
}

#[test]
fn names_map_to_files_below_the_module_root() {
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    assert_eq!(luau_path("player"), pair("code/player.luau", "player.luau"));
    assert_eq!(luau_path("_util"), pair("code/_util.luau", "_util.luau"));
    assert_eq!(
        luau_path("game.enemies"),
        pair("code/game/enemies.luau", "game/enemies.luau")
    );
    assert_eq!(luau_path("a1.b_2"), pair("code/a1/b_2.luau", "a1/b_2.luau"));
}

#[test]
fn names_outside_the_grammar_are_rejected() {
    use ModuleNameError::*;
    let cases = [
        ("", Empty),
        ("a..b", EmptySegment),
        (".a", EmptySegment),
        ("a.", EmptySegment),
        ("../x", EmptySegment),
        ("/x", InvalidCharacter('/')),
        ("a/b", InvalidCharacter('/')),
        ("a\\b", InvalidCharacter('\\')),
        ("c:x", InvalidCharacter(':')),
        ("Player", InvalidCharacter('P')),
        ("a-b", InvalidCharacter('-')),
        ("a b", InvalidCharacter(' ')),
        ("\0", InvalidCharacter('\0')),
        ("búho", InvalidCharacter('ú')),
        ("1up", LeadingDigit("1up".into())),
        ("game.2d", LeadingDigit("2d".into())),
    ];
    for (name, expected) in cases {
        assert_eq!(ModuleName::parse(name), Err(expected), "{name:?}");
    }
}

#[test]
fn extensions_are_rejected_with_the_extensionless_name() {
    for (name, stem, extension) in [
        ("player.luau", "player", "luau"),
        ("player.lua", "player", "lua"),
        ("player.rhai", "player", "rhai"),
        ("game.enemies.LUAU", "game.enemies", "LUAU"),
    ] {
        let err = ModuleName::parse(name).unwrap_err();
        assert_eq!(
            err,
            ModuleNameError::HasExtension {
                stem: stem.into(),
                extension: extension.into(),
            }
        );
        assert!(
            err.to_string().contains(&format!("use \"{stem}\"")),
            "{err}"
        );
    }
    // Only a final segment reads as an extension.
    assert!(ModuleName::parse("lua.player").is_ok());
    assert!(ModuleName::parse("luau").is_ok());
    assert!(ModuleName::parse("rhai.util").is_ok());
}
