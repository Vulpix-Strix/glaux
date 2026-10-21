//! What every cartridge scripting runtime shares, independent of any
//! language: the draw commands a script records, the log entries it writes,
//! how its failures are reported, and the validation of the values it
//! passes to Glaux.
//!
//! Runtimes convert their own values to the plain Rust types the
//! [`validate`] functions take, and wrap the detail text those return in
//! errors located at the calling script line.

pub mod draw;
pub mod error;
pub mod log;
pub mod validate;

pub use draw::{
    DrawBudgetExceeded, DrawCommand, MAX_DRAW_COMMANDS, Recording, SPRITE_TILE_COST, TEXT_CHAR_COST,
};
pub use error::{Phase, RequireError, RequireFailure, RuntimeError, ScriptError, StartFailure};
pub use log::{
    LogEntry, LogLevel, LogSink, MAX_ENTRY_BYTES, MAX_ENTRY_LINES, MAX_ORDINARY_ENTRIES,
    MAX_WARN_ENTRIES, cut_entry,
};

#[cfg(test)]
mod tests;
