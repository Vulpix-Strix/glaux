//! How the Code view moves between places: asking for a definition, the symbol panel,
//! and showing the caret after a jump.

use super::symbols::{PanelAction, PanelLayout, Row, SymbolPanel};
use super::{CodeEditor, CodeRequest};
use crate::system::editing::document::DocumentId;
use crate::system::editing::session::{CodeSession, DocumentView};

impl CodeEditor {
    /// Where the symbol panel's parts sit, while it is open.
    pub(super) fn panel_layout(&self) -> Option<PanelLayout> {
        self.symbols.as_ref()?;
        Some(PanelLayout::new(&self.layout))
    }

    /// Asks to go to the definition of the name at `caret`, or at the active document's
    /// caret when none is given.
    pub(super) fn definition_request(
        &self,
        session: &CodeSession,
        caret: Option<usize>,
    ) -> Option<CodeRequest> {
        let id = session.tabs().active()?;
        let caret = caret.or_else(|| Some(session.tabs().selection(id)?.caret().byte()))?;
        Some(CodeRequest::GoToDefinition { id, caret })
    }

    /// Opens the symbol panel with the keys, gives it the keys if it was open without
    /// them, or closes it if it already had them.
    pub(super) fn toggle_symbols(&mut self, session: &mut CodeSession) {
        if session.active_document().is_none() {
            return;
        }
        match &mut self.symbols {
            None => self.symbols = Some(SymbolPanel::new()),
            Some(panel) if !panel.focused() => panel.focus(),
            Some(_) => self.symbols = None,
        }
        self.sync_symbols(session);
    }

    /// Applies `event` to the symbol panel. `None` when the panel does not take it.
    pub(super) fn symbols_input(
        &mut self,
        event: glaux_input::InputEvent,
        session: &mut CodeSession,
    ) -> Option<Option<CodeRequest>> {
        let layout = self.panel_layout()?;
        let pointer = self.pointer;
        let action = self.symbols.as_mut()?.handle_input(event, &layout, pointer);
        match action {
            PanelAction::Ignored => None,
            PanelAction::Handled => Some(None),
            PanelAction::Go(index) => {
                let path = session.active_document()?.path();
                let target = self.symbols.as_ref()?.target(index, path)?;
                Some(Some(CodeRequest::Jump(target)))
            }
        }
    }

    /// Lists the active document's declarations again if its text changed since they
    /// were read, and keeps the highlighted one on the caret's declaration.
    pub(super) fn sync_symbols(&mut self, session: &mut CodeSession) {
        let Some(layout) = self.panel_layout() else {
            return;
        };
        let Some(panel) = self.symbols.as_mut() else {
            return;
        };
        let Some(document) = session.active_document() else {
            panel.set_rows(None, Vec::new());
            return;
        };
        let (id, revision) = (document.id(), document.revision());
        if panel.source() != Some((id, revision)) {
            let symbols = session.symbols(id).unwrap_or_default();
            let rows = session.document(id).map_or_else(Vec::new, |document| {
                symbols
                    .into_iter()
                    .map(|symbol| Row {
                        line: document.line_of(symbol.declared.start),
                        symbol,
                    })
                    .collect()
            });
            panel.set_rows(Some((id, revision)), rows);
        }
        let caret = session
            .tabs()
            .selection(id)
            .unwrap_or_default()
            .caret()
            .byte();
        panel.follow(caret, layout.visible);
    }

    /// Shows the caret of `id` after a jump: the tab, the document scrolled to it and
    /// the popups of the place left closed. `top_line` is the line to start at when the
    /// view should come back as it was, and otherwise the caret is a third of the way
    /// down.
    pub fn show_arrival(
        &mut self,
        session: &mut CodeSession,
        id: DocumentId,
        top_line: Option<usize>,
    ) {
        self.completion = None;
        self.signature = None;
        self.close_hover();
        self.refresh_metrics(session);
        self.reveal_active(session);
        let Some(document) = session.document(id) else {
            return;
        };
        let selection = session.tabs().selection(id).unwrap_or_default();
        let caret = selection.caret();
        let geometry = self.geometry(document);
        let top = top_line.unwrap_or_else(|| {
            document
                .line_of(caret.byte())
                .saturating_sub(geometry.rows / 3)
        });
        let view = session.tabs().metrics(id).map(|metrics| {
            let wanted = DocumentView {
                top_line: top,
                top_row: 0,
                left_column: 0,
            };
            geometry.clamp(wanted, metrics)
        });
        if let (Some(view), Some(stored)) = (view, session.tabs_mut().view_mut(id)) {
            *stored = view;
        }
        let view = self.revealing(session, id, caret);
        self.show_selection(session, id, selection, view);
        self.sync_symbols(session);
    }
}
