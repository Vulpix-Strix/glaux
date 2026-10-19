//! Code completion for Glaux scripts.
//!
//! [`complete`] suggests what can be written at the caret, using the text, the API
//! catalog and the parse tree from [`glaux_syntax`]. The engine has no user interface
//! and keeps nothing between requests, so the editor decides when to ask and what to
//! do with the answer.
//!
//! ```
//! use glaux_docs::{Library, ScriptLanguage};
//! use glaux_sense::{Request, Trigger, complete};
//! use glaux_syntax::{Grammar, Highlighter};
//!
//! let text = "audio.";
//! let mut syntax = Highlighter::new(Grammar::Luau).unwrap();
//! syntax.update(text);
//!
//! let request = Request {
//!     language: ScriptLanguage::Luau,
//!     text,
//!     caret: text.len(),
//!     revision: 0,
//!     trigger: Trigger::Member,
//!     line_ending: "\n",
//!     indent: "  ",
//! };
//! let answer = complete(&request, Library::embedded().api(), &syntax);
//! assert!(answer.items.iter().any(|item| item.label == "tone"));
//! ```

mod complete;
mod context;
mod lexical;
mod resolve;

pub use complete::{
    Candidate, CandidateKind, Completions, Doc, Insertion, MAX_CANDIDATES, Origin, Request,
    Trigger, complete,
};
pub use context::{Context, Position, Separator, context};
pub use resolve::{Resolved, constant_members, resolve};
