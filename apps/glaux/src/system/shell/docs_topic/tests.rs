use glaux_docs::DocId;
use glaux_i18n::FallbackChain;
use pretty_assertions::assert_eq;

use super::*;

fn library() -> Library {
    let files = [
        (
            "en/guide/getting-started.md",
            "# Getting started\n\nBegin.\n",
        ),
        ("en/guide/loops.md", "# Loops\n\nRepeat.\n"),
        ("en/api/graphics/spr.md", "# spr\n\nDraw a sprite.\n"),
        ("en/api/audio/tone.md", "# tone\n\nPlay a tone.\n"),
        ("en/api/audio/noise.md", "# noise\n\nPlay noise.\n"),
        (
            "en/api/other/noise.md",
            "# noise (other)\n\nAnother noise.\n",
        ),
    ];
    Library::from_files(
        files
            .into_iter()
            .map(|(path, text)| (path.to_owned(), text.as_bytes().to_vec())),
    )
}

fn id(text: &str) -> DocId {
    DocId::parse(text).unwrap()
}

fn resolve_in(topic: &str) -> Topic {
    resolve(topic, &library(), &FallbackChain::english())
}

#[test]
fn a_page_id_opens_that_page() {
    assert_eq!(
        resolve_in("guide/getting-started"),
        Topic::Page(id("guide/getting-started"))
    );
}

#[test]
fn a_name_only_one_page_ends_with_opens_that_page() {
    assert_eq!(resolve_in("spr"), Topic::Page(id("api/graphics/spr")));
    assert_eq!(resolve_in("SPR"), Topic::Page(id("api/graphics/spr")));
}

#[test]
fn a_name_several_pages_share_searches_instead_of_choosing() {
    assert_eq!(resolve_in("noise"), Topic::Search("noise".to_owned()));
}

#[test]
fn a_single_search_match_opens_that_page() {
    assert_eq!(resolve_in("repeat"), Topic::Page(id("guide/loops")));
}

#[test]
fn several_search_matches_search_instead_of_choosing() {
    assert_eq!(resolve_in("play"), Topic::Search("play".to_owned()));
}

#[test]
fn a_topic_nothing_mentions_is_not_found() {
    assert_eq!(resolve_in("zzzz"), Topic::NotFound);
    assert_eq!(resolve_in("  "), Topic::NotFound);
}
