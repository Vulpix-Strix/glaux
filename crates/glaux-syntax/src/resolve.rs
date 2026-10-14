use std::cmp::Reverse;
use std::ops::Range;

use crate::kind::SyntaxKind;

/// A styled byte range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub kind: SyntaxKind,
}

/// One capture produced by a query, before overlaps are settled.
#[derive(Debug, Clone)]
pub(crate) struct Capture {
    pub range: Range<usize>,
    pub kind: SyntaxKind,
    pub pattern: usize,
    pub index: usize,
}

/// Flattens overlapping captures into sorted, non-overlapping spans inside `window`.
///
/// Where captures overlap, the winner for each byte is the one on the shortest node
/// (the most specific), then the one from the later query pattern, then the one with
/// the higher capture index. The node length is measured before clipping so the
/// result for a byte does not depend on the window asked for.
pub(crate) fn resolve(captures: &[Capture], window: &Range<usize>) -> Vec<Span> {
    let mut live: Vec<&Capture> = captures
        .iter()
        .filter(|c| c.range.start < window.end && c.range.end > window.start)
        .filter(|c| c.range.start < c.range.end)
        .collect();
    live.sort_by_key(|c| c.range.start);

    let mut bounds: Vec<usize> = live
        .iter()
        .flat_map(|c| [c.range.start, c.range.end])
        .chain([window.start, window.end])
        .filter(|b| window.contains(b) || *b == window.end)
        .collect();
    bounds.sort_unstable();
    bounds.dedup();

    let mut spans: Vec<Span> = Vec::new();
    let mut active: Vec<&Capture> = Vec::new();
    let mut pending = live.into_iter().peekable();
    for pair in bounds.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        while let Some(next) = pending.next_if(|c| c.range.start <= start) {
            active.push(next);
        }
        active.retain(|c| c.range.end > start);

        let Some(winner) = active
            .iter()
            .max_by_key(|c| (Reverse(c.range.len()), c.pattern, c.index))
        else {
            continue;
        };
        match spans.last_mut() {
            Some(last) if last.range.end == start && last.kind == winner.kind => {
                last.range.end = end;
            }
            _ => spans.push(Span {
                range: start..end,
                kind: winner.kind,
            }),
        }
    }
    spans
}

#[cfg(test)]
mod tests;
