//! Incremental syntax highlighting for Glaux scripts.
//!
//! This crate turns source text into [`Span`]s of [`SyntaxKind`]. It knows nothing
//! about colors, rendering, or the editor that displays the text.

mod edit;
mod grammar;
mod highlighter;
mod kind;
mod resolve;

pub use grammar::Grammar;
pub use highlighter::{Highlighter, HighlighterError};
pub use kind::SyntaxKind;
pub use resolve::Span;
