//! Works out which documentation page F1 should open from what the caret is on.

use glaux_sense::Doc;

use super::{CodeEditor, CodeRequest};
use crate::system::code_session::{CodeSession, Symbol};
use crate::system::docs;
use crate::system::workspace::docs::DocTarget;
use crate::ui_text;

/// What F1 found.
enum Lookup {
    Page(DocTarget),
    /// A name with no page, such as a function of the script's own.
    Missing(String),
    Nothing,
}

/// Where the page of `doc` is, or that there is none for `name`.
fn lookup(doc: &Doc, name: &str, parameters: bool) -> Lookup {
    match doc {
        Doc::Api(id) if parameters => Lookup::Page(DocTarget::parameters(id.clone())),
        Doc::Api(id) => Lookup::Page(DocTarget::page(id.clone())),
        Doc::Local(_) | Doc::None => Lookup::Missing(name.to_owned()),
    }
}

impl CodeEditor {
    /// What F1 asks of the workspace. The suggestion picked in the list
    /// comes first, then the call being typed, then the word at the caret,
    /// and with none of them the documentation's search. The popups close,
    /// and neither the text, the caret nor the history change.
    pub(super) fn docs_request(&mut self, session: &mut CodeSession) -> CodeRequest {
        self.sync_completion(session);
        let found = self.docs_lookup(session);
        self.completion = None;
        self.signature = None;
        match found {
            Lookup::Page(target) => CodeRequest::OpenDocs(target),
            Lookup::Missing(name) => CodeRequest::Notice(ui_text::get_with(
                "code-no-documentation-for",
                &[("value", glaux_i18n::Arg::Text(&name))],
            )),
            Lookup::Nothing => CodeRequest::SearchDocs,
        }
    }

    fn docs_lookup(&self, session: &mut CodeSession) -> Lookup {
        if let Some(candidate) = self.completion.as_ref().and_then(|popup| popup.picked()) {
            return lookup(&candidate.doc, &candidate.label, false);
        }
        let current = session.active_document().and_then(|document| {
            self.signature
                .as_ref()
                .filter(|popup| popup.is_current(document))
        });
        if let Some(popup) = current {
            let signature = popup.signature();
            return lookup(&signature.doc, &signature.name, true);
        }
        let Some(document) = session.active_document() else {
            return Lookup::Nothing;
        };
        let id = document.id();
        let Some(selection) = session.tabs().selection(id) else {
            return Lookup::Nothing;
        };
        match session.symbol_at(id, selection.caret().byte(), docs::library().api()) {
            Some(Symbol::Api(page)) => Lookup::Page(DocTarget::page(page)),
            Some(Symbol::Local(name)) => Lookup::Missing(name),
            Some(Symbol::Unknown) | None => Lookup::Nothing,
        }
    }
}
