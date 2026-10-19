use std::collections::BTreeSet;

use glaux_i18n::FallbackChain;

use super::Library;
use crate::{DocId, Page, ScriptLanguage};

/// How well a page answers a query, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// A name of the page starts with the query.
    Prefix,
    /// A name of the page contains the query.
    Substring,
    /// A heading of the page contains the query.
    Heading,
    /// The page's summary contains the query.
    Summary,
}

/// A page found by [`Library::search`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub id: DocId,
    /// The page's title, or its id when it has none.
    pub title: String,
    pub rank: Rank,
    /// `true` when the page is not in the language that was asked for.
    pub is_fallback: bool,
}

impl Library {
    /// The pages that mention `query`, ignoring case, best first.
    ///
    /// Pages come from the first language of `chain` that has them. The
    /// names a page answers to are its title and, for an API page, the name
    /// each script language spells it with, so `print` finds the Luau page
    /// of `text` and `audio::tone` finds `tone`. An empty query finds
    /// nothing.
    pub fn search(&self, query: &str, chain: &FallbackChain) -> Vec<Hit> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        let ids: BTreeSet<&DocId> = chain
            .languages()
            .iter()
            .flat_map(|language| self.pages(language).map(Page::id))
            .collect();
        let mut found = Vec::new();
        for id in ids {
            let Some(resolved) = self.page(id, chain) else {
                continue;
            };
            let names = self.names(resolved.page);
            let Some((rank, length)) = rank(resolved.page, &names, &query) else {
                continue;
            };
            let title = resolved
                .page
                .title()
                .map_or_else(|| id.to_string(), str::to_owned);
            found.push((
                (rank, length, title.to_lowercase(), id),
                Hit {
                    id: id.clone(),
                    title,
                    rank,
                    is_fallback: resolved.is_fallback,
                },
            ));
        }
        found.sort_by(|(a, _), (b, _)| a.cmp(b));
        found.into_iter().map(|(_, hit)| hit).collect()
    }

    /// The lowercase names `page` answers to.
    fn names(&self, page: &Page) -> Vec<String> {
        let mut names: Vec<String> = page.title().map(str::to_lowercase).into_iter().collect();
        if let Some(entry) = self.catalog.get(page.id()) {
            for language in ScriptLanguage::ALL {
                if let Some(binding) = entry.binding(language) {
                    names.push(binding.name.to_lowercase());
                    names.push(language.qualified_name(binding).to_lowercase());
                }
            }
        }
        names
    }
}

/// How `page` matches `query` and the length of the name that matched, so
/// that a closer name sorts first.
fn rank(page: &Page, names: &[String], query: &str) -> Option<(Rank, usize)> {
    let by_name = |wanted: fn(&str, &str) -> bool| {
        names
            .iter()
            .filter(|name| wanted(name, query))
            .map(|name| name.chars().count())
            .min()
    };
    if let Some(length) = by_name(|name, query| name.starts_with(query)) {
        return Some((Rank::Prefix, length));
    }
    if let Some(length) = by_name(|name, query| name.contains(query)) {
        return Some((Rank::Substring, length));
    }
    if page
        .outline()
        .iter()
        .any(|section| section.title.to_lowercase().contains(query))
    {
        return Some((Rank::Heading, 0));
    }
    page.summary()
        .is_some_and(|summary| summary.to_lowercase().contains(query))
        .then_some((Rank::Summary, 0))
}
