use std::fmt;

use unic_langid::LanguageIdentifier;

/// A language or regional variant, such as `en` or `es-MX`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Language(LanguageIdentifier);

/// Why a string is not a valid language tag.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid language tag {tag:?}: {reason}")]
pub struct LanguageError {
    tag: String,
    reason: String,
}

impl Language {
    /// Parses a BCP 47 tag such as `es` or `es-MX`.
    pub fn parse(tag: &str) -> Result<Self, LanguageError> {
        tag.parse::<LanguageIdentifier>()
            .map(Self)
            .map_err(|error| LanguageError {
                tag: tag.to_owned(),
                reason: error.to_string(),
            })
    }

    /// English, the language every catalog falls back to.
    pub fn english() -> Self {
        Self(LanguageIdentifier::from_parts(
            "en".parse().expect("`en` is a valid language subtag"),
            None,
            None,
            &[],
        ))
    }

    /// The language without script, region or variants: `es-MX` gives `es`.
    pub fn base(&self) -> Self {
        Self(LanguageIdentifier::from_parts(
            self.0.language,
            None,
            None,
            &[],
        ))
    }

    pub fn identifier(&self) -> &LanguageIdentifier {
        &self.0
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The languages tried, in order, when resolving text.
///
/// The requested language comes first, then its base language when it names
/// a region, then English. English always ends the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackChain(Vec<Language>);

impl FallbackChain {
    pub fn new(requested: Language) -> Self {
        let mut chain = vec![requested.clone()];
        for next in [requested.base(), Language::english()] {
            if !chain.contains(&next) {
                chain.push(next);
            }
        }
        Self(chain)
    }

    /// A chain holding only English.
    pub fn english() -> Self {
        Self::new(Language::english())
    }

    pub fn languages(&self) -> &[Language] {
        &self.0
    }

    pub fn requested(&self) -> &Language {
        &self.0[0]
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn tags(chain: &FallbackChain) -> Vec<String> {
        chain.languages().iter().map(ToString::to_string).collect()
    }

    #[test]
    fn regional_language_falls_back_to_base_then_english() {
        let chain = FallbackChain::new(Language::parse("es-MX").unwrap());
        assert_eq!(tags(&chain), ["es-MX", "es", "en"]);
    }

    #[test]
    fn english_chain_has_no_duplicates() {
        assert_eq!(tags(&FallbackChain::english()), ["en"]);
        let chain = FallbackChain::new(Language::parse("en-GB").unwrap());
        assert_eq!(tags(&chain), ["en-GB", "en"]);
    }

    #[test]
    fn invalid_tags_are_rejected() {
        assert!(Language::parse("").is_err());
        assert!(Language::parse("not a tag").is_err());
    }
}
