//! Persistent Rhai runtime for cartridge scripts. **Experimental:** the
//! scripting API may change without backward compatibility guarantees, and
//! the runtime has no limit on the heap a script holds, so it must not run
//! cartridges you do not trust.
//!
//! A [`Runtime`] owns one engine and one compiled script for the whole time a
//! cartridge runs. Starting it runs the top level and calls the optional
//! `boot()`, and each [`Runtime::tick`] calls the optional `tick()` and
//! `draw()`, with the script's state bound as `this`. Every call runs under a
//! [`Budget`]. The scripting API is documented in the repository's
//! `docs/en/api/` pages.

mod api;
mod args;
mod debug;
mod error;
mod modules;
mod runtime;

pub use runtime::{Budget, DEFAULT_OPERATION_BUDGET, Runtime};

#[cfg(test)]
mod tests;
