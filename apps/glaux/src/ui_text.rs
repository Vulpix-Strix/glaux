//! The interface's translated text.
//!
//! Catalogs live in the repository's `rom/lang/` directory and are embedded
//! in the binary, so the interface needs no files at run time.

use glaux_i18n::{Arg, Catalog, FallbackChain, Language, Translator};

/// The English catalog. Every language falls back to it.
const ENGLISH: &str = include_str!("../../../rom/lang/en.ftl");

/// Every embedded catalog as its language tag, its repository path for
/// diagnostics, and its Fluent source.
#[cfg(test)]
const CATALOGS: [(&str, &str, &str); 1] = [("en", "rom/lang/en.ftl", ENGLISH)];

/// The translator the interface draws its text with.
///
/// English is the only language so far. This is the one place that will
/// choose the language chain once there is a language setting.
pub fn translator() -> Translator {
    let english = Catalog::parse(Language::english(), ENGLISH)
        .expect("the embedded English catalog is valid, which the tests check");
    Translator::new(FallbackChain::english(), vec![english])
}

thread_local! {
    // Fluent bundles are not Sync. Keep parsing off the rendering path without
    // sharing a bundle across threads.
    static TEXT: Translator = translator();
}

pub fn get(id: &str) -> String {
    TEXT.with(|text| text.get(id).into_owned())
}

pub fn get_with(id: &str, args: &[(&str, Arg<'_>)]) -> String {
    TEXT.with(|text| text.get_with(id, args).into_owned())
}

#[cfg(test)]
mod tests;
