//! Which links a viewer may hand to the operating system.

/// The URI schemes of links that leave the documentation and open in the
/// program the system picks. The link check and every viewer use this list,
/// so what passes the check is what opens.
pub const EXTERNAL_SCHEMES: [&str; 3] = ["http", "https", "mailto"];

/// Whether `scheme`, in any capitalization, is one of [`EXTERNAL_SCHEMES`].
pub fn is_external_scheme(scheme: &str) -> bool {
    EXTERNAL_SCHEMES
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(scheme))
}

/// The URI scheme `target` starts with, such as `https` or `mailto`.
/// `None` when the text before the first colon cannot be a scheme, as in a
/// relative path.
pub fn scheme(target: &str) -> Option<&str> {
    let (scheme, _) = target.split_once(':')?;
    let valid = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c));
    valid.then_some(scheme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemes_are_read_up_to_the_first_colon() {
        assert_eq!(scheme("https://example.com"), Some("https"));
        assert_eq!(scheme("mailto:a@b.c"), Some("mailto"));
        assert_eq!(scheme("glaux://api/x"), Some("glaux"));
        assert_eq!(scheme("docs/page.md"), None);
        assert_eq!(scheme("a b:c"), None);
        assert_eq!(scheme(":x"), None);
    }

    #[test]
    fn external_schemes_ignore_case() {
        for scheme in ["http", "HTTPS", "MailTo"] {
            assert!(is_external_scheme(scheme), "{scheme}");
        }
        for scheme in ["file", "javascript", "data", "glaux", ""] {
            assert!(!is_external_scheme(scheme), "{scheme}");
        }
    }
}
