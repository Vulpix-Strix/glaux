//! The list of the active document's problems under the text.
//!
//! The rows are the findings the session already holds, in the order of
//! their position, so they read in the same order as F8 visits them.

use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_video::draw::{draw_text, fill_rect};

use glaux_i18n::Arg;
use glaux_input::{InputEvent, Point, PointerButton};

use super::layout::CodeLayout;
use super::render::severity_color;
use super::text_view::thumb;
use super::{CodeEditor, CodeRequest};
use crate::system::editing::diagnostics::{EditorDiagnostic, Severity};
use crate::system::editing::document::{CodeDocument, DocumentId};
use crate::system::editing::session::CodeSession;
use crate::system::screen::{BACKGROUND, DIM, PALE, TEXT};
use crate::system::text::clip;
use crate::system::workspace::BAR;
use crate::system::workspace::detail::Detail;
use crate::ui_text;

/// Side of the mark that tells the severity of a row.
const MARK: i32 = 5;
/// Space around a row's contents.
const PAD: i32 = 4;

/// The finding a row stands for, kept by what it says rather than by its
/// place in the list, because the list is rebuilt whenever the analyzer
/// answers again.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pick {
    document: DocumentId,
    start: usize,
    severity: Severity,
    code: Option<String>,
    message: String,
}

impl Pick {
    fn of(document: DocumentId, found: &EditorDiagnostic) -> Self {
        Self {
            document,
            start: found.range.start,
            severity: found.severity,
            code: found.code.clone(),
            message: found.message.clone(),
        }
    }

    fn says(&self, found: &EditorDiagnostic) -> bool {
        self.severity == found.severity && self.code == found.code && self.message == found.message
    }
}

/// Which row is picked and how far the list is scrolled.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct DiagList {
    focused: bool,
    pick: Option<Pick>,
    /// Index of the first row shown.
    top: usize,
}

impl DiagList {
    pub(super) fn focused(&self) -> bool {
        self.focused
    }

    /// Gives the list the keys, picking the finding at or after `caret`
    /// unless one is already picked. Returns whether the list took them,
    /// which it does not when there is nothing to pick.
    pub(super) fn focus(
        &mut self,
        document: DocumentId,
        found: &[EditorDiagnostic],
        caret: usize,
    ) -> bool {
        if found.is_empty() {
            return false;
        }
        if self.index(document, found).is_none() {
            let at = found
                .iter()
                .position(|d| d.range.start >= caret)
                .unwrap_or(found.len() - 1);
            self.pick = Some(Pick::of(document, &found[at]));
        }
        self.focused = true;
        true
    }

    pub(super) fn blur(&mut self) {
        self.focused = false;
    }

    /// Picks `index` of `found`.
    pub(super) fn select(
        &mut self,
        document: DocumentId,
        found: &[EditorDiagnostic],
        index: usize,
    ) {
        if let Some(found) = found.get(index) {
            self.pick = Some(Pick::of(document, found));
        }
    }

    /// Scrolls by `rows` without moving the pick.
    pub(super) fn scroll(&mut self, rows: isize, shown: usize, total: usize) {
        self.top = self
            .top
            .saturating_add_signed(rows)
            .min(total.saturating_sub(shown));
    }

    /// Index in `found` of the picked finding: the same one if it is still
    /// there, otherwise one that says the same nearest to where it was, and
    /// otherwise the one nearest to where it was.
    pub(super) fn index(&self, document: DocumentId, found: &[EditorDiagnostic]) -> Option<usize> {
        let pick = self
            .pick
            .as_ref()
            .filter(|pick| pick.document == document)?;
        let nearest = |same: bool| {
            found
                .iter()
                .enumerate()
                .filter(|(_, d)| !same || pick.says(d))
                .min_by_key(|(_, d)| d.range.start.abs_diff(pick.start))
                .map(|(index, _)| index)
        };
        found
            .iter()
            .position(|d| d.range.start == pick.start && pick.says(d))
            .or_else(|| nearest(true))
            .or_else(|| nearest(false))
    }

