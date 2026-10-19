//! Incremental syntax highlighting for Glaux scripts.
//!
//! This crate turns source text into [`Span`]s of [`SyntaxKind`]. It knows nothing
//! about colors, rendering, or the editor that displays the text.
//!
//! The same parse tree also answers structural questions for editing features, such as
//! which [`Definition`]s are visible at an offset or which [`CallSite`] a caret is typing
//! arguments for. No Tree-sitter type is exposed.

mod calls;
mod edit;
mod grammar;
mod highlighter;
mod kind;
mod resolve;
mod scope;

pub use calls::CallSite;
pub use grammar::Grammar;
pub use highlighter::{Highlighter, HighlighterError};
pub use kind::SyntaxKind;
pub use resolve::Span;
pub use scope::{Definition, DefinitionKind, Site};
