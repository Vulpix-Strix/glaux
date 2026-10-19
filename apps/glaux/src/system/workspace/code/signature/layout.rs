//! Where the signature popup sits and how its text breaks into lines.

use std::ops::Range;

use glaux_machine::font::BitmapFont;
use glaux_syntax::SyntaxKind;

use super::view::SignatureView;
use crate::system::workspace::code::popup::{Place, SHADOW};
use crate::system::workspace::{Rect, wrap};

/// Space around the popup's text.
pub const PAD_X: i32 = 4;
const PAD_Y: i32 = 1;
/// Lines of the note the popup shows.
const NOTE_LINES: usize = 3;
/// Fewest columns the note keeps beside its head. With fewer it moves under
/// the head.
const MIN_NOTE_COLUMNS: usize = 12;
/// The text left in place of the parameters a long signature leaves out.
const ELIDED: &str = "..";

/// A piece of a line and the syntax category that colors it. A chip piece
/// is set off from the rest: it is the parameter being written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub kind: SyntaxKind,
    pub chip: bool,
}

/// A line of the note and where it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteLine {
    pub x: i32,
    pub y: i32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureLayout {
    /// The border included.
    pub window: Rect,
    /// The text columns each line may take.
    pub columns: usize,
    /// Top of the signature line, which starts at `text_x`.
    pub line_y: i32,
    pub text_x: i32,
    pub segments: Vec<Segment>,
    /// The line between the signature and what is said about the parameter.
    pub rule: Option<Rect>,
    /// Top of the head line, which starts at `text_x`.
    pub head_y: i32,
    pub head: Vec<Segment>,
    pub notes: Vec<NoteLine>,
}

impl SignatureLayout {
    /// Lays out `view` over the line whose top-left is `anchor`: the popup
    /// goes above it, or below when there is no room above. `avoid` lists
    /// what must stay uncovered. When the popup would cover any of it, there
    /// is no layout.
    pub fn new(
        place: Place<'_>,
        view: &SignatureView,
        anchor: (i32, i32),
        avoid: &[Rect],
    ) -> Option<Self> {
        let Place {
            font,
            area,
            cell_w,
            cell_h,
        } = place;
        let chrome = 2 + 2 * PAD_X;
        let columns = usize::try_from((area.width - SHADOW - chrome) / cell_w.max(1)).ok()?;
        if columns == 0 {
            return None;
        }
        let segments = fit(view, font, columns);
        let head = head_segments(view);
        let head_cells = cells(font, &head);
        // The note sits beside the head, as a table row does, unless that
        // leaves it too narrow.
        let beside = !head.is_empty() && columns.saturating_sub(head_cells + 1) >= MIN_NOTE_COLUMNS;
        let indent = if beside { head_cells + 1 } else { 0 };
        let notes: Vec<String> = view
            .note
            .as_deref()
            .map(|note| wrap(font, note, columns - indent))
            .unwrap_or_default()
            .into_iter()
            .take(NOTE_LINES)
            .collect();

        let detail_rows = if beside {
            notes.len().max(1)
        } else {
            usize::from(!head.is_empty()) + notes.len()
        };
        let widest = notes
            .iter()
            .map(|line| font.text_cells(line) + indent)
            .chain([cells(font, &segments).min(columns), head_cells])
            .max()
            .unwrap_or(0);
        let width = usize_to_i32(widest) * cell_w + chrome;
        let detail_height = if detail_rows == 0 {
            0
        } else {
            1 + PAD_Y + usize_to_i32(detail_rows) * cell_h + PAD_Y
        };
        let height = 2 + PAD_Y + cell_h + PAD_Y + detail_height;

        let x = anchor
            .0
            .clamp(area.x, (area.right() - width - SHADOW).max(area.x));
        let above = anchor.1 - height - SHADOW;
        let below = anchor.1 + cell_h;
        let y = if above >= area.y {
            above
        } else if below + height + SHADOW <= area.bottom() {
            below
        } else {
            (area.bottom() - height - SHADOW).max(area.y)
        };
        let window = Rect {
            x,
            y,
            width,
            height,
        };
        let shadowed = Rect {
            width: width + SHADOW,
            height: height + SHADOW,
            ..window
        };
        if avoid.iter().any(|other| overlap(shadowed, *other)) {
            return None;
        }

        let text_x = window.x + 1 + PAD_X;
        let line_y = window.y + 1 + PAD_Y;
        let rule_y = line_y + cell_h + PAD_Y;
        let rule = (detail_rows > 0).then_some(Rect {
            x: window.x + 1,
            y: rule_y,
            width: window.width - 2,
            height: 1,
        });
        let head_y = rule_y + 1 + PAD_Y;
        let first_note_y = if beside || head.is_empty() {
            head_y
        } else {
            head_y + cell_h
        };
        let note_x = text_x + usize_to_i32(indent) * cell_w;
        let notes = notes
            .into_iter()
            .enumerate()
            .map(|(row, text)| NoteLine {
                x: note_x,
                y: first_note_y + usize_to_i32(row) * cell_h,
                text,
            })
            .collect();
        Some(Self {
            window,
            columns,
            line_y,
            text_x,
            segments,
            rule,
            head_y,
            head,
            notes,
        })
    }
}

