//! How the Code view shows text, chosen by the user and kept across
//! launches independently of any cartridge.

use std::ops::RangeInclusive;

use crate::ui_text;
use glaux_machine::font::Features;
use serde::{Deserialize, Serialize};

/// Tab sizes the Code view accepts, in columns.
pub const TAB_SIZES: RangeInclusive<u8> = 1..=8;
/// Indentation sizes the Code view accepts, in columns.
pub const INDENT_SIZES: RangeInclusive<u8> = 1..=8;

/// What indentation the Code view inserts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndentStyle {
    /// Whatever each file already uses: tabs or spaces, and for spaces the
    /// width of its steps where it shows one.
    #[default]
    Detect,
    Spaces,
    Tabs,
}

impl IndentStyle {
    pub const ALL: [Self; 3] = [Self::Detect, Self::Spaces, Self::Tabs];

    /// The style `step` places away in the order Detect, Spaces, Tabs,
    /// wrapping around.
    pub fn stepped(self, step: i8) -> Self {
        let len = Self::ALL.len() as isize;
        let at = Self::ALL
            .iter()
            .position(|&style| style == self)
            .unwrap_or(0) as isize;
        Self::ALL[(at + isize::from(step)).rem_euclid(len) as usize]
    }

    pub fn label(self) -> String {
        match self {
            Self::Detect => ui_text::get("system-detect"),
            Self::Spaces => ui_text::get("system-spaces"),
            Self::Tabs => ui_text::get("system-tabs"),
        }
    }
}

/// The Code view's preferences.
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
    /// Draw the font's programming ligatures and contextual alternates.
    /// They only change how characters look: every character keeps its
    /// own cells, so the caret and the document are unaffected.
    pub ligatures: bool,
    pub indent: IndentStyle,
    /// Columns one indentation step takes with spaces, where the file
    /// doesn't show its own. Within [`INDENT_SIZES`] unless read from a
    /// file that says otherwise.
    pub indent_size: u8,
    /// Write every modified document to the cartridge before it runs from
    /// the Code view. When off, a run uses what the cartridge's files hold.
    pub save_before_running: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            word_wrap: false,
            tab_size: 4,
            line_numbers: true,
            ligatures: true,
            indent: IndentStyle::Detect,
            indent_size: 4,
            save_before_running: true,
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

    /// The indentation size `step` sizes away, kept within
    /// [`INDENT_SIZES`].
    pub fn step_indent_size(&mut self, step: i8) {
        self.indent_size = self
            .indent_size
            .saturating_add_signed(step)
            .clamp(*INDENT_SIZES.start(), *INDENT_SIZES.end());
    }

    /// [`indent_size`](Self::indent_size) as a column count, never zero.
    pub fn indent_columns(&self) -> usize {
        usize::from(self.indent_size.max(1))
    }

    /// The font substitutions the text is drawn with.
    pub fn features(&self) -> Features {
        if self.ligatures {
            Features::LIGATURES | Features::CONTEXTUAL
        } else {
            Features::empty()
        }
    }

    /// [`tab_size`](Self::tab_size) as a column count, never zero.
    pub fn tab_columns(&self) -> usize {
        usize::from(self.tab_size.max(1))
    }
}

#[cfg(test)]
mod tests;
