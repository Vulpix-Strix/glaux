//! Counters of the traffic with one server, to see what a strategy costs.

use std::sync::atomic::{AtomicU64, Ordering};

/// Live counters. Cheap to bump from the session and to read from anywhere.
#[derive(Debug, Default)]
pub(crate) struct Stats {
    pulls_sent: AtomicU64,
    pulls_answered: AtomicU64,
    pulls_discarded: AtomicU64,
    pulls_cancelled: AtomicU64,
    notifications_sent: AtomicU64,
}

impl Stats {
    pub fn pull_sent(&self) {
        self.pulls_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// An answer arrived for a pull. `kept` says whether it still described
    /// the document and was passed on.
    pub fn pull_answered(&self, kept: bool) {
        self.pulls_answered.fetch_add(1, Ordering::Relaxed);
        if !kept {
            self.pulls_discarded.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn pull_cancelled(&self) {
        self.pulls_cancelled.fetch_add(1, Ordering::Relaxed);
    }

    pub fn notifications_sent(&self, count: usize) {
        self.notifications_sent
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            pulls_sent: self.pulls_sent.load(Ordering::Relaxed),
            pulls_answered: self.pulls_answered.load(Ordering::Relaxed),
            pulls_discarded: self.pulls_discarded.load(Ordering::Relaxed),
            pulls_cancelled: self.pulls_cancelled.load(Ordering::Relaxed),
            notifications_sent: self.notifications_sent.load(Ordering::Relaxed),
        }
    }
}

/// What a session has done so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatsSnapshot {
    /// `textDocument/diagnostic` requests written.
    pub pulls_sent: u64,
    /// Answers received, including the ones discarded.
    pub pulls_answered: u64,
    /// Answers that no longer described the document.
    pub pulls_discarded: u64,
    /// Requests given up on after their deadline.
    pub pulls_cancelled: u64,
    /// `didOpen`, `didChange` and `didClose` written.
    pub notifications_sent: u64,
}

impl StatsSnapshot {
    /// What happened between `earlier` and this snapshot.
    pub fn since(self, earlier: Self) -> Self {
        Self {
            pulls_sent: self.pulls_sent - earlier.pulls_sent,
            pulls_answered: self.pulls_answered - earlier.pulls_answered,
            pulls_discarded: self.pulls_discarded - earlier.pulls_discarded,
            pulls_cancelled: self.pulls_cancelled - earlier.pulls_cancelled,
            notifications_sent: self.notifications_sent - earlier.notifications_sent,
        }
    }
}
