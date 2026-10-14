//! Previously submitted command lines.

use std::collections::VecDeque;

/// Most commands kept. Submitting another drops the oldest.
pub(super) const HISTORY_LIMIT: usize = 256;

/// Bounded list of submitted commands with an Up/Down navigation cursor.
///
/// Navigation hands out copies, so whatever the caller does with a recalled
/// line never changes the stored entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct History {
    /// Oldest first.
    entries: VecDeque<String>,
    /// Index into `entries` of the recalled line, or `None` when not
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
mod tests;
