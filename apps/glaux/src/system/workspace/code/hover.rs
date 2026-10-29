//! A quick look at the word under a pointer that has come to rest, with the
//! documentation's summary and a way into the full page.

mod layout;
mod render;
mod view;

use std::ops::Range;

use glaux_docs::{DocId, ScriptLanguage};
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::machine::TICK_RATE;

use super::popup::Place;
use super::{CodeEditor, CodeRequest};
use crate::cartridge::Language;
use crate::system::docs;
use crate::system::editing::document::{CodeDocument, DocumentId};
use crate::system::editing::metrics::spot;
use crate::system::editing::session::CodeSession;
use crate::system::workspace::Rect;
use crate::system::workspace::detail::Detail;
use crate::system::workspace::docs::DocTarget;
use crate::ui_text;
pub(super) use layout::HoverLayout;
use view::{HoverView, ViewSource, view_for};

/// Ticks the pointer must rest on a word before the popup opens.
pub const HOVER_DELAY_TICKS: u32 = TICK_RATE / 2;

/// The status bar hint while the popup is shown.
pub fn hover_hint(opens_page: bool) -> String {
    if opens_page {
        ui_text::get("code-click-open-docs-tab-more-esc-close")
    } else {
        ui_text::get("code-tab-more-esc-close")
    }
}

/// The word the popup describes, as it was when the engine last looked at the
/// text. See [`is_current`](Self::is_current).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverPopup {
    document: DocumentId,
    revision: u64,
    /// Where the word is in the text.
    range: Range<usize>,
    view: HoverView,
    /// The page a click on the popup opens, if the word has one.
    target: Option<DocTarget>,
}

impl HoverPopup {
    /// Whether the popup still describes `document`: the same text, so the
    /// word is still where it was.
    pub fn is_current(&self, document: &CodeDocument) -> bool {
        document.id() == self.document && document.revision() == self.revision
    }

    pub fn target(&self) -> Option<&DocTarget> {
        self.target.as_ref()
    }

    #[cfg(test)]
    pub fn view(&self) -> &HoverView {
        &self.view
    }
}

/// A pointer that stopped moving, and for how long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rest {
    at: Point,
    ticks: u32,
}

impl CodeEditor {
    /// Counts the pointer's rest and opens the popup once it has lasted
    /// [`HOVER_DELAY_TICKS`].
    pub(super) fn tick_hover(&mut self, session: &mut CodeSession) {
        let Some(rest) = &mut self.rest else {
            return;
        };
        rest.ticks = rest.ticks.saturating_add(1);
        let Rest { at, ticks } = *rest;
        if ticks != HOVER_DELAY_TICKS || self.hover.is_some() || self.hover_blocked() {
            return;
        }
        self.hover = self.hover_at(session, at);
    }

    /// Closes the popup and forgets the pointer's rest.
    pub fn close_hover(&mut self) {
        self.hover = None;
        self.rest = None;
    }

    /// Whether something else has the pointer or the keys, so no popup
    /// should open.
    fn hover_blocked(&self) -> bool {
        self.has_overlay()
            || self.completion.is_some()
            || self.signature.is_some()
            || self.find_focused()
            || self.selecting.is_some()
            || self.drag.is_some()
    }

    /// Opens the popup for the word under `at`, if there is one with
    /// something to say.
    fn hover_at(&self, session: &mut CodeSession, at: Point) -> Option<HoverPopup> {
        if !self.text_area().contains(at) {
            return None;
        }
        let (id, offset) = self.hit_text(session, at)?;
        let hover = session.hover_at(id, offset, docs::library().api())?;
        let document = session.document(id)?;
        let language = match document.language() {
            Language::Luau => ScriptLanguage::Luau,
            Language::Rhai => ScriptLanguage::Rhai,
        };
        let summary = |id: &DocId| {
            let page = docs::library().page(id, docs::chain())?.page;
            page.summary().map(str::to_owned)
        };
        let meaning = |page: &DocId, param: &str| {
            let page = docs::library().page(page, docs::chain())?.page;
            page.parameters()?
                .iter()
                .find(|row| row.name == param)
                .map(|row| row.meaning.clone())
                .filter(|meaning| !meaning.is_empty())
        };
        let line_of = |byte: usize| document.line_of(byte) + 1;
        let (view, target) = view_for(
            &hover.subject,
            &ViewSource {
                language,
                summary: &summary,
                meaning: &meaning,
                line_of: &line_of,
            },
        )?;
        let popup = HoverPopup {
            document: id,
            revision: document.revision(),
            range: hover.range,
            view,
            target,
        };
        // Resting on the gap beside a word, or on its edge, is not resting on it.
        let (_, word) = self.hover_layout_of(&popup, session)?;
        word.contains(at).then_some(popup)
    }

