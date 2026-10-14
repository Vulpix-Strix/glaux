//! Consistency checks between a catalog and the English reference.

use std::fmt;

use crate::Catalog;

/// How serious a [`CatalogIssue`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth fixing, but the catalog still works.
    Warning,
    /// The catalog is inconsistent and should not ship.
    Error,
}

/// A problem found by [`check`] or [`check_resolution`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogIssue {
    /// A message exists in the translation but not in the reference, so
    /// nothing would ever look it up.
    UnknownMessage { id: String },
    /// A message exists in the reference but not in the translation. English
    /// is shown instead.
    MissingTranslation { id: String },
    /// The translation uses different variables than the reference.
    VariableMismatch {
        id: String,
        expected: Vec<String>,
        found: Vec<String>,
    },
    /// Formatting the message with sample arguments failed.
    Unresolvable { id: String, error: String },
}

impl CatalogIssue {
    pub fn severity(&self) -> Severity {
        match self {
            Self::MissingTranslation { .. } => Severity::Warning,
            _ => Severity::Error,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::UnknownMessage { id }
            | Self::MissingTranslation { id }
            | Self::VariableMismatch { id, .. }
            | Self::Unresolvable { id, .. } => id,
        }
    }
}

impl fmt::Display for CatalogIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownMessage { id } => {
                write!(f, "message {id:?} is not in the English catalog")
            }
            Self::MissingTranslation { id } => write!(f, "message {id:?} is not translated"),
            Self::VariableMismatch {
                id,
                expected,
                found,
            } => write!(
                f,
                "message {id:?} uses variables {found:?} but English uses {expected:?}"
            ),
            Self::Unresolvable { id, error } => {
                write!(f, "message {id:?} does not resolve: {error}")
            }
        }
    }
}

/// Compares `catalog` with the English `reference`: unknown and missing
/// messages, and variables that differ from English.
pub fn check(reference: &Catalog, catalog: &Catalog) -> Vec<CatalogIssue> {
    let mut issues = Vec::new();
    for id in catalog.ids() {
        let Some(expected) = reference.variables(id) else {
            issues.push(CatalogIssue::UnknownMessage { id: id.to_owned() });
            continue;
        };
        let found = catalog.variables(id).into_iter().flatten();
        let found: Vec<String> = found.cloned().collect();
        let expected: Vec<String> = expected.iter().cloned().collect();
        if found != expected {
            issues.push(CatalogIssue::VariableMismatch {
                id: id.to_owned(),
                expected,
                found,
            });
        }
    }
    for id in reference.ids() {
        if !catalog.contains(id) {
            issues.push(CatalogIssue::MissingTranslation { id: id.to_owned() });
        }
    }
    issues
}

/// Formats every message with sample arguments and reports the ones that
/// fail, such as a reference to a message or term that does not exist.
pub fn check_resolution(catalog: &Catalog) -> Vec<CatalogIssue> {
    let mut issues = Vec::new();
    for id in catalog.ids() {
        let Some(samples) = catalog.resolve_samples(id) else {
            continue;
        };
        for error in samples.errors {
            issues.push(CatalogIssue::Unresolvable {
                id: id.to_owned(),
                error,
            });
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{CatalogError, Language};

    fn catalog(tag: &str, source: &str) -> Catalog {
        Catalog::parse(Language::parse(tag).unwrap(), source).unwrap()
    }

    const ENGLISH: &str = "greeting = Hello, { $name }\ntab-code = Code\n";

    #[test]
    fn complete_matching_translation_has_no_issues() {
        let es = catalog("es", "greeting = Hola, { $name }\ntab-code = Código\n");
        assert_eq!(check(&catalog("en", ENGLISH), &es), []);
    }

    #[test]
    fn reports_unknown_missing_and_mismatched_messages() {
        let es = catalog("es", "greeting = Hola, { $nombre }\nextra = Extra\n");
        let issues = check(&catalog("en", ENGLISH), &es);
        assert_eq!(
            issues,
            [
                CatalogIssue::UnknownMessage { id: "extra".into() },
                CatalogIssue::VariableMismatch {
                    id: "greeting".into(),
                    expected: vec!["name".into()],
                    found: vec!["nombre".into()],
                },
                CatalogIssue::MissingTranslation {
                    id: "tab-code".into()
                },
            ]
        );
        assert_eq!(issues[0].severity(), Severity::Error);
        assert_eq!(issues[2].severity(), Severity::Warning);
    }

    #[test]
    fn unresolvable_references_are_reported() {
        let broken = catalog("en", "a = { missing-message }\nb = Fine\n");
        let issues = check_resolution(&broken);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].id(), "a");
    }

    #[test]
    fn syntax_errors_are_reported_with_their_line() {
        let result = Catalog::parse(Language::english(), "ok = Fine\nbroken = { $ }\n");
        let Err(CatalogError::Syntax { issues, .. }) = result else {
            panic!("expected a syntax error");
        };
        assert!(issues.iter().any(|issue| issue.line == 2), "{issues:?}");
    }

    #[test]
    fn duplicate_messages_are_rejected() {
        let result = Catalog::parse(Language::english(), "a = One\na = Two\n");
        assert!(matches!(
            result,
            Err(CatalogError::DuplicateMessage { ref id, .. }) if id == "a"
        ));
    }
}
