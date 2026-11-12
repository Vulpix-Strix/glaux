//! Persistent Luau runtime for cartridge scripts.
//!
//! A [`Runtime`] owns one VM for the whole time a cartridge runs. Starting it
//! executes the entry chunk and calls the script's optional `boot()`, and each
//! [`Runtime::tick`] calls the optional `tick()` and `draw()`. Every call runs
//! under a [`Budget`] and the heap under [`MEMORY_LIMIT`]. The scripting API
//! is documented in the repository's `docs/en/api/` pages, and [`definitions`]
//! renders it as type definitions for `luau-lsp`.

/// Where the generated definitions are kept, relative to this crate.
pub const DEFINITIONS_PATH: &str = "definitions/glaux.d.luau";

/// The committed definitions, for hosts that hand them to a type checker
/// without the repository at hand. A test keeps them equal to what
/// [`definitions`] generates.
pub const DEFINITIONS: &str = include_str!("../definitions/glaux.d.luau");

mod api;
mod args;
mod debug;
mod definitions;
mod error;
mod modules;
mod runtime;

pub use definitions::{DefinitionsError, definitions};
pub use glaux_script::{
    DrawCommand, LogEntry, LogLevel, MAX_DRAW_COMMANDS, Phase, RequireError, RequireFailure,
    RuntimeError, SPRITE_TILE_COST, ScriptError, StartFailure, TEXT_CHAR_COST,
};
pub use runtime::{Budget, DEFAULT_INTERRUPT_BUDGET, MEMORY_LIMIT, Runtime};

#[cfg(test)]
mod tests;
