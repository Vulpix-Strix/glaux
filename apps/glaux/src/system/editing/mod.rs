//! The script documents of the loaded cartridge and what is done to them,
//! apart from how they are shown.
//!
//! [`document`] holds the text and, in its `store`, reads and writes the
//! files. [`cursor`], [`selection`], [`edit`], [`history`] and [`search`]
//! are the operations on a document. [`session`] ties the open documents
//! to their tabs, undo history and syntax. [`metrics`] maps a document onto
//! the character cells of a view, which vertical caret movement needs.
//!
//! Nothing here draws or reads input. The workspace shows these and turns
//! keys into calls on them.

pub(super) mod cursor;
pub(super) mod document;
pub(super) mod edit;
pub(super) mod history;
pub(super) mod metrics;
pub(super) mod search;
pub(super) mod selection;
pub(super) mod session;
pub(super) mod syntax;
