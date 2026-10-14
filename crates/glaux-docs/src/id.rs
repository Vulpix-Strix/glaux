use std::fmt;

/// URI scheme of internal documentation links.
pub const URI_SCHEME: &str = "glaux://";

/// The stable identifier of a documentation page, such as `api/graphics/spr`.
///
/// An id is the page's path below its language directory without the `.md`
/// extension, so `docs/en/api/graphics/spr.md` has the id `api/graphics/spr`
/// and the link `glaux://api/graphics/spr`. Every segment matches
/// `[a-z][a-z0-9-]*`. Ids are published: once released, one is never
/// renamed or reused, which `docs/ids.lock` enforces.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocId(String);

/// Why a string is not a [`DocId`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocIdError {
    #[error("documentation id is empty")]
    Empty,
    #[error("documentation id {id:?} has an empty segment")]
    EmptySegment { id: String },
    #[error("segment {segment:?} of documentation id {id:?} must match [a-z][a-z0-9-]*")]
    BadSegment { id: String, segment: String },
}

impl DocId {
    pub fn parse(id: &str) -> Result<Self, DocIdError> {
        if id.is_empty() {
            return Err(DocIdError::Empty);
        }
        for segment in id.split('/') {
            if segment.is_empty() {
                return Err(DocIdError::EmptySegment { id: id.to_owned() });
            }
            let mut chars = segment.chars();
            let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
            let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !(first_ok && rest_ok) {
                return Err(DocIdError::BadSegment {
                    id: id.to_owned(),
                    segment: segment.to_owned(),
                });
            }
        }
        Ok(Self(id.to_owned()))
    }

    /// Reads a `glaux://` link target. A `#fragment` is ignored. Returns
    /// `None` when the target is not an internal link, and an error when it
    /// is one with a malformed id.
    pub fn from_uri(target: &str) -> Option<Result<Self, DocIdError>> {
        let rest = target.strip_prefix(URI_SCHEME)?;
        let path = rest.split('#').next().unwrap_or(rest);
        Some(Self::parse(path))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first segment, such as `api` or `guide`.
    pub fn section(&self) -> &str {
        self.0.split('/').next().unwrap_or_default()
    }

    /// The internal link for this page.
    pub fn uri(&self) -> String {
        format!("{URI_SCHEME}{}", self.0)
    }
}

impl fmt::Display for DocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn accepts_nested_lowercase_ids() {
        for id in [
            "guide",
            "api/graphics/spr",
            "guide/getting-started",
            "a1/b2-c",
        ] {
            assert_eq!(DocId::parse(id).unwrap().as_str(), id);
        }
    }

    #[test]
    fn rejects_malformed_ids() {
        assert_eq!(DocId::parse(""), Err(DocIdError::Empty));
        for id in ["/api", "api/", "api//spr"] {
            assert!(
                matches!(DocId::parse(id), Err(DocIdError::EmptySegment { .. })),
                "{id}"
            );
        }
        for id in [
            "Api/spr", "api/1st", "api/sp_r", "api/../x", "api/.", "api/sp r", "-a", "api/é",
        ] {
            assert!(
                matches!(DocId::parse(id), Err(DocIdError::BadSegment { .. })),
                "{id}"
            );
        }
    }

    #[test]
    fn uri_round_trips_and_ignores_fragments() {
        let id = DocId::parse("api/graphics/spr").unwrap();
        assert_eq!(id.uri(), "glaux://api/graphics/spr");
        assert_eq!(DocId::from_uri(&id.uri()), Some(Ok(id.clone())));
        assert_eq!(
            DocId::from_uri("glaux://api/graphics/spr#flip"),
            Some(Ok(id))
        );
        assert_eq!(DocId::from_uri("https://example.com"), None);
        assert!(matches!(DocId::from_uri("glaux://Api/x"), Some(Err(_))));
    }

    #[test]
    fn section_is_the_first_segment() {
        assert_eq!(DocId::parse("api/graphics/spr").unwrap().section(), "api");
        assert_eq!(DocId::parse("guide").unwrap().section(), "guide");
    }

    proptest! {
        #[test]
        fn valid_ids_round_trip_through_uris(id in "[a-z][a-z0-9-]{0,8}(/[a-z][a-z0-9-]{0,8}){0,3}") {
            let parsed = DocId::parse(&id).unwrap();
            prop_assert_eq!(DocId::from_uri(&parsed.uri()), Some(Ok(parsed)));
        }

        #[test]
        fn parse_never_panics(text in "\\PC{0,24}") {
            let _ = DocId::parse(&text);
        }
    }
}
