//! The embedded documentation and the languages it is read in, shared by
//! everything in the interface that shows it.

use std::sync::OnceLock;

use glaux_docs::Library;
use glaux_i18n::FallbackChain;

/// The embedded documentation, parsed the first time something needs it.
pub fn library() -> &'static Library {
    static LIBRARY: OnceLock<Library> = OnceLock::new();
    LIBRARY.get_or_init(Library::embedded)
}

/// The languages the documentation is read in, in order of preference.
pub fn chain() -> &'static FallbackChain {
    static CHAIN: OnceLock<FallbackChain> = OnceLock::new();
    CHAIN.get_or_init(|| crate::ui_text::translator().chain().clone())
}
