//! The cartridge's script documents and the operations on them. Nothing here
//! draws or reads input, so the workspace can show a document and turn keys
//! into calls on it.
//!
//! [`metrics`] sits here, and not with the views, because vertical caret
//! movement depends on how a document wraps into character cells.

pub(super) mod cursor;
pub(super) mod document;
pub(super) mod edit;
pub(super) mod history;
pub(super) mod metrics;
pub(super) mod search;
pub(super) mod selection;
pub(super) mod session;
pub(super) mod syntax;
