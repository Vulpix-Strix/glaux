//! Documentation, API metadata and their validation for Glaux.
//!
//! Everything shipped as documentation lives in the repository's `docs/`
//! directory and is embedded into the binary, so it is versioned with each
//! release and works offline. See [`Library`].

pub mod catalog;
mod diagnostic;
mod id;
mod ids_lock;
mod library;
mod page;
pub mod validate;

pub use catalog::{ApiCatalog, ApiEntry, Binding, Kind, Param, ScriptLanguage, ValueType};
pub use diagnostic::{Diagnostic, Severity};
pub use id::{DocId, DocIdError, URI_SCHEME};
pub use ids_lock::{IdsLock, IdsLockError};
pub use library::{Library, Resolved};
pub use page::{ImageRef, LinkRef, Page, Scope, Section, TextRun};