fn usize_to_i32(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn cells(font: &BitmapFont, segments: &[Segment]) -> usize {
    segments
        .iter()
        .map(|segment| font.text_cells(&segment.text))
        .sum()
}

fn overlap(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// The name and type of the parameter being written.
fn head_segments(view: &SignatureView) -> Vec<Segment> {
    let Some(head) = &view.head else {
        return Vec::new();
    };
    let piece = |text: &str, kind| Segment {
        text: text.to_owned(),
        kind,
        chip: false,
    };
    let mut segments = vec![piece(&head.name, SyntaxKind::Variable)];
    if let Some(ty) = head.ty {
        segments.push(piece(" ", SyntaxKind::Operator));
        segments.push(piece(ty, SyntaxKind::Type));
    }
    segments
}

/// The signature line within `columns`. When the whole signature does not
/// fit, the return type goes first and then the parameters far from the
/// one being written, which `..` stands for.
fn fit(view: &SignatureView, font: &BitmapFont, columns: usize) -> Vec<Segment> {
    let count = view.params.len();
    for returns in [true, false] {
        let all = signature_line(view, 0..count, returns);
        if cells(font, &all) <= columns {
            return all;
        }
    }
    let center = view.active.unwrap_or(0).min(count.saturating_sub(1));
    let mut range = center..(center + 1).min(count);
    let mut best = signature_line(view, range.clone(), false);
    // Take in the parameter after the range and then the one before it for as
    // long as the line still fits.
    loop {
        let after = (range.end < count).then(|| range.start..range.end + 1);
        let before = (range.start > 0).then(|| range.start - 1..range.end);
        let mut grew = false;
        for next in [after, before].into_iter().flatten() {
            let line = signature_line(view, next.clone(), false);
            if cells(font, &line) <= columns {
                range = next;
                best = line;
                grew = true;
                break;
            }
        }
        if !grew {
            return best;
        }
    }
}

/// The signature line showing only the parameters in `shown`.
fn signature_line(view: &SignatureView, shown: Range<usize>, returns: bool) -> Vec<Segment> {
    let piece = |text: &str, kind| Segment {
        text: text.to_owned(),
        kind,
        chip: false,
    };
    let operator = |text: &str| piece(text, SyntaxKind::Operator);
    let mut line = vec![piece(&view.name, SyntaxKind::Function), operator("(")];
    if shown.start > 0 {
        line.push(operator(ELIDED));
        if !shown.is_empty() {
            line.push(operator(", "));
        }
    }
    for index in shown.clone() {
        if index > shown.start {
            line.push(operator(", "));
        }
        line.push(Segment {
            chip: view.active == Some(index),
            ..piece(&view.params[index], SyntaxKind::Variable)
        });
    }
    if shown.end < view.params.len() {
        if !shown.is_empty() {
            line.push(operator(", "));
        }
        line.push(operator(ELIDED));
    }
    line.push(operator(")"));
    if returns && let Some(ty) = view.returns {
        line.push(operator(" : "));
        line.push(piece(ty, SyntaxKind::Type));
    }
    if let Some((at, of)) = view.overload {
        line.push(piece(&format!(" {at}/{of}"), SyntaxKind::Comment));
    }
    line
}

#[cfg(test)]
mod tests;
