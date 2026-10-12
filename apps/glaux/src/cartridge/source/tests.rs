use super::*;
use pretty_assertions::assert_eq;

#[test]
fn language_comes_from_the_extension() {
    for (path, language) in [
        ("code/main.luau", Language::Luau),
        ("code/ui/hud.rhai", Language::Rhai),
        ("main.luau", Language::Luau),
        ("code/v1.2.rhai", Language::Rhai),
    ] {
        let parsed = SourcePath::parse(path).unwrap();
        assert_eq!(parsed.language(), language, "{path}");
        assert_eq!(parsed.as_str(), path);
    }
}

#[test]
fn rejects_files_that_are_not_luau_or_rhai() {
    for path in [
        "code/main.lua",
        "code/notes.txt",
        "code/main",
        "code/main.LUAU",
        "code/.luau",
        "code/main.luau.bak",
        "code/mainluau",
    ] {
        assert_eq!(
            SourcePath::parse(path),
            Err(SourcePathError::UnsupportedExtension(path.into())),
            "{path}"
        );
    }
}

#[test]
fn rejects_paths_that_are_not_portable_and_relative() {
    for path in [
        "",
        "/code/main.luau",
        "code/../main.luau",
        "../main.luau",
        "./main.luau",
        "code//main.luau",
        "code/",
        "code\\main.luau",
        "c:main.luau",
        "code/ma\0in.luau",
    ] {
        assert_eq!(
            SourcePath::parse(path),
            Err(SourcePathError::Invalid(path.into())),
            "{path:?}"
        );
    }
}
