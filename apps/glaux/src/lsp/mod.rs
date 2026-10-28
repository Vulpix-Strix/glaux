//! Diagnostics from `luau-lsp`, run beside the game and never inside it.
//!
//! The system stays pure: it only says which scripts are open and takes the
//! findings back. Everything about processes, files and JSON-RPC lives
//! here and in `glaux-lsp`.
//!
//! The analyzer is optional. Without the executable, or with the setting
//! off, the editor works exactly as before.

mod definitions;
mod locate;
mod service;
mod translate;
mod workspace;

pub(crate) use service::Analysis;