    /// Moves the pick `by` rows, stopping at the ends.
    pub(super) fn step(&mut self, document: DocumentId, found: &[EditorDiagnostic], by: isize) {
        let Some(at) = self.index(document, found) else {
            return;
        };
        let to = at.saturating_add_signed(by).min(found.len() - 1);
        self.pick = Some(Pick::of(document, &found[to]));
    }

    /// Follows the findings after they changed: the pick stays on the same
    /// finding or the nearest one, which stays in view, and the list
    /// returns the keys when nothing is left to pick.
    pub(super) fn sync(&mut self, document: DocumentId, found: &[EditorDiagnostic], shown: usize) {
        if self.pick.as_ref().is_some_and(|p| p.document != document) {
            *self = Self::default();
        }
        if found.is_empty() {
            self.focused = false;
            self.top = 0;
            return;
        }
        if let Some(at) = self.index(document, found) {
            self.pick = Some(Pick::of(document, &found[at]));
            if self.focused {
                if at < self.top {
                    self.top = at;
                } else if at >= self.top + shown {
                    self.top = at + 1 - shown;
                }
            }
        }
        self.top = self.top.min(found.len().saturating_sub(shown));
    }

    pub(super) fn top(&self) -> usize {
        self.top
    }

    /// Draws the list into the layout's problem rows.
    pub(super) fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        layout: &CodeLayout,
        document: &CodeDocument,
        found: &[EditorDiagnostic],
        hover: Option<usize>,
    ) {
        let area = layout.diag;
        fill_rect(
            framebuffer,
            area.x,
            area.y,
            area.width,
            area.height,
            BACKGROUND,
        );
        fill_rect(framebuffer, area.x, area.y, area.width, 1, BAR);
        let rows = layout.diag_rows();
        let cell_w = layout.metrics.cell_w;
        if found.is_empty() {
            let row = layout.diag_row(0);
            let text = ui_text::get("code-diag-none");
            draw_text(framebuffer, font, &text, row.x + PAD, row.y, DIM);
            return;
        }
        let picked = self
            .focused
            .then(|| self.index(document.id(), found))
            .flatten();
        let name = document.path().file_name();
        let name = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        let scrolls = found.len() > rows;
        let right = area.right() - if scrolls { 3 } else { 0 };
        for (n, (at, finding)) in found
            .iter()
            .enumerate()
            .skip(self.top)
            .take(rows)
            .enumerate()
        {
            let row = layout.diag_row(n);
            let lit = picked == Some(at);
            if lit {
                fill_rect(framebuffer, row.x, row.y, right - row.x, row.height, PALE);
            } else if hover == Some(at) {
                fill_rect(framebuffer, row.x, row.y, right - row.x, row.height, BAR);
            }
            let ink = if lit { BACKGROUND } else { TEXT };
            let mark = if lit {
                BACKGROUND
            } else {
                severity_color(finding.severity)
            };
            let y = row.y + (row.height - MARK) / 2;
            fill_rect(framebuffer, row.x + PAD, y, MARK, MARK, mark);
            let place = format!("{name}:{}", document.line_of(finding.range.start) + 1);
            let place_x = right - PAD - layout.cells(&place) * cell_w;
            draw_text(
                framebuffer,
                font,
                &place,
                place_x,
                row.y,
                if lit { BACKGROUND } else { DIM },
            );
            let message_x = row.x + PAD + MARK + PAD;
            let cells = usize::try_from((place_x - PAD - message_x) / cell_w.max(1)).unwrap_or(0);
            let first = finding.message.lines().next().unwrap_or_default();
            draw_text(
                framebuffer,
                font,
                &clip(font, first, cells),
                message_x,
                row.y,
                ink,
            );
        }
        if let Some((start, length)) = thumb(self.top, rows, found.len(), area.height - 1) {
            fill_rect(
                framebuffer,
                area.right() - 2,
                area.y + 1,
                2,
                area.height - 1,
                BAR,
            );
            fill_rect(
                framebuffer,
                area.right() - 2,
                area.y + 1 + start,
                2,
                length,
                PALE,
            );
        }
    }
}

