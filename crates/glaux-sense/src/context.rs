//! Works out what kind of name is being typed at the caret.
//!
//! The decision is made from the characters around the caret rather than from the
//! parse tree, because the tree of a half-typed `audio.` or `Key::` is wrong, and
//! wrong differently in each language. The tree is only asked about what text cannot
//! show, such as parameter names and the code inside string interpolations.

use std::ops::Range;

use glaux_docs::ScriptLanguage;
use glaux_syntax::Highlighter;

use crate::lexical::{self, State};

/// The token between a receiver and its member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Separator {
    /// A dot, as in `audio.tone`.
    Dot,
    /// A double colon, as in `audio::tone`.
    Path,
}

/// How the name at the caret is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Position {
    /// A name that is read, such as `co` in `x = co`.
    Reference {
        /// The word starts a statement outside every block, which is where a callback
        /// can be declared.
        top_level_statement: bool,
    },
    /// A member of a receiver, such as `to` in `audio.to` or `Sp` in `Key::Sp`.
    Member {
        /// The names before the separator, outermost first. Empty when the receiver is
        /// not a plain chain of names, as in `f().`, because nothing is known about it.
        receiver: Vec<String>,
        separator: Separator,
    },
    /// A name that is being declared, such as `x` in `local x`.
    Declaration {
        /// The name follows `function` or `fn` outside every block, so it can be the
        /// name of a callback.
        callback_site: bool,
    },
    /// A comment, a string or a number, where nothing is offered.
    Suppressed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// The whole word around the caret, which accepting a completion replaces.
    pub anchor: Range<usize>,
    /// The part of the word before the caret.
    pub prefix: String,
    pub position: Position,
    /// The indentation of the line holding the word, or its `function` keyword when
    /// the word names a callback.
    pub line_indent: String,
}

/// Reads the context at `caret`. `syntax` must have parsed `text`.
pub fn context(
    language: ScriptLanguage,
    text: &str,
    caret: usize,
    syntax: &Highlighter,
) -> Option<Context> {
    if caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    let bytes = text.as_bytes();
    let start = bytes[..caret]
        .iter()
        .rposition(|&b| !is_word(b))
        .map_or(0, |at| at + 1);
    let end = caret
        + bytes[caret..]
            .iter()
            .position(|&b| !is_word(b))
            .unwrap_or(bytes.len() - caret);
    let anchor = start..end;
    let prefix = text[start..caret].to_owned();
    let line_indent = indent_of_line(text, start);

    let scan = lexical::scan(language, &text[..start]);
    let site = syntax.site_at(anchor.clone());
    let in_code = match scan.state {
        State::Code => true,
        State::Template => site.in_interpolation,
        State::Comment | State::String => false,
    };
    let suppressed = |anchor, prefix, line_indent| Context {
        anchor,
        prefix,
        position: Position::Suppressed,
        line_indent,
    };
    if !in_code || bytes.get(start).is_some_and(u8::is_ascii_digit) {
        return Some(suppressed(anchor, prefix, line_indent));
    }

    let position = if let Some(member) = member_before(language, text, start) {
        member
    } else if let Some(position) = declaration_before(language, text, start, &site) {
        position
    } else {
        Position::Reference {
            top_level_statement: starts_top_level_statement(language, text, start),
        }
    };
    let line_indent = match position {
        Position::Declaration {
            callback_site: true,
        } => {
            let keyword = previous_word(text, start).map_or(start, |w| w.start);
            indent_of_line(text, keyword)
        }
        _ => line_indent,
    };
    Some(Context {
        anchor,
        prefix,
        position,
        line_indent,
    })
}

/// Recognizes a member access, and the separators that rule out any completion.
fn member_before(language: ScriptLanguage, text: &str, start: usize) -> Option<Position> {
    let before = &text[..start];
    if language == ScriptLanguage::Luau && before.trim_end_matches([' ', '\t']).ends_with(':') {
        // In Luau a colon starts a method call or a type, and `::` is a type cast.
        return Some(Position::Suppressed);
    }
    let (separator, separator_start) = if before.ends_with("::") {
        (Separator::Path, start - 2)
    } else if before.ends_with('.') {
        // `..` and `...` are operators, not member access.
        if before.ends_with("..") {
            return None;
        }
        (Separator::Dot, start - 1)
    } else {
        return None;
    };

    let mut receiver = Vec::new();
    let mut end = separator_start;
    loop {
        let word = word_ending_at(text, end);
        if word.is_empty() {
            receiver.clear();
            break;
        }
        if text.as_bytes()[word.start].is_ascii_digit() {
            return Some(Position::Suppressed);
        }
        receiver.push(text[word.clone()].to_owned());
        let earlier = &text[..word.start];
        if earlier.ends_with("::") && language == ScriptLanguage::Rhai {
            end = word.start - 2;
        } else if earlier.ends_with('.') && !earlier.ends_with("..") {
            end = word.start - 1;
        } else {
            receiver.reverse();
            break;
        }
    }
    Some(Position::Member {
        receiver,
        separator,
    })
}

/// Recognizes a name being declared, from the word before it or from the parse tree.
fn declaration_before(
    language: ScriptLanguage,
    text: &str,
    start: usize,
    site: &glaux_syntax::Site,
) -> Option<Position> {
    let keyword = previous_word(text, start).map(|w| &text[w]);
    let declares = match (language, keyword) {
        (ScriptLanguage::Luau, Some("local" | "function" | "for"))
        | (ScriptLanguage::Rhai, Some("let" | "const" | "fn" | "for" | "as")) => true,
        _ => site.declares_name,
    };
    if !declares {
        return None;
    }
    let function_keyword = match language {
        ScriptLanguage::Luau => "function",
        ScriptLanguage::Rhai => "fn",
    };
    let callback_site = keyword == Some(function_keyword) && {
        let word = previous_word(text, start)?;
        // The host never calls a `local function` or a `private fn`.
        let hidden = previous_word(text, word.start)
            .map(|w| &text[w])
            .is_some_and(|w| matches!(w, "local" | "private"));
        !hidden && starts_top_level_statement(language, text, word.start)
    };
    Some(Position::Declaration { callback_site })
}

/// Whether `at` begins a statement outside every block.
fn starts_top_level_statement(language: ScriptLanguage, text: &str, at: usize) -> bool {
    let line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
    if !text[line_start..at].trim().is_empty() {
        return false;
    }
    // A line that continues the previous expression, after `+` or `,` for example,
    // does not start a statement.
    let continues = text[..line_start]
        .trim_end()
        .chars()
        .next_back()
        .is_some_and(|c| "=+-*/%^<>~&|,.:?".contains(c));
    if continues {
        return false;
    }
    let scan = lexical::scan(language, &text[..at]);
    scan.state == State::Code && scan.blocks == 0 && scan.groups == 0
}

/// The identifier that ends at `end`, which is empty when there is none.
fn word_ending_at(text: &str, end: usize) -> Range<usize> {
    let start = text.as_bytes()[..end]
        .iter()
        .rposition(|&b| !is_word(b))
        .map_or(0, |at| at + 1);
    start..end
}

/// The word before `at`, ignoring whitespace in between.
fn previous_word(text: &str, at: usize) -> Option<Range<usize>> {
    let end = text[..at].trim_end().len();
    let word = word_ending_at(text, end);
    (!word.is_empty()).then_some(word)
}

fn indent_of_line(text: &str, at: usize) -> String {
    let line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
    text[line_start..]
        .chars()
        .take_while(|c| matches!(c, ' ' | '\t'))
        .collect()
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests;
