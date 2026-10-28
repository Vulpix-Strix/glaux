//! What an external analyzer said about the open documents.
//!
//! A result belongs to one revision of one document. Once the document is
//! edited the result is stale and none of its positions can be trusted, so
//! [`DiagnosticStore::current`] stops returning it. The last counts stay
//! readable as a [`Summary`] marked stale, so the status bar can dim them
//! instead of blinking while the next result is on its way.

use std::collections::HashMap;
use std::ops::Range;

use super::document::DocumentId;

/// Ordered from most to least serious.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// One finding, located by byte offsets into the document it was made for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorDiagnostic {
    pub range: Range<usize>,
    pub severity: Severity,
    /// The analyzer's name for the rule, such as `TypeError`.
    pub code: Option<String>,
    pub message: String,
}

/// How many errors and warnings a document has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    pub errors: usize,
    pub warnings: usize,
    /// The counts are from before the latest edit.
    pub stale: bool,
}

#[derive(Debug)]
struct Entry {
    revision: u64,
    items: Vec<EditorDiagnostic>,
}

impl Entry {
    fn summary(&self, stale: bool) -> Summary {
        let count = |severity| self.items.iter().filter(|d| d.severity == severity).count();
        Summary {
            errors: count(Severity::Error),
            warnings: count(Severity::Warning),
            stale,
        }
    }
}

#[derive(Debug, Default)]
pub struct DiagnosticStore {
    entries: HashMap<DocumentId, Entry>,
}

impl DiagnosticStore {
    /// Keeps `items` if they were made for the revision the document has
    /// now. A result for an older revision changes nothing.
    pub fn set(
        &mut self,
        id: DocumentId,
        now: u64,
        made_for: u64,
        mut items: Vec<EditorDiagnostic>,
    ) -> bool {
        if now != made_for {
            return false;
        }
        items.sort_by_key(|d| (d.range.start, d.range.end, d.severity));
        self.entries.insert(
            id,
            Entry {
                revision: made_for,
                items,
            },
        );
        true
    }

    /// The findings that describe revision `now` of `id`, sorted by
    /// position. Empty once the document moved on.
    pub fn current(&self, id: DocumentId, now: u64) -> &[EditorDiagnostic] {
        match self.entries.get(&id) {
            Some(entry) if entry.revision == now => &entry.items,
            _ => &[],
        }
    }

    /// The counts for `id`, or `None` if it was never analyzed.
    pub fn summary(&self, id: DocumentId, now: u64) -> Option<Summary> {
        let entry = self.entries.get(&id)?;
        Some(entry.summary(entry.revision != now))
    }

    pub fn forget(&mut self, id: DocumentId) {
        self.entries.remove(&id);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn id(n: u64) -> DocumentId {
        DocumentId::from_raw(n)
    }

    fn diagnostic(start: usize, severity: Severity) -> EditorDiagnostic {
        EditorDiagnostic {
            range: start..start + 1,
            severity,
            code: None,
            message: format!("at {start}"),
        }
    }

    #[test]
    fn a_result_for_an_older_revision_is_refused() {
        let mut store = DiagnosticStore::default();
        assert!(!store.set(id(1), 5, 4, vec![diagnostic(0, Severity::Error)]));
        assert_eq!(store.current(id(1), 5), []);
        assert_eq!(store.summary(id(1), 5), None);
    }

    #[test]
    fn findings_are_sorted_and_vanish_when_the_document_is_edited() {
        let mut store = DiagnosticStore::default();
        let items = vec![
            diagnostic(9, Severity::Warning),
            diagnostic(2, Severity::Error),
        ];
        assert!(store.set(id(1), 3, 3, items));
        let starts: Vec<_> = store
            .current(id(1), 3)
            .iter()
            .map(|d| d.range.start)
            .collect();
        assert_eq!(starts, [2, 9]);
        assert_eq!(store.current(id(1), 4), []);
    }

    #[test]
    fn the_summary_survives_an_edit_marked_stale() {
        let mut store = DiagnosticStore::default();
        store.set(
            id(1),
            3,
            3,
            vec![
                diagnostic(0, Severity::Error),
                diagnostic(4, Severity::Info),
            ],
        );
        let fresh = Summary {
            errors: 1,
            warnings: 0,
            stale: false,
        };
        assert_eq!(store.summary(id(1), 3), Some(fresh));
        assert_eq!(
            store.summary(id(1), 4),
            Some(Summary {
                stale: true,
                ..fresh
            })
        );
    }

    #[test]
    fn forgetting_a_document_drops_its_findings() {
        let mut store = DiagnosticStore::default();
        store.set(id(1), 1, 1, vec![diagnostic(0, Severity::Error)]);
        store.forget(id(1));
        assert_eq!(store.summary(id(1), 1), None);
    }
}
