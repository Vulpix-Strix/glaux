//! The lines of a document where the debugger should stop, and how they
//! follow the text as it is edited.

use std::collections::BTreeSet;

/// An edit seen as the lines it touches.
///
/// Lines are counted from 0 in the text before the edit. The edit replaces
/// text that starts on line `first` and ends on line `last`, with text that
/// holds `inserted` line breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LineEdit {
    pub(super) first: usize,
    pub(super) last: usize,
    /// Whether the edit starts at the first character of line `first`, so
    /// nothing of that line stays before it.
    pub(super) at_line_start: bool,
    pub(super) inserted: usize,
}

impl LineEdit {
    fn removed(&self) -> usize {
        self.last - self.first
    }
}

/// The breakpoints of one document, by line from 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Breakpoints {
    lines: BTreeSet<usize>,
}

impl Breakpoints {
    pub(super) fn from_lines(lines: impl IntoIterator<Item = usize>) -> Self {
        Self {
            lines: lines.into_iter().collect(),
        }
    }

    pub fn lines(&self) -> impl Iterator<Item = usize> + '_ {
        self.lines.iter().copied()
    }

    pub fn contains(&self, line: usize) -> bool {
        self.lines.contains(&line)
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Sets the breakpoint of `line`, or clears it if set. Returns whether
    /// it is set afterwards.
    pub(super) fn toggle(&mut self, line: usize) -> bool {
        if self.lines.remove(&line) {
            return false;
        }
        self.lines.insert(line);
        true
    }

    /// Forgets the breakpoints on lines `count` and after.
    pub(super) fn truncate(&mut self, count: usize) {
        self.lines.split_off(&count);
    }

    /// Moves the breakpoints to where their lines went.
    ///
    /// A line keeps its breakpoint when part of it stays before the edit,
    /// and a line whose end survives takes it along: the breakpoint of a
    /// line that is pushed down moves with it, and the one of a line that is
    /// deleted goes with it. A line joined into the one above loses it.
    pub(super) fn follow(&mut self, edit: LineEdit) {
        if edit.removed() == 0 && edit.inserted == 0 {
            return;
        }
        let moved = edit.first + edit.inserted;
        self.lines = self
            .lines
            .iter()
            .filter_map(|&line| match line {
                before if before < edit.first => Some(before),
                on_first if on_first == edit.first && edit.last > edit.first => {
                    // Its beginning is part of what was replaced unless
                    // something of it stays in front of the edit.
                    (!edit.at_line_start).then_some(on_first)
                }
                on_last if on_last == edit.last => {
                    if edit.at_line_start {
                        Some(moved)
                    } else if edit.last == edit.first {
                        Some(on_last)
                    } else {
                        None
                    }
                }
                inside if inside < edit.last => None,
                after => Some(after + edit.inserted - edit.removed()),
            })
            .collect();
    }
}

#[cfg(test)]
mod tests;
