//! The order of the Code view's document tabs, which one is active, and
//! how each document is shown.

use crate::system::editing::document::DocumentId;
use crate::system::editing::metrics::TextMetrics;
use crate::system::editing::selection::Selection;

/// How one open document is shown, kept per tab so switching documents
/// returns to each where it was left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocumentView {
    /// The line of the first row shown, counted from 0.
    pub top_line: usize,
    /// Which fold of `top_line` the first row shows while lines wrap.
    pub top_row: usize,
    /// The first column shown while lines don't wrap.
    pub left_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenTab {
    id: DocumentId,
    view: DocumentView,
    selection: Selection,
    /// The document's extent as last measured for the view.
    metrics: Option<TextMetrics>,
}

/// The Code view's tabs, at most one per document.
///
/// An active tab exists whenever any tab does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentTabs {
    tabs: Vec<OpenTab>,
    active: Option<DocumentId>,
    /// Index of the first tab the strip shows when they don't all fit.
    scroll: usize,
}

impl DocumentTabs {
    /// The documents, in tab order.
    pub fn ids(&self) -> impl ExactSizeIterator<Item = DocumentId> + '_ {
        self.tabs.iter().map(|tab| tab.id)
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn active(&self) -> Option<DocumentId> {
        self.active
    }

    pub fn index_of(&self, id: DocumentId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    /// Shows `id`'s tab, adding it right after the active tab if it has
    /// none. A document never gets a second tab.
    pub fn open(&mut self, id: DocumentId) {
        if self.index_of(id).is_none() {
            let at = self
                .active
                .and_then(|active| self.index_of(active))
                .map_or(self.tabs.len(), |index| index + 1);
            self.tabs.insert(
                at,
                OpenTab {
                    id,
                    view: DocumentView::default(),
                    selection: Selection::default(),
                    metrics: None,
                },
            );
        }
        self.active = Some(id);
    }

    /// Makes `id`'s tab the active one. Returns whether it has a tab.
    pub fn activate(&mut self, id: DocumentId) -> bool {
        let known = self.index_of(id).is_some();
        if known {
            self.active = Some(id);
        }
        known
    }

    /// Activates the tab `step` places after the active one, wrapping
    /// around either end. Does nothing without tabs.
    pub fn cycle(&mut self, step: isize) {
        let Some(index) = self.active.and_then(|active| self.index_of(active)) else {
            return;
        };
        let len = self.tabs.len() as isize;
        let next = (index as isize + step).rem_euclid(len) as usize;
        self.active = Some(self.tabs[next].id);
    }

    /// Removes `id`'s tab and its view. Closing the active tab activates
    /// the one that took its place, or else the one before it. Returns
    /// whether it had a tab.
    pub fn close(&mut self, id: DocumentId) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        self.tabs.remove(index);
        if self.active == Some(id) {
            self.active = self
                .tabs
                .get(index)
                .or_else(|| self.tabs.last())
                .map(|tab| tab.id);
        }
        self.scroll = self.scroll.min(self.tabs.len().saturating_sub(1));
        true
    }

    /// Moves the tab at `from` to `to`, shifting those between. The active
    /// tab stays active. Out-of-range indices change nothing.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from < self.tabs.len() && to < self.tabs.len() {
            let tab = self.tabs.remove(from);
            self.tabs.insert(to, tab);
        }
    }

    pub fn view(&self, id: DocumentId) -> Option<&DocumentView> {
        self.tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| &tab.view)
    }

    pub fn view_mut(&mut self, id: DocumentId) -> Option<&mut DocumentView> {
        self.tabs
            .iter_mut()
            .find(|tab| tab.id == id)
            .map(|tab| &mut tab.view)
    }

    /// What is selected in `id`'s document. Each tab keeps its own, so
    /// switching documents returns to it.
    pub fn selection(&self, id: DocumentId) -> Option<Selection> {
        self.tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.selection)
    }

    /// Changes the selection of `id`'s document. Does nothing if it has no
    /// tab.
    pub fn set_selection(&mut self, id: DocumentId, selection: Selection) {
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.selection = selection;
        }
    }

    /// `id`'s document's extent, as last given to
    /// [`set_metrics`](Self::set_metrics).
    pub fn metrics(&self, id: DocumentId) -> Option<&TextMetrics> {
        self.tabs
            .iter()
            .find(|tab| tab.id == id)
            .and_then(|tab| tab.metrics.as_ref())
    }

    /// Records `id`'s document's extent. Does nothing if it has no tab.
    pub fn set_metrics(&mut self, id: DocumentId, metrics: TextMetrics) {
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.metrics = Some(metrics);
        }
    }

    /// Index of the first tab the strip shows.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Clamped to the last tab.
    pub fn set_scroll(&mut self, scroll: usize) {
        self.scroll = scroll.min(self.tabs.len().saturating_sub(1));
    }
}

#[cfg(test)]
mod tests;
