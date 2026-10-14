use std::collections::BTreeSet;

use crate::{DocId, DocIdError};

const HEADER: &str = "\
# Published documentation ids, one per line.
#
# An id listed here is part of Glaux's public documentation. It keeps
# resolving in every later release, so do not delete or rename a page that
# has an entry. `mise run docs-update-ids` adds new ids and never removes one.
# Retiring an id is a deliberate edit of this file in a reviewed change.
";

/// The contents of `docs/ids.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IdsLock {
    ids: BTreeSet<DocId>,
}

/// A line of `ids.lock` that is not a valid id.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {source}")]
pub struct IdsLockError {
    pub line: usize,
    #[source]
    pub source: DocIdError,
}

impl IdsLock {
    /// Parses the lock file. Blank lines and lines starting with `#` are
    /// skipped. Every malformed line is reported, and the valid ones are
    /// still returned.
    pub fn parse(text: &str) -> (Self, Vec<IdsLockError>) {
        let mut lock = Self::default();
        let mut errors = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match DocId::parse(line) {
                Ok(id) => {
                    lock.ids.insert(id);
                }
                Err(source) => errors.push(IdsLockError {
                    line: index + 1,
                    source,
                }),
            }
        }
        (lock, errors)
    }

    pub fn ids(&self) -> &BTreeSet<DocId> {
        &self.ids
    }

    pub fn contains(&self, id: &DocId) -> bool {
        self.ids.contains(id)
    }

    /// The lock after adding `current`, with the ids that were added. Ids
    /// already locked are kept even when `current` no longer has them.
    pub fn updated(&self, current: &BTreeSet<DocId>) -> (Self, Vec<DocId>) {
        let added: Vec<DocId> = current.difference(&self.ids).cloned().collect();
        let mut ids = self.ids.clone();
        ids.extend(added.iter().cloned());
        (Self { ids }, added)
    }

    /// The file contents: the header, then the sorted ids.
    pub fn render(&self) -> String {
        let mut text = String::from(HEADER);
        for id in &self.ids {
            text.push_str(id.as_str());
            text.push('\n');
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn ids(list: &[&str]) -> BTreeSet<DocId> {
        list.iter().map(|id| DocId::parse(id).unwrap()).collect()
    }

    #[test]
    fn parse_skips_comments_and_reports_bad_lines() {
        let (lock, errors) = IdsLock::parse("# note\n\napi/a\n  guide  \nBad\napi/a\n");
        assert_eq!(lock.ids(), &ids(&["api/a", "guide"]));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 5);
    }

    #[test]
    fn update_only_adds_and_keeps_orphans() {
        let (lock, _) = IdsLock::parse("api/old\napi/kept\n");
        let (updated, added) = lock.updated(&ids(&["api/kept", "api/new"]));
        assert_eq!(updated.ids(), &ids(&["api/kept", "api/new", "api/old"]));
        assert_eq!(added, [DocId::parse("api/new").unwrap()]);
    }

    #[test]
    fn update_with_nothing_new_leaves_the_text_unchanged() {
        let (lock, _) = IdsLock::parse(&IdsLock::default().updated(&ids(&["b", "a"])).0.render());
        let (updated, added) = lock.updated(&ids(&["a", "b"]));
        assert_eq!(added, []);
        assert_eq!(updated.render(), lock.render());
    }

    #[test]
    fn render_is_sorted_and_reparses_to_the_same_lock() {
        let lock = IdsLock::default()
            .updated(&ids(&["guide", "api/z", "api/a"]))
            .0;
        let text = lock.render();
        let body: Vec<_> = text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .collect();
        assert_eq!(body, ["api/a", "api/z", "guide"]);
        assert_eq!(IdsLock::parse(&text), (lock, vec![]));
    }
}
