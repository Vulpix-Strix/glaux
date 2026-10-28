//! A small asynchronous client for language servers.
//!
//! The crate runs servers as child processes and speaks JSON-RPC to them over
//! stdio on a dedicated Tokio runtime. The caller lives on an ordinary
//! thread, such as a game loop, and only ever touches [`LspHandle`], whose
//! methods never wait for I/O.

pub mod documents;
pub mod frame;
pub mod protocol;
pub mod uri;

mod runtime;
mod session;
mod stats;

pub use runtime::{DocumentInit, Event, LspError, LspHandle, LspRuntime, ServerSpec, SessionKey};
pub use stats::StatsSnapshot;
