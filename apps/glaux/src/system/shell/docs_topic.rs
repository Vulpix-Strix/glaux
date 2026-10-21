//! Turns what `docs` was given into the page, or the search, that answers it.

use std::collections::BTreeSet;

use glaux_docs::{DocId, Library};
use glaux_i18n::FallbackChain;

/// What a topic named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Topic {
    /// Exactly one page.
    Page(DocId),
    /// Several pages could be meant, so the reader chooses among them.
    Search(String),
    NotFound,
}

/// Finds what `topic` names. A page opens only when nothing else could be
/// meant: `topic` is a page id, or the only page whose last id segment or
/// search match it is. The best-ranked page is never picked over others,
/// since a name shared by the Luau and Rhai pages has no better one.
pub fn resolve(topic: &str, library: &Library, chain: &FallbackChain) -> Topic {
    let topic = topic.trim().trim_matches('/').to_lowercase();
    if topic.is_empty() {
        return Topic::NotFound;
    }
    if let Ok(id) = DocId::parse(&topic)
        && library.page(&id, chain).is_some()
    {
        return Topic::Page(id);
    }

    let suffix = format!("/{topic}");
    let ids: BTreeSet<DocId> = chain
        .languages()
        .iter()
        .flat_map(|language| library.page_ids(language))
        .collect();
    let named: Vec<&DocId> = ids
        .iter()
        .filter(|id| id.as_str().ends_with(&suffix))
        .collect();
    match named.as_slice() {
        [id] => return Topic::Page((*id).clone()),
        [_, ..] => return Topic::Search(topic),
        [] => {}
    }

    match library.search(&topic, chain).as_slice() {
        [] => Topic::NotFound,
        [hit] => Topic::Page(hit.id.clone()),
        _ => Topic::Search(topic),
    }
}

#[cfg(test)]
mod tests;
