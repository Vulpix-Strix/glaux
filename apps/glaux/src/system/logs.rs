//! The log of the current run and the panel that displays it.
//!
//! A record is built once and shared with the shell's scrollback, so its
//! text is stored a single time.

mod panel;
mod record;
mod store;

pub use panel::{LogPanel, PanelAction};
pub use record::{Category, FieldValue, LogRecord, Severity, Source, SourceLocation};
pub use store::LogStore;

#[cfg(test)]
mod tests;
