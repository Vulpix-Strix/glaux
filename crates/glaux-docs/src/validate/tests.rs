use pretty_assertions::assert_eq;

use super::*;

const TONE_CATALOG: &str = r#"#![enable(implicit_some)]
[(
    id: "api/audio/tone",
    kind: Function,
    luau: (name: "tone"),
    params: [(name: "channel", ty: Int), (name: "freq", ty: Number)],
)]"#;

const TONE_PAGE: &str = "# tone\n\nPlays a tone.\n\n## Parameters\n\n| Name | Meaning |\n|---|---|\n| `channel` | Voice |\n| `freq` | Hertz |\n";

fn library(files: &[(&str, &str)]) -> Library {
    Library::from_files(
        files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec())),
    )
}

/// Codes of the diagnostics, in order, with their locations for context.
fn codes(library: &Library) -> Vec<&'static str> {
    validate(library).iter().map(|d| d.code).collect()
}

fn consistent() -> Vec<(&'static str, &'static str)> {
    vec![
        ("api/audio.ron", TONE_CATALOG),
        ("en/api/audio/tone.md", TONE_PAGE),
        (
            "en/guide/start.md",
            "# Start\n\nSee [tone](glaux://api/audio/tone#x).\n",
        ),
        ("ids.lock", "api/audio/tone\nguide/start\n"),
    ]
}

#[test]
fn a_consistent_library_has_no_diagnostics() {
    assert_eq!(validate(&library(&consistent())), []);
}

#[test]
fn pages_need_a_title_and_a_summary() {
    let mut files = consistent();
    files.push(("en/guide/empty.md", "## Only a subsection\n"));
    files[3].1 = "api/audio/tone\nguide/empty\nguide/start\n";
    assert_eq!(
        codes(&library(&files)),
        ["missing-title", "missing-summary"]
    );
}

#[test]
fn links_must_resolve_to_english_pages() {
    let mut files = consistent();
    files.push((
        "en/guide/links.md",
        "# L\n\n[a](glaux://api/audio/nope) [b](glaux://Bad) [c](other.md) [d](ftp://x) [e](https://example.com) [f](#top) [g]()\n",
    ));
    files[3].1 = "api/audio/tone\nguide/links\nguide/start\n";
    assert_eq!(
        codes(&library(&files)),
        [
            "broken-link",
            "bad-link",
            "relative-link",
            "unsupported-link",
            "empty-link"
        ]
    );
}

#[test]
fn diagnostics_locate_the_link_line() {
    let mut files = consistent();
    files.push((
        "en/guide/links.md",
        "# L\n\nok\n\n[a](glaux://api/audio/nope)\n",
    ));
    files[3].1 = "api/audio/tone\nguide/links\nguide/start\n";
    let found = validate(&library(&files));
    assert_eq!(found[0].location, "docs/en/guide/links.md:5");
}

#[test]
fn images_resolve_relative_to_the_page() {
    let mut files = consistent();
    files.push(("assets/sheet.png", "png"));
    files.push((
        "en/guide/images.md",
        "# I\n\n![Sheet](../../assets/sheet.png) ![Missing](../../assets/none.png) ![Up](../../../../x.png) ![](../../assets/sheet.png) ![Web](https://example.com/a.png) ![Data](data:image/png;base64,AA)\n",
    ));
    files[3].1 = "api/audio/tone\nguide/images\nguide/start\n";
    assert_eq!(
        codes(&library(&files)),
        [
            "missing-image",
            "bad-image",
            "image-alt",
            "external-image",
            "bad-image"
        ]
    );
}

#[test]
fn raw_html_is_a_warning() {
    let mut files = consistent();
    files.push(("en/guide/html.md", "# H\n\nText\n\n<div>x</div>\n"));
    files[3].1 = "api/audio/tone\nguide/html\nguide/start\n";
    let found = validate(&library(&files));
    assert_eq!(
        found.iter().map(|d| d.code).collect::<Vec<_>>(),
        ["raw-html"]
    );
    assert!(!found[0].is_error());
}

