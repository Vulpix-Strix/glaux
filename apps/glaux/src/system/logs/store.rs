//! The records of the current run.

use std::collections::VecDeque;
use std::rc::Rc;

use glaux_script::LogEntry;

use super::{Category, FieldValue, LogRecord, Severity, Source, SourceLocation};
use crate::ui_text;

/// Most records kept before the oldest are dropped.
pub const STORE_LIMIT: usize = 2000;

/// Kept after the run ends so the editor can still show it.
#[derive(Debug, Default)]
pub struct LogStore {
    records: VecDeque<Rc<LogRecord>>,
    /// Held apart so a flood of records cannot push the error that ended
    /// the run out of the log.
    terminal: Option<Rc<LogRecord>>,
    next_seq: u64,
    /// Lets views detect a change without comparing records.
    revision: u64,
    retained: usize,
}

impl LogStore {
    /// Clears the log for a new run. Sequence numbers keep counting.
    pub fn begin_run(&mut self) {
        self.records.clear();
        self.terminal = None;
        self.retained = 0;
        self.revision += 1;
    }

    /// Adds the entries a cartridge logged, plus a notice if `dropped` were
    /// refused. Returns the records made.
    pub fn push_cartridge(&mut self, entries: Vec<LogEntry>, dropped: u32) -> Vec<Rc<LogRecord>> {
        let mut made = Vec::with_capacity(entries.len() + usize::from(dropped > 0));
        for entry in entries {
            made.push(self.push(
                Source::Cartridge,
                entry.level.into(),
                Category::Script,
                entry.text.into_boxed_str(),
            ));
        }
        if dropped > 0 {
            made.push(self.push_skipped(u64::from(dropped)));
        }
        made
    }

    pub fn push_skipped(&mut self, count: u64) -> Rc<LogRecord> {
        let message = ui_text::get_with(
            "logs-messages-skipped",
            &[("count", glaux_i18n::Arg::Number(count as f64))],
        );
        self.push(
            Source::System,
            Severity::Warn,
            Category::Diagnostic,
            message.into_boxed_str(),
        )
    }

    pub fn push_diagnostic(
        &mut self,
        severity: Severity,
        message: Box<str>,
        fields: Vec<(&'static str, FieldValue)>,
    ) -> Rc<LogRecord> {
        let record = self.make(Source::System, severity, Category::Diagnostic, message);
        let record = Rc::new(LogRecord { fields, ..record });
        self.insert(Rc::clone(&record));
        record
    }

    /// Adds the error that ended the run and keeps it past ring eviction.
    pub fn push_terminal(
        &mut self,
        category: Category,
        message: impl Into<Box<str>>,
        location: Option<SourceLocation>,
    ) -> Rc<LogRecord> {
        let mut message = message.into().into_string();
        glaux_script::cut_entry(&mut message);
        let record = self.make(
            Source::System,
            Severity::Error,
            category,
            message.into_boxed_str(),
        );
        let record = Rc::new(LogRecord { location, ..record });
        self.terminal = Some(Rc::clone(&record));
        self.insert(Rc::clone(&record));
        record
    }

    /// The records oldest first. A terminal error already evicted from the
    /// ring is listed first.
    pub fn iter(&self) -> impl Iterator<Item = &Rc<LogRecord>> {
        let evicted = self.terminal.as_ref().filter(|terminal| {
            self.records
                .front()
                .is_none_or(|first| terminal.seq < first.seq)
        });
        evicted.into_iter().chain(self.records.iter())
    }

    #[cfg(test)]
    pub fn terminal(&self) -> Option<&Rc<LogRecord>> {
        self.terminal.as_ref()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.records.len() + usize::from(self.terminal_evicted())
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// An estimate of the memory the records hold, in bytes.
    #[cfg(test)]
    pub fn retained_bytes(&self) -> usize {
        self.retained
    }

    #[cfg(test)]
    fn terminal_evicted(&self) -> bool {
        self.terminal.as_ref().is_some_and(|terminal| {
            self.records
                .front()
                .is_none_or(|first| terminal.seq < first.seq)
        })
    }

    fn push(
        &mut self,
        source: Source,
        severity: Severity,
        category: Category,
        message: Box<str>,
    ) -> Rc<LogRecord> {
        let record = Rc::new(self.make(source, severity, category, message));
        self.insert(Rc::clone(&record));
        record
    }

    fn make(
        &mut self,
        source: Source,
        severity: Severity,
        category: Category,
        message: Box<str>,
    ) -> LogRecord {
        let seq = self.next_seq;
        self.next_seq += 1;
        LogRecord {
            seq,
            source,
            severity,
            category,
            message,
            fields: Vec::new(),
            location: None,
        }
    }

    fn insert(&mut self, record: Rc<LogRecord>) {
        if self.records.len() == STORE_LIMIT
            && let Some(oldest) = self.records.pop_front()
        {
            self.retained -= oldest.retained_bytes();
        }
        self.retained += record.retained_bytes();
        self.records.push_back(record);
        self.revision += 1;
    }
}
