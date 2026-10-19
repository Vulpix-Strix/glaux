//! The signature of the call being typed, shown over the text with the
//! argument the caret is on set off.

mod layout;
mod render;
mod view;

use glaux_docs::DocId;
use glaux_input::InputEvent;
use glaux_sense::SignatureHelp;

use super::popup::Place;
use super::{CodeEditor, CodeRequest};
use crate::system::code_document::{CodeDocument, DocumentId};
use crate::system::code_session::CodeSession;
use crate::system::code_text::spot;
use crate::system::docs;
use crate::system::workspace::Rect;
pub(super) use layout::SignatureLayout;
use view::{SignatureView, ViewSource, view_for};

/// The status bar hint while the signature is shown.
pub const SIGNATURE_HINT: &str = "Esc: close";

/// The call the caret is typing arguments for, as it was when the engine
/// last looked at the text. See [`is_current`](Self::is_current).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignaturePopup {
    document: DocumentId,
    help: SignatureHelp,
    view: SignatureView,
}

impl SignaturePopup {
    /// Whether the popup still describes `document`: the same text, so the
    /// offsets in the answer are still right.
    pub fn is_current(&self, document: &CodeDocument) -> bool {
        document.id() == self.document && document.revision() == self.help.revision
    }

    #[cfg(test)]
    pub fn help(&self) -> &SignatureHelp {
        &self.help
    }

    #[cfg(test)]
    pub fn view(&self) -> &SignatureView {
        &self.view
    }
}

/// A call whose popup Escape closed. Typing on inside it does not bring the
/// popup back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dismissed {
    document: DocumentId,
    /// The offset of the call's opening parenthesis.
    open: usize,
}

/// Whether typing `event` may bring the popup up.
fn opens(event: InputEvent) -> bool {
    matches!(event, InputEvent::Text('(' | ','))
}

impl CodeEditor {
    /// Opens, refreshes or closes the popup after `event` was applied.
    /// `revision` is the active document's before the event.
    ///
    /// Typing a parenthesis or a comma that changed the text opens the popup
    /// when the caret is then in a call, and so does asking for it. Once it is
    /// open, every event refreshes it, so it follows the caret into other
    /// arguments and calls and goes when the caret leaves them. Pasting,
    /// undoing and moving never bring it up.
    pub(super) fn update_signature(
        &mut self,
        event: InputEvent,
        revision: Option<u64>,
        session: &mut CodeSession,
    ) {
        let manual = event == InputEvent::SignatureHelp;
        let typed =
            opens(event) && session.active_document().map(CodeDocument::revision) != revision;
        if !(manual || typed || self.signature.is_some() || self.signature_dismissed.is_some()) {
            return;
        }
        let Some((document, help)) = self.signature_at_caret(session) else {
            self.signature = None;
            self.signature_dismissed = None;
            return;
        };
        if manual {
            self.signature_dismissed = None;
        }
        match self.signature_dismissed {
            Some(dismissed) if dismissed.document == document && dismissed.open == help.open => {
                self.signature = None;
                return;
            }
            _ => self.signature_dismissed = None,
        }
        if !(manual || typed || self.signature.is_some()) {
            return;
        }
        self.signature = self.signature_popup(session, document, help);
    }

    /// The call the active document's caret is typing arguments for.
    fn signature_at_caret(&self, session: &mut CodeSession) -> Option<(DocumentId, SignatureHelp)> {
        let (id, selection, _) = self.editable(session)?;
        if !selection.is_empty() {
            return None;
        }
        let help = session.signature_help(id, selection.caret().byte(), docs::library().api())?;
        Some((id, help))
    }

    fn signature_popup(
        &self,
        session: &CodeSession,
        document: DocumentId,
        help: SignatureHelp,
    ) -> Option<SignaturePopup> {
        let text = session.document(document)?;
        let meaning = |page: &DocId, param: &str| {
            let page = docs::library().page(page, docs::chain())?.page;
            page.parameters()?
                .iter()
                .find(|row| row.name == param)
                .map(|row| row.meaning.clone())
                .filter(|meaning| !meaning.is_empty())
        };
        let line_of = |byte: usize| text.line_of(byte) + 1;
        let view = view_for(
            &help,
            &ViewSource {
                meaning: &meaning,
                line_of: &line_of,
            },
        );
        Some(SignaturePopup {
            document,
            help,
            view,
        })
    }

    /// Handles `event` while the popup is up. Only Escape is for it: it
    /// closes the popup and keeps it closed for the call. `None` passes the
    /// event on.
    pub(super) fn signature_input(&mut self, event: InputEvent) -> Option<Option<CodeRequest>> {
        let popup = self.signature.as_ref()?;
        if event != InputEvent::Break {
            return None;
        }
        self.signature_dismissed = Some(Dismissed {
            document: popup.document,
            open: popup.help.open,
        });
        self.signature = None;
        Some(None)
    }

    /// Where the popup is drawn, or `None` when it is not showing: it is
    /// closed, it no longer fits the text, the caret is scrolled out of
    /// sight, or it would cover the list of suggestions.
    pub(super) fn signature_layout(&self, session: &CodeSession) -> Option<SignatureLayout> {
        let popup = self.signature.as_ref()?;
        let document = session.active_document()?;
        let id = document.id();
        let selection = session.tabs().selection(id)?;
        if !selection.is_empty() || !popup.is_current(document) {
            return None;
        }
        let geometry = self.geometry(document);
        let tabs = session.tabs();
        let (view, metrics) = (tabs.view(id)?, tabs.metrics(id)?);
        if !metrics.describes(document, geometry.metrics_key()) {
            return None;
        }
        let font = &self.layout.font;
        let point = |byte| {
            let at = spot(document, byte, metrics.key(), font);
            geometry.caret_point(*view, &at, metrics)
        };
        let caret = point(selection.caret().byte())?;
        // Under the opening parenthesis when it is on the caret's row, so the
        // popup stays put while the arguments are typed.
        let x = point(popup.help.open)
            .filter(|open| open.1 == caret.1)
            .map_or(caret.0, |open| open.0);
        let place = Place {
            font,
            area: self.text_area(),
            cell_w: geometry.cell_w,
            cell_h: geometry.cell_h,
        };
        let mut avoid: Vec<Rect> = Vec::new();
        if let Some(suggestions) = self.completion_layout(session) {
            avoid.push(suggestions.list);
            avoid.extend(suggestions.panel.map(|panel| panel.window));
        }
        SignatureLayout::new(place, &popup.view, (x, caret.1), &avoid)
    }
}
