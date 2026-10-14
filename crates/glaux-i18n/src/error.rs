use std::fmt;

use crate::Language;

/// A problem found while parsing a catalog, located by line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxIssue {
    /// One-based line of the problem.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for SyntaxIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Why a catalog source could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    /// The source has syntax errors. Every one found is listed.
    #[error("{language}: {} syntax error(s), first at {}", .issues.len(), .issues[0])]
    Syntax {
        language: Language,
        issues: Vec<SyntaxIssue>,
    },
    #[error("{language}: message {id:?} is defined more than once")]
    DuplicateMessage { language: Language, id: String },
}
