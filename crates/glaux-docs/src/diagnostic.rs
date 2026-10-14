use std::fmt;

/// How serious a [`Diagnostic`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth fixing, but the documentation still works.
    Warning,
    /// The documentation is inconsistent and should not ship.
    Error,
}

/// A problem found in the documentation, the API catalog, or a UI catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// A stable kebab-case code such as `duplicate-id`.
    pub code: &'static str,
    /// Where the problem is: a file path, optionally with a line.
    pub location: String,
    pub message: String,
}

impl Diagnostic {
    pub fn error(
        code: &'static str,
        location: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(Severity::Error, code, location, message)
    }

    pub fn warning(
        code: &'static str,
        location: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(Severity::Warning, code, location, message)
    }

    fn new(
        severity: Severity,
        code: &'static str,
        location: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code,
            location: location.into(),
            message: message.into(),
        }
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(
            f,
            "{severity}[{}] {}: {}",
            self.code, self.location, self.message
        )
    }
}
