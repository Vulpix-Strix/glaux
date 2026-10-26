//! What the language tooling says about an open script: its highlighting,
//! completions, signature help, hover and symbols.

use std::ops::Range;

use glaux_docs::{ApiCatalog, DocId, ScriptLanguage};
use glaux_sense::{
    Completions, Hover, Request, Resolution, SignatureHelp, SignatureRequest, Subject, Trigger,
};

use super::CodeSession;
use crate::cartridge::Language;
use crate::system::editing::document::DocumentId;
use crate::system::editing::syntax::DocumentSyntax;

/// What a word in a script refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Symbol {
    /// A name of the script API, with the id of its documentation page.
    Api(DocId),
    /// Something the script defines, which has no page.
    Local(String),
    /// A word that names nothing the engine knows.
    Unknown,
}

fn script_language(language: Language) -> ScriptLanguage {
    match language {
        Language::Luau => ScriptLanguage::Luau,
        Language::Rhai => ScriptLanguage::Rhai,
    }
}

impl CodeSession {
    /// Brings `id`'s highlighting up to date for `lines`, the lines shown,
    /// parsing again only if the text changed. Does nothing if `id` is not
    /// open.
    pub fn refresh_syntax(&mut self, id: DocumentId, lines: Range<usize>) {
        let Some(document) = self.documents.get(id) else {
            return;
        };
        let last = document.line_count().saturating_sub(1);
        let start = document
            .line_range(lines.start.min(last))
            .map_or(0, |range| range.start);
        let end = document
            .line_range(lines.end.saturating_sub(1).min(last))
            .map_or(0, |range| range.end);
        self.syntax
            .entry(id)
            .or_insert_with(|| DocumentSyntax::new(document.language()))
            .refresh(document, start..end.max(start));
    }

    /// The highlight spans of the lines last given to
    /// [`refresh_syntax`](Self::refresh_syntax), sorted and apart.
    pub fn syntax_spans(&self, id: DocumentId) -> &[glaux_syntax::Span] {
        self.syntax.get(&id).map_or(&[], DocumentSyntax::spans)
    }

    /// What can be written at `caret` in `id`, answered from the text as it
    /// is now, unsaved edits included. The answer carries the document's
    /// revision so it can be told apart from a later text. `indent` is one
    /// step of indentation for inserted declarations. `None` if `id` is not
    /// open or its syntax is unavailable.
    pub fn completions(
        &mut self,
        id: DocumentId,
        caret: usize,
        trigger: Trigger,
        indent: &str,
        api: &ApiCatalog,
    ) -> Option<Completions> {
        let document = self.documents.get(id)?;
        let highlighter = self
            .syntax
            .entry(id)
            .or_insert_with(|| DocumentSyntax::new(document.language()))
            .parsed(document)?;
        let request = Request {
            language: script_language(document.language()),
            text: document.text(),
            caret,
            revision: document.revision(),
            trigger,
            line_ending: document.line_ending().as_str(),
            indent,
        };
        Some(glaux_sense::complete(&request, api, highlighter))
    }

    /// The call `id`'s caret at `caret` is typing arguments for, answered
    /// from the text as it is now, unsaved edits included. The answer
    /// carries the document's revision so it can be told apart from a later
    /// text. `None` if `id` is not open, its syntax is unavailable or the
    /// caret is not in a call to a function the engine knows.
    pub fn signature_help(
        &mut self,
        id: DocumentId,
        caret: usize,
        api: &ApiCatalog,
    ) -> Option<SignatureHelp> {
        let document = self.documents.get(id)?;
        let highlighter = self
            .syntax
            .entry(id)
            .or_insert_with(|| DocumentSyntax::new(document.language()))
            .parsed(document)?;
        let request = SignatureRequest {
            language: script_language(document.language()),
            text: document.text(),
            caret,
            revision: document.revision(),
        };
        glaux_sense::signature_help(&request, api, highlighter)
    }

    /// What the word at `caret` in `id` refers to, answered from the text as
    /// it is now. `None` if `id` is not open, its syntax is unavailable or
    /// there is no word there to look up, as in a comment or a string.
    pub fn symbol_at(&mut self, id: DocumentId, caret: usize, api: &ApiCatalog) -> Option<Symbol> {
        Some(match self.hover_at(id, caret, api)?.subject {
            Subject::Entry(entry) | Subject::Constant { group: entry, .. } => {
                Symbol::Api(entry.id().clone())
            }
            Subject::Local(definition) => Symbol::Local(definition.name),
            Subject::Declared(name) => Symbol::Local(name),
            Subject::Unknown => Symbol::Unknown,
        })
    }

    /// The word around `offset` in `id` and what it refers to, answered from
    /// the text as it is now. `None` under the same conditions as
    /// [`CodeSession::symbol_at`].
    pub fn hover_at<'a>(
        &mut self,
        id: DocumentId,
        offset: usize,
        api: &'a ApiCatalog,
    ) -> Option<Hover<'a>> {
        let document = self.documents.get(id)?;
        let highlighter = self
            .syntax
            .entry(id)
            .or_insert_with(|| DocumentSyntax::new(document.language()))
            .parsed(document)?;
        let language = script_language(document.language());
        glaux_sense::hover(api, language, document.text(), offset, highlighter)
    }

    /// What the name at `caret` in `id` leads to within that document and
    /// its imports, answered from the text as it is now. Finding the other
    /// document is left to the caller. `None` if `id` is not open or its
    /// syntax is unavailable.
    pub fn resolve_at(&mut self, id: DocumentId, caret: usize) -> Option<Resolution> {
        let document = self.documents.get(id)?;
        let highlighter = self
            .syntax
            .entry(id)
            .or_insert_with(|| DocumentSyntax::new(document.language()))
            .parsed(document)?;
        let language = script_language(document.language());
        Some(glaux_sense::definition(
            language,
            document.text(),
            caret,
            highlighter,
        ))
    }

    /// The declarations of `id`'s text as it is now, for an outline. The syntax is
    /// parsed again only if the text changed. `None` if `id` is not open or its syntax
    /// is unavailable.
    pub fn symbols(&mut self, id: DocumentId) -> Option<Vec<glaux_syntax::Symbol>> {
        let document = self.documents.get(id)?;
        let highlighter = self
            .syntax
            .entry(id)
            .or_insert_with(|| DocumentSyntax::new(document.language()))
            .parsed(document)?;
        Some(highlighter.symbols())
    }
}
