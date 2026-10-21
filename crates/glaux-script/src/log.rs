//! [`LogEntry`]s a script writes, and the bounded [`LogSink`] they are
//! recorded in.

use std::cell::{Cell, RefCell};

/// Severity of a [`LogEntry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
}

impl LogLevel {
    pub const ALL: [Self; 4] = [Self::Trace, Self::Debug, Self::Info, Self::Warn];

    /// The lowercase name scripts use for this level, as in `log.info` or
    /// `log::info`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
        }
    }
}

/// Text a script logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub level: LogLevel,
    pub text: String,
}

/// Entries of Trace, Debug and Info a sink accepts between two
/// [`LogSink::take`]s.
pub const MAX_ORDINARY_ENTRIES: u32 = 256;

/// Entries of Warn a sink accepts between two takes. Counted apart from the
/// ordinary levels so a flood of them cannot hide a warning.
pub const MAX_WARN_ENTRIES: u32 = 64;

/// Longest text of one entry in bytes. Longer text is cut.
pub const MAX_ENTRY_BYTES: usize = 2048;

/// Most lines one entry keeps. Further lines are cut.
pub const MAX_ENTRY_LINES: usize = 16;

/// Where a runtime records what its script logs, bounded so a script that
/// logs in a loop costs a fixed amount of memory per tick.
///
/// The bound renews whenever the output is taken, which the system does once
/// per tick. Callers ask [`admit`](Self::admit) before building an entry's
/// text, so a refused entry costs no conversion or allocation.
#[derive(Debug, Default)]
pub struct LogSink {
    entries: RefCell<Vec<LogEntry>>,
    ordinary: Cell<u32>,
    warns: Cell<u32>,
    dropped: Cell<u32>,
}

impl LogSink {
    /// Whether an entry of `level` fits in this tick's allowance. A refusal
    /// is counted as dropped.
    pub fn admit(&self, level: LogLevel) -> bool {
        let (used, limit) = match level {
            LogLevel::Warn => (&self.warns, MAX_WARN_ENTRIES),
            LogLevel::Trace | LogLevel::Debug | LogLevel::Info => {
                (&self.ordinary, MAX_ORDINARY_ENTRIES)
            }
        };
        if used.get() < limit {
            used.set(used.get() + 1);
            true
        } else {
            self.dropped.set(self.dropped.get().saturating_add(1));
            false
        }
    }

    /// Records `text` at `level`, cut to [`MAX_ENTRY_BYTES`] and
    /// [`MAX_ENTRY_LINES`]. Call it only after [`admit`](Self::admit) agreed.
    pub fn push(&self, level: LogLevel, mut text: String) {
        cut_entry(&mut text);
        self.entries.borrow_mut().push(LogEntry { level, text });
    }

    /// Takes what was recorded and renews the allowance.
    pub fn take(&self) -> Vec<LogEntry> {
        self.ordinary.set(0);
        self.warns.set(0);
        std::mem::take(&mut *self.entries.borrow_mut())
    }

    /// Takes how many entries were refused since the last call.
    pub fn take_dropped(&self) -> u32 {
        self.dropped.replace(0)
    }
}

/// Cuts `text` to the limits of one entry, [`MAX_ENTRY_BYTES`] and
/// [`MAX_ENTRY_LINES`], on a character boundary.
pub fn cut_entry(text: &mut String) {
    let mut end = text.len();
    if let Some((index, _)) = text.match_indices('\n').nth(MAX_ENTRY_LINES - 1) {
        end = index;
    }
    end = end.min(MAX_ENTRY_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}
