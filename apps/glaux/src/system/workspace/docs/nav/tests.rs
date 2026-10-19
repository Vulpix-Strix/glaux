use glaux_i18n::{FallbackChain, Language};
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

fn ids(list: &[DocId]) -> Vec<&str> {
    list.iter().map(DocId::as_str).collect()
}

#[test]
fn a_section_follows_the_links_of_its_index_and_then_the_rest_by_id() {
    let library = library(&[
        (
            "en/guide/index.md",
            "# G\n\nStart.\n\n[z](glaux://guide/z) then [m](glaux://guide/m).\n",
        ),
        ("en/guide/a.md", "# A\n\nText.\n"),
        ("en/guide/m.md", "# M\n\nText.\n"),
        ("en/guide/z.md", "# Z\n\nText.\n"),
    ]);
    let order = order(&library, &FallbackChain::english());
    assert_eq!(
        ids(&order),
        ["guide/index", "guide/z", "guide/m", "guide/a"]
    );
}

#[test]
fn guides_come_first_then_the_api_and_then_other_sections_by_name() {
    let library = library(&[
        ("en/zeta/x.md", "# X\n\nText.\n"),
        ("en/api/f.md", "# F\n\nText.\n"),
        ("en/guide/g.md", "# G\n\nText.\n"),
        ("en/extra/e.md", "# E\n\nText.\n"),
    ]);
    let order = order(&library, &FallbackChain::english());
    assert_eq!(ids(&order), ["guide/g", "api/f", "extra/e", "zeta/x"]);
}

#[test]
fn only_index_pages_decide_the_order_of_what_they_link_to() {
    let library = library(&[
        (
            "en/guide/index.md",
            "# G\n\nStart.\n\n[b](glaux://guide/b) [a](glaux://guide/a)\n",
        ),
        // Reading `b` must not pull `c` ahead of `a`.
        ("en/guide/b.md", "# B\n\nSee [c](glaux://guide/c).\n"),
        ("en/guide/a.md", "# A\n\nText.\n"),
        ("en/guide/c.md", "# C\n\nText.\n"),
    ]);
    let order = order(&library, &FallbackChain::english());
    assert_eq!(
        ids(&order),
        ["guide/index", "guide/b", "guide/a", "guide/c"]
    );
}

#[test]
fn links_to_other_sections_or_missing_pages_do_not_order_anything() {
    let library = library(&[
        (
            "en/guide/index.md",
            "# G\n\nStart.\n\n[x](glaux://api/x) [no](glaux://guide/none) [b](glaux://guide/b)\n",
        ),
        ("en/guide/b.md", "# B\n\nText.\n"),
        ("en/api/x.md", "# X\n\nText.\n"),
    ]);
    let order = order(&library, &FallbackChain::english());
    assert_eq!(ids(&order), ["guide/index", "guide/b", "api/x"]);
}

#[test]
fn pages_from_every_language_of_the_chain_are_listed_once() {
    let library = library(&[
        ("en/guide/a.md", "# A\n\nText.\n"),
        ("en/guide/b.md", "# B\n\nText.\n"),
        ("es/guide/a.md", "# Ah\n\nTexto.\n"),
        ("es/guide/c.md", "# C\n\nTexto.\n"),
    ]);
    let chain = FallbackChain::new(Language::parse("es").unwrap());
    assert_eq!(
        ids(&order(&library, &chain)),
        ["guide/a", "guide/b", "guide/c"]
    );
    // Without Spanish in the chain its extra page is not offered.
    assert_eq!(
        ids(&order(&library, &FallbackChain::english())),
        ["guide/a", "guide/b"]
    );
}

#[test]
fn the_embedded_docs_list_every_page_once_with_the_guide_first() {
    let library = Library::embedded();
    let chain = FallbackChain::english();
    let order = order(&library, &chain);
    assert_eq!(order[0], id("guide/index"));
    let mut sorted = order.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), order.len());
    assert_eq!(
        sorted,
        library
            .page_ids(&Language::english())
            .into_iter()
            .collect::<Vec<_>>()
    );
    let at = |name: &str| order.iter().position(|id| id.as_str() == name).unwrap();
    assert!(at("guide/getting-started") < at("guide/game-loop"));
    assert!(at("guide/game-loop") < at("api/index"));
    assert!(at("api/lifecycle/boot") < at("api/graphics/cls"));
    assert!(at("api/audio/index") < at("api/audio/tone"));
}

fn page(source: &str) -> Page {
    Page::new(id("guide/this"), Language::english(), source.to_owned())
}

#[test]
fn related_pages_are_the_links_of_the_see_also_section() {
    let page = page(
        "# T\n\nSee [early](glaux://guide/early).\n\n## See also\n\n- [b](glaux://guide/b)\n- [c](glaux://guide/c) and [b again](glaux://guide/b)\n- [web](https://example.com)\n- [me](glaux://guide/this)\n\n## After\n\n[late](glaux://guide/late)\n",
    );
    assert_eq!(ids(&related(&page)), ["guide/b", "guide/c"]);
}

#[test]
fn a_page_without_see_also_has_nothing_related() {
    assert_eq!(related(&page("# T\n\n[a](glaux://guide/a)\n")), []);
    // Only a level 2 heading counts.
    assert_eq!(
        related(&page("# T\n\n### See also\n\n[a](glaux://guide/a)\n")),
        []
    );
}
