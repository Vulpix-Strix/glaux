use pretty_assertions::assert_eq;

use super::*;

fn library(files: &[(&str, &str)]) -> Library {
    Library::from_files(
        files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec())),
    )
}

fn id(text: &str) -> DocId {
    DocId::parse(text).unwrap()
}

fn spanish() -> Language {
    Language::parse("es").unwrap()
}

#[test]
fn pages_are_found_by_language_and_id() {
    let library = library(&[
        ("en/guide/start.md", "# Start\n"),
        ("es/guide/start.md", "# Inicio\n"),
        ("en/api/graphics/spr.md", "# spr\n"),
    ]);
    assert_eq!(library.languages(), [Language::english(), spanish()]);
    assert_eq!(
        library.page_ids(&Language::english()),
        [id("api/graphics/spr"), id("guide/start")].into()
    );
    let page = library.page_in(&spanish(), &id("guide/start")).unwrap();
    assert_eq!(page.title(), Some("Inicio"));
    assert_eq!(library.problems, []);
}

#[test]
fn a_missing_translation_falls_back_and_says_so() {
    let library = library(&[
        ("en/guide/a.md", "# A\n"),
        ("en/guide/b.md", "# B\n"),
        ("es/guide/a.md", "# Á\n"),
    ]);
    let chain = FallbackChain::new(spanish());

    let translated = library.page(&id("guide/a"), &chain).unwrap();
    assert_eq!(
        (translated.page.title(), translated.is_fallback),
        (Some("Á"), false)
    );

    let fallback = library.page(&id("guide/b"), &chain).unwrap();
    assert_eq!(
        (fallback.page.title(), fallback.is_fallback),
        (Some("B"), true)
    );
    assert_eq!(fallback.page.language(), &Language::english());

    assert!(library.page(&id("guide/none"), &chain).is_none());
}

#[test]
fn files_are_classified_without_a_hand_kept_list() {
    let library = library(&[
        (
            "api/audio.ron",
            r#"#![enable(implicit_some)]
[(id: "api/audio/tone", kind: Function, luau: (name: "tone"))]"#,
        ),
        ("ids.lock", "guide/new\n"),
        ("assets/sheet.png", "not really a png"),
        ("README.md", "# Contributor guide\n"),
        ("en/guide/new.md", "# New\n"),
    ]);
    assert_eq!(library.problems, []);
    assert_eq!(library.api().entries().len(), 1);
    assert!(library.ids_lock().unwrap().contains(&id("guide/new")));
    assert!(library.has_file("assets/sheet.png"));
    assert!(library.has_file("en/guide/new.md"));
}

#[test]
fn bad_files_are_reported_and_the_rest_still_loads() {
    let library = Library::from_files([
        ("en/guide/ok.md".to_owned(), b"# Ok\n".to_vec()),
        ("en/Guide/Bad.md".to_owned(), b"# Bad\n".to_vec()),
        ("not a language/x.md".to_owned(), b"# X\n".to_vec()),
        ("en/guide/binary.md".to_owned(), vec![0xff, 0xfe]),
        ("notes.txt".to_owned(), b"stray".to_vec()),
    ]);
    let codes: Vec<_> = library.problems.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        [
            "bad-page-id",
            "bad-language-dir",
            "invalid-utf8",
            "unrecognized-file"
        ]
    );
    assert_eq!(library.pages(&Language::english()).count(), 1);
}

#[test]
fn catalog_diagnostics_name_the_docs_path() {
    let library = library(&[("api/bad.ron", "[(")]);
    assert_eq!(library.problems.len(), 1);
    assert!(library.problems[0].location.starts_with("docs/api/bad.ron"));
}
