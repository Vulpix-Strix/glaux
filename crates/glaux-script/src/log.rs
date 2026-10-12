//! [`LogEntry`]s a script writes.

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
