//! Setting and clearing breakpoints from the Code view.

use glaux_input::Point;

use super::CodeEditor;
use crate::system::editing::session::CodeSession;

impl CodeEditor {
    /// Sets the breakpoint on the caret's line, or clears it if set.
    pub(super) fn toggle_breakpoint_at_caret(&mut self, session: &mut CodeSession) {
        let Some(document) = session.active_document() else {
            return;
        };
        let id = document.id();
        let caret = session.tabs().selection(id).unwrap_or_default().caret();
        let line = document.line_of(caret.byte());
        session.toggle_breakpoint(id, line);
    }

    /// Handles a primary press at `at` if it is on the line numbers, which
    /// set or clear the breakpoint of the line pressed. Returns whether it
    /// was one.
    pub(super) fn press_gutter(&mut self, session: &mut CodeSession, at: Point) -> bool {
        let Some(document) = session.active_document() else {
            return false;
        };
        let Some(gutter) = self.geometry(document).gutter else {
            return false;
        };
        if !gutter.contains(at) {
            return false;
        }
        if let Some((id, byte)) = self.hit_text(session, at)
            && let Some(document) = session.document(id)
        {
            let line = document.line_of(byte);
            session.toggle_breakpoint(id, line);
        }
        true
    }
}
