use std::borrow::Cow;

use fluent_bundle::{FluentArgs, FluentValue};

use crate::{Catalog, FallbackChain, Language};

/// A value passed to a message variable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arg<'a> {
    Text(&'a str),
    /// A number. Plural selectors see its value.
    Number(f64),
}

/// Resolves message ids to text through a [`FallbackChain`].
pub struct Translator {
    chain: FallbackChain,
    /// Catalogs in chain order. Languages without a source are skipped.
    catalogs: Vec<Catalog>,
}

impl Translator {
    /// Builds a translator from already parsed catalogs. Catalogs for
    /// languages outside the chain are ignored.
    pub fn new(chain: FallbackChain, mut catalogs: Vec<Catalog>) -> Self {
        let rank = |language: &Language| chain.languages().iter().position(|l| l == language);
        catalogs.retain(|catalog| rank(catalog.language()).is_some());
        catalogs.sort_by_key(|catalog| rank(catalog.language()));
        Self { chain, catalogs }
    }

    pub fn chain(&self) -> &FallbackChain {
        &self.chain
    }

    /// The text of `id`, or the id itself when no language in the chain has
    /// the message.
    pub fn get<'a>(&self, id: &'a str) -> Cow<'a, str> {
        self.get_with(id, &[])
    }

    /// Like [`Translator::get`], filling the message's variables from `args`.
    pub fn get_with<'a>(&self, id: &'a str, args: &[(&str, Arg<'_>)]) -> Cow<'a, str> {
        let mut fluent_args = FluentArgs::new();
        for (name, arg) in args {
            let value = match arg {
                Arg::Text(text) => FluentValue::from(*text),
                Arg::Number(number) => FluentValue::from(*number),
            };
            fluent_args.set(*name, value);
        }
        self.catalogs
            .iter()
            .find_map(|catalog| catalog.format(id, &fluent_args))
            .map_or(Cow::Borrowed(id), Cow::Owned)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn catalog(tag: &str, source: &str) -> Catalog {
        Catalog::parse(Language::parse(tag).unwrap(), source).unwrap()
    }

    fn spanish() -> Translator {
        Translator::new(
            FallbackChain::new(Language::parse("es").unwrap()),
            vec![
                catalog("en", "tab-code = Code\nonly-english = Only English\n"),
                catalog("es", "tab-code = Código\n"),
            ],
        )
    }

    #[test]
    fn requested_language_wins() {
        assert_eq!(spanish().get("tab-code"), "Código");
    }

    #[test]
    fn missing_message_falls_back_to_english() {
        assert_eq!(spanish().get("only-english"), "Only English");
    }

    #[test]
    fn message_missing_everywhere_returns_its_id() {
        assert_eq!(spanish().get("no-such-message"), "no-such-message");
    }

    #[test]
    fn variables_are_interpolated_without_isolation_marks() {
        let translator = Translator::new(
            FallbackChain::english(),
            vec![catalog("en", "no-cartridge = No cartridge in { $path }\n")],
        );
        let text = translator.get_with("no-cartridge", &[("path", Arg::Text("demo"))]);
        assert_eq!(text, "No cartridge in demo");
        assert!(!text.contains(['\u{2068}', '\u{2069}']));
    }

    #[test]
    fn plurals_follow_the_language_rules() {
        let source =
            "files = { $count ->\n    [one] { $count } file\n   *[other] { $count } files\n}\n";
        let translator = Translator::new(FallbackChain::english(), vec![catalog("en", source)]);
        assert_eq!(
            translator.get_with("files", &[("count", Arg::Number(1.0))]),
            "1 file"
        );
        assert_eq!(
            translator.get_with("files", &[("count", Arg::Number(2.0))]),
            "2 files"
        );
    }

    #[test]
    fn catalogs_outside_the_chain_are_ignored() {
        let translator = Translator::new(
            FallbackChain::english(),
            vec![
                catalog("en", "a = English\n"),
                catalog("fr", "a = Français\n"),
            ],
        );
        assert_eq!(translator.get("a"), "English");
    }
}