#[test]
fn translations_need_an_english_page_and_missing_ones_are_warnings() {
    let mut files = consistent();
    files.push(("es/guide/start.md", "# Inicio\n\nHola.\n"));
    files.push(("es/guide/extra.md", "# Extra\n\nSolo en español.\n"));
    let found = validate(&library(&files));
    let summary: Vec<_> = found.iter().map(|d| (d.code, d.is_error())).collect();
    assert_eq!(
        summary,
        [("orphan-translation", true), ("missing-translation", false)]
    );
    assert_eq!(found[1].location, "docs/es/api/audio/tone.md");
}

#[test]
fn catalog_entries_and_api_pages_must_match() {
    let files = [
        ("api/audio.ron", TONE_CATALOG),
        ("en/api/audio/env.md", "# env\n\nOrphan.\n"),
        (
            "en/api/index.md",
            "# API\n\nOverview pages need no entry.\n",
        ),
        ("ids.lock", "api/audio/env\napi/index\n"),
    ];
    assert_eq!(
        codes(&library(&files)),
        ["missing-api-page", "orphan-api-page"]
    );
}

#[test]
fn a_parameters_table_that_drifts_from_the_catalog_is_a_warning() {
    let mut files = consistent();
    files[1].1 = "# tone\n\nPlays a tone.\n\n## Parameters\n\n| Name | Meaning |\n|---|---|\n| `channel` | Voice |\n| `hz` | Hertz |\n";
    let found = validate(&library(&files));
    assert_eq!(
        found.iter().map(|d| d.code).collect::<Vec<_>>(),
        ["parameters-drift"]
    );
    assert!(!found[0].is_error());
}

#[test]
fn ids_lock_protects_published_ids() {
    let mut files = consistent();
    files[3].1 = "api/audio/old-name\napi/audio/tone\n";
    assert_eq!(
        codes(&library(&files)),
        ["locked-id-missing", "unlocked-id"]
    );

    files.pop();
    assert_eq!(codes(&library(&files)), ["missing-ids-lock"]);
}

#[test]
fn a_library_without_english_is_an_error() {
    let files = [("es/guide/a.md", "# A\n\nHola.\n"), ("ids.lock", "")];
    let found = codes(&library(&files));
    assert!(found.contains(&"missing-english"), "{found:?}");
}

#[test]
fn load_problems_are_part_of_the_result() {
    let mut files = consistent();
    files.push(("api/broken.ron", "[("));
    assert_eq!(codes(&library(&files)), ["catalog-parse"]);
}

#[test]
fn resolving_relative_paths_stays_inside_the_tree() {
    assert_eq!(
        resolve_relative("en/api/audio", "../../x.png").as_deref(),
        Some("en/x.png")
    );
    assert_eq!(
        resolve_relative("en", "./a/./b.png").as_deref(),
        Some("en/a/b.png")
    );
    assert_eq!(resolve_relative("en", "../../x.png"), None);
}

mod ui {
    use pretty_assertions::assert_eq;

    use super::*;

    fn check(sources: &[(&str, &str)]) -> Vec<&'static str> {
        let sources: Vec<_> = sources
            .iter()
            .map(|(tag, text)| UiCatalogSource {
                language: Language::parse(tag).unwrap(),
                location: "catalog.ftl",
                source: text,
            })
            .collect();
        ui_catalogs(&sources).iter().map(|d| d.code).collect()
    }

    #[test]
    fn consistent_catalogs_pass() {
        assert_eq!(
            check(&[("en", "a = A { $n }\n"), ("es", "a = Á { $n }\n")]),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn english_is_required() {
        assert_eq!(check(&[("es", "a = A\n")]), ["ui-missing-english"]);
    }

    #[test]
    fn problems_are_reported_by_kind() {
        assert_eq!(
            check(&[("en", "a = {\n")]),
            ["ui-syntax", "ui-missing-english"]
        );
        assert_eq!(
            check(&[("en", "a = One\na = Two\n")]),
            ["ui-duplicate", "ui-missing-english"]
        );
        assert_eq!(check(&[("en", "a = { missing }\n")]), ["ui-unresolvable"]);
        assert_eq!(
            check(&[
                ("en", "a = A { $n }\nb = B\n"),
                ("es", "a = Á { $m }\nc = C\n")
            ]),
            [
                "ui-variables",
                "ui-unknown-message",
                "ui-missing-translation"
            ]
        );
    }
}
