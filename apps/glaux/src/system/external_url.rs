//! Links the system may ask the host to open outside Glaux.

use glaux_docs::{is_external_scheme, scheme};

/// A link that passed the closed list of [`EXTERNAL_SCHEMES`].
///
/// The host opens only values of this type, so a link that was never checked
/// cannot reach the operating system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalUrl(String);

/// Why a link is not an [`ExternalUrl`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExternalUrlError {
    #[error("the link has no scheme")]
    NoScheme,
    #[error("`{0}:` links cannot be opened outside Glaux")]
    Scheme(String),
    #[error("the link has nothing after its scheme")]
    Empty,
    #[error("the link has spaces or control characters")]
    Unsafe,
}

impl ExternalUrl {
    /// Reads `target` as written in a page. Only the schemes in
    /// [`EXTERNAL_SCHEMES`] pass, in any capitalization.
    pub fn parse(target: &str) -> Result<Self, ExternalUrlError> {
        let scheme = scheme(target).ok_or(ExternalUrlError::NoScheme)?;
        if !is_external_scheme(scheme) {
            return Err(ExternalUrlError::Scheme(scheme.to_ascii_lowercase()));
        }
        if target
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
        {
            return Err(ExternalUrlError::Unsafe);
        }
        let rest = &target[scheme.len() + 1..];
        if rest.trim_start_matches('/').is_empty() {
            return Err(ExternalUrlError::Empty);
        }
        Ok(Self(target.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use glaux_docs::EXTERNAL_SCHEMES;

    use super::*;

    #[test]
    fn web_and_mail_links_pass_in_any_capitalization() {
        for link in [
            "http://example.com",
            "https://example.com/a?b=c#d",
            "HTTPS://Example.com",
            "MailTo:someone@example.com",
        ] {
            assert_eq!(ExternalUrl::parse(link).unwrap().as_str(), link);
        }
    }

    #[test]
    fn other_schemes_are_blocked_by_name() {
        for (link, scheme) in [
            ("file:///etc/passwd", "file"),
            ("JavaScript:alert(1)", "javascript"),
            ("data:text/html,x", "data"),
            ("glaux://api/graphics/spr", "glaux"),
            ("ftp://example.com", "ftp"),
            ("C:\\Windows\\notepad.exe", "c"),
        ] {
            assert_eq!(
                ExternalUrl::parse(link),
                Err(ExternalUrlError::Scheme(scheme.to_owned())),
                "{link}"
            );
        }
    }

    #[test]
    fn paths_and_empty_text_have_no_scheme() {
        for link in ["", "docs/page.md", "../x", "/usr/bin/x", "example.com"] {
            assert_eq!(
                ExternalUrl::parse(link),
                Err(ExternalUrlError::NoScheme),
                "{link:?}"
            );
        }
    }

    #[test]
    fn a_scheme_needs_a_destination() {
        for link in ["https:", "https://", "http:///", "mailto:"] {
            assert_eq!(
                ExternalUrl::parse(link),
                Err(ExternalUrlError::Empty),
                "{link}"
            );
        }
    }

    #[test]
    fn spaces_and_control_characters_are_refused() {
        for link in [
            "https://exa mple.com",
            "https://example.com\n",
            "https://example.com/\u{7}",
            "https://example.com/\u{a0}x",
            " https://example.com",
        ] {
            let result = ExternalUrl::parse(link);
            assert!(
                matches!(
                    result,
                    Err(ExternalUrlError::Unsafe | ExternalUrlError::NoScheme)
                ),
                "{link:?}: {result:?}"
            );
        }
    }

    #[test]
    fn every_scheme_the_docs_check_admits_is_admitted_here() {
        for scheme in EXTERNAL_SCHEMES {
            assert!(
                ExternalUrl::parse(&format!("{scheme}://example.com")).is_ok(),
                "{scheme}"
            );
        }
    }
}
