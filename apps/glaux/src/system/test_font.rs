//! A font for tests of how text takes cells, where only the width of each
//! glyph matters.

use std::sync::LazyLock;

use glaux_machine::font::{BitmapFont, FontMetrics};

use super::editing::metrics::Columns;

/// Characters [`LAYOUT_FONT`] draws two cells wide.
pub(crate) const WIDE: [char; 2] = ['✅', '⭕'];

/// Blank glyphs in system-sized cells: one cell wide for every character,
/// through its fallback, but two for those in [`WIDE`].
pub(crate) static LAYOUT_FONT: LazyLock<BitmapFont> = LazyLock::new(|| {
    let metrics = FontMetrics::SYSTEM;
    let rows = vec![0; usize::from(metrics.cell_height())];
    let mut font = BitmapFont::new(metrics);
    for ch in WIDE {
        font.insert(ch, 2, &rows).unwrap();
    }
    let blank = font.add_glyph(1, &rows).unwrap();
    font.set_fallback(blank).unwrap();
    font
});

/// Columns in [`LAYOUT_FONT`] with tab stops every `tab_size` columns.
pub(crate) fn columns(tab_size: usize) -> Columns<'static> {
    Columns {
        font: &LAYOUT_FONT,
        tab_size,
    }
}
