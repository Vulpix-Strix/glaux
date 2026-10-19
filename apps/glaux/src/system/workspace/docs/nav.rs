//! The order pages are listed in, and the pages a page points to.

use std::collections::BTreeSet;

use glaux_docs::{DocId, Library, Page};
use glaux_i18n::FallbackChain;

/// The heading whose links a page lists as related reading.
const SEE_ALSO: &str = "See also";
/// The sections listed first, in this order.
const FIRST_SECTIONS: [&str; 2] = ["guide", "api"];

/// Every page `chain` can show, grouped by section.
///
/// Pages have no order of their own, so a section follows its `index` page:
/// the pages it links to come in the order the links appear, and any left
/// over follow by id.
pub fn order(library: &Library, chain: &FallbackChain) -> Vec<DocId> {
    let all: BTreeSet<DocId> = chain
        .languages()
        .iter()
        .flat_map(|language| library.pages(language).map(|page| page.id().clone()))
        .collect();
    let mut sections: Vec<&str> = all.iter().map(DocId::section).collect();
    sections.sort_by_key(|section| {
        let first = FIRST_SECTIONS.iter().position(|name| name == section);
        (first.unwrap_or(FIRST_SECTIONS.len()), *section)
    });
    sections.dedup();

    let mut ordered = Vec::new();
    let mut seen = BTreeSet::new();
    for section in sections {
        if let Ok(index) = DocId::parse(&format!("{section}/index")) {
            visit(
                library,
                chain,
                &all,
                section,
                index,
                &mut seen,
                &mut ordered,
            );
        }
        for id in all.iter().filter(|id| id.section() == section) {
            if seen.insert(id.clone()) {
                ordered.push(id.clone());
            }
        }
    }
    ordered
}

/// Adds `id`, and the pages it links to when it is an index page.
fn visit(
    library: &Library,
    chain: &FallbackChain,
    all: &BTreeSet<DocId>,
    section: &str,
    id: DocId,
    seen: &mut BTreeSet<DocId>,
    ordered: &mut Vec<DocId>,
) {
    if !all.contains(&id) || !seen.insert(id.clone()) {
        return;
    }
    ordered.push(id.clone());
    if !id.as_str().ends_with("/index") && id.as_str() != "index" {
        return;
    }
    let Some(resolved) = library.page(&id, chain) else {
        return;
    };
    for link in resolved.page.links() {
        if let Some(Ok(target)) = DocId::from_uri(&link.target)
            && target.section() == section
        {
            visit(library, chain, all, section, target, seen, ordered);
        }
    }
}

/// The pages the `See also` section of `page` links to, in order and
/// without repeats.
pub fn related(page: &Page) -> Vec<DocId> {
    let Some(section) = page
        .outline()
        .iter()
        .find(|section| section.level == 2 && section.title == SEE_ALSO)
    else {
        return Vec::new();
    };
    let mut related = Vec::new();
    for link in page.links() {
        if !section.lines.contains(&link.line) {
            continue;
        }
        if let Some(Ok(id)) = DocId::from_uri(&link.target)
            && &id != page.id()
            && !related.contains(&id)
        {
            related.push(id);
        }
    }
    related
}

#[cfg(test)]
mod tests;
