//! The records the log keeps.

use std::borrow::Cow;

use glaux_script::LogLevel;

use crate::cartridge::source::SourcePath;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A cartridge script, through `log` or `print`.
    Cartridge,
    /// Glaux itself.
    System,
}

/// Ordered from least to most serious.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    /// Scripts cannot log errors, only Glaux can.
    Error,
}

impl Severity {
    /// The level the shell shows this severity at. The shell has no error
    /// level and reports errors in its own way, so they show as warnings.
    pub fn shell_level(self) -> LogLevel {
        match self {
            Self::Trace => LogLevel::Trace,
            Self::Debug => LogLevel::Debug,
            Self::Info => LogLevel::Info,
            Self::Warn | Self::Error => LogLevel::Warn,
        }
    }
}

impl From<LogLevel> for Severity {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => Self::Trace,
            LogLevel::Debug => Self::Debug,
            LogLevel::Info => Self::Info,
            LogLevel::Warn => Self::Warn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Text a script logged.
    Script,
    /// A Glaux diagnostic that is not about one of the stages below.
    Diagnostic,
    Save,
    Load,
    /// A script that did not compile or parse.
    Compile,
    /// A script that failed while running.
    Runtime,
}

/// A value attached to a record by name.
pub use crate::logging::Field as FieldValue;

fn heap_bytes(value: &FieldValue) -> usize {
    match value {
        FieldValue::Str(text) => text.len(),
        FieldValue::I64(_) | FieldValue::U64(_) | FieldValue::Bool(_) => 0,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub path: SourcePath,
    /// 1-based.
    pub line: u32,
}

/// One log entry. Cartridge records carry no fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    /// Increases with every record made in the session.
    pub seq: u64,
    pub source: Source,
    pub severity: Severity,
    pub category: Category,
    pub message: Box<str>,
    pub fields: Vec<(&'static str, FieldValue)>,
    /// Only set when the file exists in the cartridge.
    pub location: Option<SourceLocation>,
}

impl LogRecord {
    /// The message followed by its fields as `name=value`.
    pub fn text(&self) -> Cow<'_, str> {
        if self.fields.is_empty() {
            return Cow::Borrowed(&self.message);
        }
        let mut text = self.message.to_string();
        for (name, value) in &self.fields {
            text.push_str(&format!(" {name}="));
            match value {
                FieldValue::Str(value) => text.push_str(value),
                FieldValue::I64(value) => text.push_str(&value.to_string()),
                FieldValue::U64(value) => text.push_str(&value.to_string()),
                FieldValue::Bool(value) => text.push_str(&value.to_string()),
            }
        }
        Cow::Owned(text)
    }

    /// An estimate, in bytes, of the memory the record holds.
    pub fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + self.message.len()
            + self.fields.capacity() * size_of::<(&'static str, FieldValue)>()
            + self
                .fields
                .iter()
                .map(|(_, value)| heap_bytes(value))
                .sum::<usize>()
            + self
                .location
                .as_ref()
                .map_or(0, |location| location.path.as_str().len())
    }
}
