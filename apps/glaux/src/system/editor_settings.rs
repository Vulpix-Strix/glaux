//! How the Code view shows text, chosen by the user and kept across
//! launches independently of any cartridge.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

/// Tab sizes the Code view accepts, in columns.
pub const TAB_SIZES: RangeInclusive<u8> = 1..=8;

/// The Code view's display preferences.
///
/// Missing fields take their default when read, and unknown ones are
/// ignored, so files written by other versions still load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSettings {
    /// Fold lines wider than the view onto the rows below instead of
    /// scrolling sideways.
    pub word_wrap: bool,
    /// Columns a tab advances to the next multiple of. Within
    /// [`TAB_SIZES`] unless read from a file that says otherwise.
    pub tab_size: u8,
    pub line_numbers: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            word_wrap: false,
            tab_size: 4,
            line_numbers: true,
        }
    }
}

impl EditorSettings {
    /// The tab size `step` sizes away, kept within [`TAB_SIZES`].
    pub fn step_tab_size(&mut self, step: i8) {
        self.tab_size = self
            .tab_size
            .saturating_add_signed(step)
            .clamp(*TAB_SIZES.start(), *TAB_SIZES.end());
    }

    /// [`tab_size`](Self::tab_size) as a column count, never zero.
    pub fn tab_columns(&self) -> usize {
        usize::from(self.tab_size.max(1))
    }
}

#[cfg(test)]
mod tests;
