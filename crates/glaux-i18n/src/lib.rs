//! Localization for Glaux's user interface text.
//!
//! UI strings live in Fluent catalogs, one per [`Language`]. A [`Translator`]
//! resolves a message id through a [`FallbackChain`], so a language that only
//! translates part of the interface still shows English for the rest. A
//! message missing everywhere resolves to its own id, which keeps gaps
//! visible without failing.
//!
//! The crate takes catalog sources as strings and never touches the
//! filesystem. Callers decide where catalogs come from.
//!
//! # Glyph safety
//!
//! Glaux draws text with a bitmap font that has no combining marks, and the
//! Unicode isolation characters Fluent normally inserts around placeables
//! are not drawable either. Isolation is therefore always off. Catalogs
//! should use precomposed letters, and [`Catalog::resolve_samples`] returns
//! every text a message can produce so a host can check each character
//! against its font.

mod catalog;
mod error;
mod language;
mod translator;
pub mod validate;

pub use catalog::{Catalog, SAMPLE_NUMBERS, SAMPLE_TEXT, Samples};
pub use error::{CatalogError, SyntaxIssue};
pub use language::{FallbackChain, Language, LanguageError};
pub use translator::{Arg, Translator};
