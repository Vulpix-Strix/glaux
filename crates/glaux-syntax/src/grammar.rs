use tree_sitter::Language;
use tree_sitter_language::LanguageFn;

unsafe extern "C" {
    fn tree_sitter_rhai() -> *const ();
}

/// A language the highlighter can parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Grammar {
    Luau,
    Rhai,
}

impl Grammar {
    pub(crate) fn language(self) -> Language {
        match self {
            Grammar::Luau => tree_sitter_luau::LANGUAGE.into(),
            Grammar::Rhai => {
                // SAFETY: `tree_sitter_rhai` is the entry point of the parser compiled by
                // `build.rs`. It takes no arguments and returns a pointer to a static
                // `TSLanguage`, which is what `LanguageFn` requires.
                let language = unsafe { LanguageFn::from_raw(tree_sitter_rhai) };
                language.into()
            }
        }
    }

    /// The highlight query. Patterns run general to specific, because a later
    /// pattern wins when two captures cover exactly the same node.
    pub(crate) fn query(self) -> &'static str {
        match self {
            Grammar::Luau => include_str!("../queries/luau.scm"),
            Grammar::Rhai => include_str!("../queries/rhai.scm"),
        }
    }
}
