//! Code completion, signature help and hover for Glaux scripts.
//!
//! [`complete`] suggests what can be written at the caret, [`signature_help`]
//! describes the call the caret is typing arguments for, and [`hover`] says what the
//! word under a pointer refers to. They use the text, the API
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
mod hover;
mod lexical;
mod resolve;
mod signature;

pub use complete::{
    Candidate, CandidateKind, Completions, Doc, Insertion, MAX_CANDIDATES, Origin, Request,
    Trigger, complete,
};
pub use context::{Context, Position, Separator, context};
pub use hover::{Hover, Subject, hover};
pub use resolve::{Resolved, constant_members, resolve};
pub use signature::{ParamInfo, Signature, SignatureHelp, SignatureRequest, signature_help};
