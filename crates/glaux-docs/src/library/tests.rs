use pretty_assertions::assert_eq;

use glaux_i18n::{FallbackChain, Language};

use super::*;
use crate::{BlockKind, Inline};

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

fn graphics_library() -> Library {
    library(&[
        (
            "api/graphics.ron",
            r#"#![enable(implicit_some)]
[
    (id: "api/graphics/spr", kind: Function, luau: (name: "spr"), rhai: (name: "spr")),
    (id: "api/graphics/sspr", kind: Function, luau: (name: "sspr"), rhai: (name: "sspr")),
    (id: "api/graphics/text", kind: Function, luau: (name: "print"), rhai: (name: "text")),
]"#,
        ),
        (
            "api/audio.ron",
            r#"#![enable(implicit_some)]
[
    (id: "api/audio/tone", kind: Function, luau: (path: ["audio"], name: "tone"), rhai: (path: ["audio"], name: "tone")),
]"#,
        ),
        (
            "en/api/graphics/spr.md",
            "# spr\n\nDraws a block of tiles.\n",
        ),
        (
            "en/api/graphics/sspr.md",
            "# sspr\n\nDraws part of a sheet.\n",
        ),
        (
            "en/api/graphics/text.md",
            "# text\n\nDraws text.\n\n## Colors\n",
        ),
        ("en/api/audio/tone.md", "# tone\n\nPlays a spr-like beep.\n"),
        ("en/guide/sprites.md", "# Sprites\n\nHow tiles work.\n"),
    ])
}

fn search(library: &Library, query: &str) -> Vec<(String, Rank)> {
    let chain = FallbackChain::new(Language::english());
    library
        .search(query, &chain)
        .into_iter()
        .map(|hit| (hit.id.to_string(), hit.rank))
        .collect()
}

#[test]
fn search_ranks_prefix_before_substring_before_heading_and_summary() {
    let library = graphics_library();
    assert_eq!(
        search(&library, "spr"),
        [
            ("api/graphics/spr".to_owned(), Rank::Prefix),
            ("guide/sprites".to_owned(), Rank::Prefix),
            ("api/graphics/sspr".to_owned(), Rank::Substring),
            ("api/audio/tone".to_owned(), Rank::Summary),
        ]
    );
    assert_eq!(
        search(&library, "colors"),
        [("api/graphics/text".to_owned(), Rank::Heading)]
    );
}

#[test]
fn search_ignores_case_and_surrounding_space() {
    let library = graphics_library();
    assert_eq!(search(&library, "  SPRITES "), search(&library, "sprites"));
    assert_eq!(search(&library, "sprites").len(), 1);
}

#[test]
fn an_empty_query_finds_nothing() {
    let library = graphics_library();
    assert_eq!(search(&library, ""), []);
    assert_eq!(search(&library, "   "), []);
}

#[test]
fn api_pages_answer_to_the_name_each_language_spells() {
    let library = graphics_library();
    assert_eq!(
        search(&library, "print"),
        [("api/graphics/text".to_owned(), Rank::Prefix)]
    );
    assert_eq!(
        search(&library, "audio::tone"),
        [("api/audio/tone".to_owned(), Rank::Prefix)]
    );
    assert_eq!(
        search(&library, "audio.t"),
        [("api/audio/tone".to_owned(), Rank::Prefix)]
    );
}

#[test]
fn search_reads_the_first_language_that_has_each_page() {
    let library = library(&[
        ("en/guide/start.md", "# Start\n\nBegin here.\n"),
        ("en/guide/other.md", "# Other\n\nStart again.\n"),
        ("es/guide/start.md", "# Inicio\n\nEmpieza aqui.\n"),
    ]);
    let chain = FallbackChain::new(spanish());
    let hits = library.search("inicio", &chain);
    assert_eq!(hits.len(), 1);
    assert_eq!(
        (hits[0].title.as_str(), hits[0].is_fallback),
        ("Inicio", false)
    );
    // The Spanish page replaces the English one, so its English title no
    // longer matches, while a page only English has still does.
    let hits = library.search("start", &chain);
    assert_eq!(
        hits.iter()
            .map(|hit| (hit.id.to_string(), hit.is_fallback))
            .collect::<Vec<_>>(),
        [("guide/other".to_owned(), true)]
    );
}

#[test]
fn every_embedded_page_has_blocks_that_keep_its_links() {
    let library = Library::embedded();
    for language in library.languages() {
        for page in library.pages(&language) {
            assert!(!page.blocks().is_empty(), "{}", page.id());
            let mut shown = Vec::new();
            for block in page.blocks() {
                collect_links(&block.kind, &mut shown);
            }
            let mut written: Vec<_> = page.links().iter().map(|l| l.target.clone()).collect();
            shown.sort();
            written.sort();
            assert_eq!(shown, written, "{}", page.id());
        }
    }
}

fn collect_links(kind: &BlockKind, out: &mut Vec<String>) {
    fn inlines(content: &[Inline], out: &mut Vec<String>) {
        for inline in content {
            match inline {
                Inline::Link { target, children } => {
                    out.push(target.clone());
                    inlines(children, out);
                }
                Inline::Strong(children) | Inline::Emph(children) => inlines(children, out),
                Inline::Text(_) | Inline::Code(_) | Inline::Break => {}
            }
        }
    }
    match kind {
        BlockKind::Heading {
            inlines: content, ..
        }
        | BlockKind::Paragraph(content) => {
            inlines(content, out);
        }
        BlockKind::List { items, .. } => {
            for item in items {
                for kind in item {
                    collect_links(kind, out);
                }
            }
        }
        BlockKind::Table { header, rows } => {
            for cell in header.iter().chain(rows.iter().flatten()) {
                inlines(cell, out);
            }
        }
        BlockKind::Alert { blocks, .. } | BlockKind::Quote(blocks) => {
            for kind in blocks {
                collect_links(kind, out);
            }
        }
        BlockKind::Code { .. } | BlockKind::Rule => {}
    }
}