    /// Where the popup and the word it describes are drawn, or `None` when
    /// it is not showing: it is closed, it no longer fits, or its word is
    /// scrolled out of sight.
    pub(super) fn hover_layout(&self, session: &CodeSession) -> Option<(HoverLayout, Rect)> {
        self.hover_layout_of(self.hover.as_ref()?, session)
    }

    fn hover_layout_of(
        &self,
        popup: &HoverPopup,
        session: &CodeSession,
    ) -> Option<(HoverLayout, Rect)> {
        let document = session.active_document()?;
        if !popup.is_current(document) {
            return None;
        }
        let id = document.id();
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
        let area = self.text_area();
        let start = point(popup.range.start)?;
        // A word broken over two rows is hovered on its first.
        let end = point(popup.range.end)
            .filter(|end| end.1 == start.1)
            .map_or(area.right(), |end| end.0);
        let word = Rect {
            x: start.0,
            y: start.1,
            width: (end - start.0).max(geometry.cell_w),
            height: geometry.cell_h,
        };
        let place = Place {
            font,
            area,
            cell_w: geometry.cell_w,
            cell_h: geometry.cell_h,
        };
        Some((HoverLayout::new(place, &popup.view, word)?, word))
    }

    /// Follows the pointer and the keys while the popup may be showing.
    /// Moving over the word or the popup keeps it, a press on the popup
    /// opens its page, Escape closes it, and anything else closes it and
    /// goes on to the view. `None` passes the event on.
    pub(super) fn hover_input(
        &mut self,
        event: InputEvent,
        session: &CodeSession,
    ) -> Option<Option<CodeRequest>> {
        match event {
            InputEvent::PointerMove(Some(at)) => {
                let inside = self.hover_layout(session).is_some_and(|(layout, word)| {
                    layout.hit_area().contains(at) || word.contains(at)
                });
                if inside {
                    self.rest = None;
                } else {
                    self.hover = None;
                    self.rest = Some(Rest { at, ticks: 0 });
                }
                None
            }
            InputEvent::PointerMove(None) => {
                self.close_hover();
                None
            }
            // Releasing the press that opened a page, or any other, says
            // nothing about the popup. F1 reads it before closing it.
            InputEvent::PointerUp { .. } | InputEvent::PointerCancel | InputEvent::Docs => None,
            _ if self.hover.is_none() => {
                self.rest = None;
                None
            }
            InputEvent::Break => {
                self.close_hover();
                Some(None)
            }
            // The popup keeps only what fits, so Tab opens all of it.
            InputEvent::Tab { .. } => {
                if let Some(popup) = self.hover.take() {
                    self.detail = Some(Detail::new(
                        &self.layout.font,
                        self.layout.text,
                        &ui_text::get("code-hover-details"),
                        popup.view.tag.as_deref().unwrap_or_default(),
                        &popup.view.full_text(),
                    ));
                }
                self.close_hover();
                Some(None)
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } if self
                .hover_layout(session)
                .is_some_and(|(layout, _)| layout.hit_area().contains(at)) =>
            {
                let target = self.hover.take().and_then(|popup| popup.target);
                self.close_hover();
                Some(target.map(CodeRequest::OpenDocs))
            }
            _ => {
                self.close_hover();
                None
            }
        }
    }

    /// Drops the popup once something else is showing or its text changed.
    pub(super) fn sync_hover(&mut self, session: &CodeSession) {
        let stale = self.hover.as_ref().is_some_and(|popup| {
            session
                .active_document()
                .is_none_or(|document| !popup.is_current(document))
        });
        if stale || self.completion.is_some() || self.signature.is_some() || self.has_overlay() {
            self.close_hover();
        }
    }

    /// Whether `at` is over a popup that a click opens a page from.
    pub(super) fn hover_opens_at(&self, at: Point, session: Option<&CodeSession>) -> bool {
        let Some(session) = session else {
            return false;
        };
        self.hover
            .as_ref()
            .is_some_and(|popup| popup.target.is_some())
            && self
                .hover_layout(session)
                .is_some_and(|(layout, _)| layout.hit_area().contains(at))
    }
}
