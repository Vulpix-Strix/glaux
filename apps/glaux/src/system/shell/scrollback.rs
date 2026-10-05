//! Shell output kept on screen between commands.

use std::collections::VecDeque;

use glaux_luau::LogLevel;

/// Most entries kept; pushing another drops the oldest.
pub(super) const SCROLLBACK_LIMIT: usize = 1024;

/// One logical line of shell output, before it is wrapped and positioned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Entry {
    /// The greeting shown when the shell starts.
    Banner(String),
    /// A submitted command line, shown after the prompt.
    Command(String),
    Output(String),
    Error(String),
    /// One line a cartridge logged, at the level it was logged with.
    Log(LogLevel, String),
}

/// Bounded list of output entries, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Scrollback {
    entries: VecDeque<Entry>,
}

impl Scrollback {
    pub(super) fn push(&mut self, entry: Entry) {
        if self.entries.len() == SCROLLBACK_LIMIT {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Entries oldest first.
    pub(super) fn iter(&self) -> impl DoubleEndedIterator<Item = &Entry> + ExactSizeIterator {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn evicts_oldest_entry_past_the_limit() {
        let mut scrollback = Scrollback::default();
        for n in 0..SCROLLBACK_LIMIT + 3 {
            scrollback.push(Entry::Output(n.to_string()));
        }
        assert_eq!(scrollback.iter().len(), SCROLLBACK_LIMIT);
        assert_eq!(scrollback.iter().next(), Some(&Entry::Output("3".into())));
        assert_eq!(
            scrollback.iter().next_back(),
            Some(&Entry::Output((SCROLLBACK_LIMIT + 2).to_string()))
        );
    }

    #[test]
    fn clear_removes_every_entry() {
        let mut scrollback = Scrollback::default();
        scrollback.push(Entry::Command("help".into()));
        scrollback.push(Entry::Error("oops".into()));
        scrollback.clear();
        assert_eq!(scrollback.iter().len(), 0);
    }
}
