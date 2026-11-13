use tree_sitter::Language;

/// A language the highlighter can parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Grammar {
    Luau,
}

impl Grammar {
    pub(crate) fn language(self) -> Language {
        match self {
            Grammar::Luau => tree_sitter_luau::LANGUAGE.into(),
        }
    }

    /// The highlight query. Patterns run general to specific, because a later
    /// pattern wins when two captures cover exactly the same node.
    pub(crate) fn query(self) -> &'static str {
        match self {
            Grammar::Luau => include_str!("../queries/luau.scm"),
        }
    }

    /// The reserved words, as the highlight query styles them.
    pub fn keywords(self) -> &'static [&'static str] {
        match self {
            Grammar::Luau => &[
                "and", "break", "continue", "do", "else", "elseif", "end", "export", "for",
                "function", "if", "in", "local", "not", "or", "repeat", "return", "then", "type",
                "until", "while",
            ],
        }
    }

    /// The literals written as words, as the highlight query styles them.
    pub fn literals(self) -> &'static [&'static str] {
        match self {
            Grammar::Luau => &["false", "nil", "true"],
        }
    }
}

#[cfg(test)]
mod tests;