impl CodeEditor {
    /// Keeps the problem list on the same finding after the findings or the
    /// active document changed.
    pub(super) fn sync_diag(&mut self, session: &CodeSession) {
        let Some(id) = session.active_document().map(CodeDocument::id) else {
            self.diag = DiagList::default();
            return;
        };
        let shown = self.layout.diag_rows();
        self.diag.sync(id, session.diagnostics(id), shown);
    }

    /// Applies `event` to the problem list. `None` when the list does not
    /// take it.
    pub(super) fn diag_input(
        &mut self,
        event: InputEvent,
        session: &mut CodeSession,
    ) -> Option<Option<CodeRequest>> {
        let id = session.active_document()?.id();
        let found = session.diagnostics(id).to_vec();
        let shown = self.layout.diag_rows();
        match event {
            InputEvent::FocusDiagnostics if self.diag.focused() => self.diag.blur(),
            InputEvent::FocusDiagnostics => {
                let caret = session
                    .tabs()
                    .selection(id)
                    .unwrap_or_default()
                    .caret()
                    .byte();
                if self.diag.focus(id, &found, caret) {
                    self.close_hover();
                }
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } if self.layout.diag.contains(at) => {
                if let Some(index) = self.diag_row_at(at, found.len()) {
                    self.diag.select(id, &found, index);
                    self.go_to_problem(session, found[index].range.start);
                }
            }
            // A press anywhere else takes the keys back and goes on to
            // whatever is under it.
            InputEvent::PointerDown { .. } => {
                self.diag.blur();
                return None;
            }
            InputEvent::Wheel { delta }
                if self.pointer.is_some_and(|at| self.layout.diag.contains(at)) =>
            {
                self.diag.scroll(-isize::from(delta), shown, found.len());
            }
            _ if !self.diag.focused() => return None,
            InputEvent::Up { .. } => self.diag.step(id, &found, -1),
            InputEvent::Down { .. } => self.diag.step(id, &found, 1),
            InputEvent::PageUp { .. } => self.diag.step(id, &found, -(shown as isize)),
            InputEvent::PageDown { .. } => self.diag.step(id, &found, shown as isize),
            InputEvent::Enter => {
                if let Some(index) = self.diag.index(id, &found) {
                    self.diag.blur();
                    self.go_to_problem(session, found[index].range.start);
                }
            }
            InputEvent::Break => self.diag.blur(),
            InputEvent::Tab { .. } => {
                if let Some(finding) = self.diag.index(id, &found).map(|at| &found[at]) {
                    let line = session.active_document()?.line_of(finding.range.start) + 1;
                    self.detail = Some(finding_detail(&self.layout, finding, line));
                }
            }
            // Keys that would edit or move the caret do nothing while the
            // list has the keys.
            InputEvent::Text(_)
            | InputEvent::Backspace
            | InputEvent::Delete
            | InputEvent::Left { .. }
            | InputEvent::Right { .. }
            | InputEvent::Home { .. }
            | InputEvent::End { .. }
            | InputEvent::WordLeft { .. }
            | InputEvent::WordRight { .. }
            | InputEvent::Undo
            | InputEvent::Redo
            | InputEvent::Cut
            | InputEvent::Paste => {}
            _ => return None,
        }
        Some(None)
    }

    /// The finding whose row `at` is over, among `total`.
    pub(super) fn diag_row_at(&self, at: Point, total: usize) -> Option<usize> {
        (0..self.layout.diag_rows())
            .find(|&n| self.layout.diag_row(n).contains(at))
            .map(|n| self.diag.top() + n)
            .filter(|&index| index < total)
    }
}

/// The window with the whole of `finding`, found on `line`.
fn finding_detail(layout: &CodeLayout, finding: &EditorDiagnostic, line: usize) -> Detail {
    let title = ui_text::get(match finding.severity {
        Severity::Error => "code-diag-error",
        Severity::Warning => "code-diag-warning",
        Severity::Info => "code-diag-info",
    });
    let place = ui_text::get_with("code-diag-line", &[("line", Arg::Text(&line.to_string()))]);
    let body = match &finding.code {
        Some(code) => format!("{code}\n{}", finding.message),
        None => finding.message.clone(),
    };
    Detail::new(&layout.font, layout.text, &title, &place, &body)
}
