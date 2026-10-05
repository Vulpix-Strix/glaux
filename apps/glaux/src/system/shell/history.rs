//! Previously submitted command lines.

use std::collections::VecDeque;

/// Most commands kept; submitting another drops the oldest.
pub(super) const HISTORY_LIMIT: usize = 256;

/// Bounded list of submitted commands with an Up/Down navigation cursor.
///
/// Navigation hands out copies, so whatever the caller does with a recalled
/// line never changes the stored entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct History {
    /// Oldest first.
    entries: VecDeque<String>,
    /// Index into `entries` of the recalled line; `None` when not
    /// navigating.
    cursor: Option<usize>,
    /// The line being edited when navigation began, restored when stepping
    /// past the newest entry.
    draft: String,
}

impl History {
    /// Records a submitted line and ends navigation. Blank lines and
    /// repeats of the newest entry are not recorded.
    pub(super) fn push(&mut self, line: &str) {
        self.reset();
        if line.trim().is_empty() || self.entries.back().is_some_and(|last| last == line) {
            return;
        }
        if self.entries.len() == HISTORY_LIMIT {
            self.entries.pop_front();
        }
        self.entries.push_back(line.to_owned());
    }

    /// Steps to the next older entry and returns it. `current` is the line
    /// being edited, saved as the draft when navigation begins.
    ///
    /// Returns `None` when there is nothing older to show.
    pub(super) fn older(&mut self, current: &str) -> Option<&str> {
        let index = match self.cursor {
            None if self.entries.is_empty() => return None,
            None => {
                self.draft = current.to_owned();
                self.entries.len() - 1
            }
            Some(0) => return None,
            Some(index) => index - 1,
        };
        self.cursor = Some(index);
        Some(&self.entries[index])
    }

    /// Steps to the next newer entry and returns it. Stepping past the
    /// newest entry ends navigation and returns the saved draft.
    ///
    /// Returns `None` when not navigating.
    pub(super) fn newer(&mut self) -> Option<&str> {
        let index = self.cursor? + 1;
        if index < self.entries.len() {
            self.cursor = Some(index);
            Some(&self.entries[index])
        } else {
            self.cursor = None;
            Some(&self.draft)
        }
    }

    /// Ends navigation without changing the stored entries.
    pub(super) fn reset(&mut self) {
        self.cursor = None;
    }

    #[cfg(test)]
    pub(super) fn entries(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn history(lines: &[&str]) -> History {
        let mut history = History::default();
        for line in lines {
            history.push(line);
        }
        history
    }

    fn entries(history: &History) -> Vec<&str> {
        history.entries().collect()
    }

    #[test]
    fn evicts_oldest_entry_past_the_limit() {
        let mut history = History::default();
        for n in 0..HISTORY_LIMIT + 2 {
            history.push(&format!("cmd {n}"));
        }
        let entries = entries(&history);
        assert_eq!(entries.len(), HISTORY_LIMIT);
        assert_eq!(entries[0], "cmd 2");
        assert_eq!(
            entries.last(),
            Some(&format!("cmd {}", HISTORY_LIMIT + 1).as_str())
        );
    }

    #[test]
    fn blank_lines_are_not_recorded() {
        assert_eq!(entries(&history(&["", "   ", "info"])), ["info"]);
    }

    #[test]
    fn only_adjacent_duplicates_are_suppressed() {
        assert_eq!(
            entries(&history(&["help", "help", "info", "help"])),
            ["help", "info", "help"]
        );
    }

    #[test]
    fn up_walks_older_and_stops_at_the_oldest() {
        let mut history = history(&["a", "b", "c"]);
        assert_eq!(history.older(""), Some("c"));
        assert_eq!(history.older("c"), Some("b"));
        assert_eq!(history.older("b"), Some("a"));
        assert_eq!(history.older("a"), None);
        assert_eq!(history.newer(), Some("b"));
    }

    #[test]
    fn down_walks_newer_then_restores_the_draft() {
        let mut history = history(&["a", "b"]);
        history.older("dra");
        history.older("b");
        assert_eq!(history.newer(), Some("b"));
        assert_eq!(history.newer(), Some("dra"));
        assert_eq!(history.newer(), None, "navigation ended");
        assert_eq!(history.older("x"), Some("b"), "starts again from newest");
    }

    #[test]
    fn down_without_navigation_does_nothing() {
        let mut history = history(&["a"]);
        assert_eq!(history.newer(), None);
    }

    #[test]
    fn up_on_empty_history_does_nothing() {
        assert_eq!(History::default().older("draft"), None);
    }

    #[test]
    fn reset_and_push_end_navigation() {
        let mut history = history(&["a", "b"]);
        history.older("");
        history.older("b");
        history.reset();
        assert_eq!(history.older(""), Some("b"));

        history.push("c");
        assert_eq!(history.newer(), None);
        assert_eq!(history.older(""), Some("c"));
    }
}
