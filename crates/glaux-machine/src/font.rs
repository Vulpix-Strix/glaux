//! Fixed-cell bitmap fonts covering any set of Unicode characters.
//!
//! Every glyph spans one or two character cells across and exactly one cell
//! down, and is drawn from the top-left corner of its first cell. Glyphs are
//! stored as one `u16` per row: only the low `width` bits of a row are used,
//! and bit `width - 1` is the leftmost column.
//!
//! A font maps characters to glyphs, may name a fallback glyph for the
//! characters it lacks, and may carry [`Lookup`]s that swap glyphs for others
//! of the same width, such as the parts of a programming ligature. Swaps never
//! change how many cells a character takes, so text laid out with
//! [`BitmapFont::cells`] keeps its geometry whichever [`Features`] draw it.

use std::sync::Arc;

mod substitution;

pub use substitution::{ContextRule, Features, GlyphSet, Ligature, Lookup};

/// Most cells a glyph spans.
pub const MAX_GLYPH_CELLS: u8 = 2;

/// A glyph of one [`BitmapFont`], valid only with the font that issued it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlyphId(u16);

impl GlyphId {
    /// Position of the glyph in the order glyphs were added to its font.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Errors raised while building a [`BitmapFont`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FontError {
    #[error(
        "invalid font metrics: {cell_width}x{cell_height} cells (cell width \
         must be 1..=8 so two cells fit in a 16-bit row, height at least 1)"
    )]
    InvalidMetrics { cell_width: u8, cell_height: u8 },
    #[error("a glyph must span 1..={MAX_GLYPH_CELLS} cells, not {0}")]
    InvalidCells(u8),
    #[error("glyph has {actual} rows, expected {expected}")]
    WrongRowCount { expected: usize, actual: usize },
    #[error("glyph row {row} ({bits:#018b}) is wider than the glyph's {width} pixels")]
    RowTooWide { row: usize, bits: u16, width: u8 },
    #[error("character {0:?} already has a glyph")]
    DuplicateChar(char),
    #[error("glyph {} does not belong to this font", .0.index())]
    UnknownGlyph(GlyphId),
    #[error("the font cannot hold more than {} glyphs", u16::MAX)]
    TooManyGlyphs,
    #[error(
        "glyph {} cannot replace glyph {}: they span a different number of cells",
        .to.index(),
        .from.index()
    )]
    WidthMismatch { from: GlyphId, to: GlyphId },
    #[error(
        "a ligature needs one part per input glyph and at least one of each, got {input} input glyphs and {parts} parts"
    )]
    LigatureShape { input: usize, parts: usize },
    #[error("a contextual rule needs at least one input glyph")]
    EmptyContext,
}

/// Cell dimensions of a bitmap font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontMetrics {
    cell_width: u8,
    cell_height: u8,
}

impl FontMetrics {
    /// Metrics of the system font: 6×12 cells.
    pub const SYSTEM: Self = Self {
        cell_width: 6,
        cell_height: 12,
    };

    /// Validates and creates font metrics.
    ///
    /// Cell width must be `1..=8`, so that a glyph two cells wide still fits
    /// one `u16` per row, and cell height must be at least 1.
    pub fn new(cell_width: u8, cell_height: u8) -> Result<Self, FontError> {
        if !(1..=8).contains(&cell_width) || cell_height == 0 {
            return Err(FontError::InvalidMetrics {
                cell_width,
                cell_height,
            });
        }
        Ok(Self {
            cell_width,
            cell_height,
        })
    }

    pub const fn cell_width(&self) -> u8 {
        self.cell_width
    }

    pub const fn cell_height(&self) -> u8 {
        self.cell_height
    }
}

/// A fixed-cell bitmap font with sparse Unicode coverage.
///
/// Cloning is cheap: clones share their glyphs until one of them is
/// changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitmapFont {
    data: Arc<FontData>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FontData {
    metrics: FontMetrics,
    glyphs: Vec<GlyphEntry>,
    /// Every glyph's rows, back to back; each glyph holds exactly
    /// `cell_height` rows that fit in its width.
    rows: Vec<u16>,
    ascii: [Option<GlyphId>; 128],
    /// Mapped characters above ASCII, sorted by character.
    others: Vec<(char, GlyphId)>,
    fallback: Option<GlyphId>,
    lookups: Vec<Lookup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GlyphEntry {
    cells: u8,
    first_row: usize,
}

impl BitmapFont {
    /// Creates a font with the given metrics and no glyphs.
    pub fn new(metrics: FontMetrics) -> Self {
        Self {
            data: Arc::new(FontData {
                metrics,
                glyphs: Vec::new(),
                rows: Vec::new(),
                ascii: [None; 128],
                others: Vec::new(),
                fallback: None,
                lookups: Vec::new(),
            }),
        }
    }

    pub fn metrics(&self) -> FontMetrics {
        self.data.metrics
    }

    /// Adds a glyph `cells` cells wide that no character maps to yet.
    ///
    /// `rows` must hold exactly `cell_height` rows, each using only the low
    /// `cells × cell_width` bits.
    pub fn add_glyph(&mut self, cells: u8, rows: &[u16]) -> Result<GlyphId, FontError> {
        if !(1..=MAX_GLYPH_CELLS).contains(&cells) {
            return Err(FontError::InvalidCells(cells));
        }
        let metrics = self.data.metrics;
        let expected = usize::from(metrics.cell_height);
        if rows.len() != expected {
            return Err(FontError::WrongRowCount {
                expected,
                actual: rows.len(),
            });
        }
        let width = cells * metrics.cell_width;
        if let Some((row, &bits)) = rows
            .iter()
            .enumerate()
            .find(|&(_, &bits)| u32::from(bits) >> width != 0)
        {
            return Err(FontError::RowTooWide { row, bits, width });
        }
        let id = u16::try_from(self.data.glyphs.len())
            .map(GlyphId)
            .map_err(|_| FontError::TooManyGlyphs)?;

        let data = Arc::make_mut(&mut self.data);
        data.glyphs.push(GlyphEntry {
            cells,
            first_row: data.rows.len(),
        });
        data.rows.extend_from_slice(rows);
        Ok(id)
    }

    /// Maps `ch` to `glyph`. Each character maps to at most one glyph, but a
    /// glyph may serve several characters.
    pub fn map(&mut self, ch: char, glyph: GlyphId) -> Result<(), FontError> {
        self.check(glyph)?;
        if self.glyph_id(ch).is_some() {
            return Err(FontError::DuplicateChar(ch));
        }
        let data = Arc::make_mut(&mut self.data);
        match data.ascii.get_mut(ch as usize) {
            Some(slot) => *slot = Some(glyph),
            None => {
                let at = data.others.partition_point(|&(c, _)| c < ch);
                data.others.insert(at, (ch, glyph));
            }
        }
        Ok(())
    }

    /// Adds a glyph like [`add_glyph`](Self::add_glyph) and maps `ch` to it.
    /// Nothing is added when `ch` already has a glyph.
    pub fn insert(&mut self, ch: char, cells: u8, rows: &[u16]) -> Result<GlyphId, FontError> {
        if self.glyph_id(ch).is_some() {
            return Err(FontError::DuplicateChar(ch));
        }
        let glyph = self.add_glyph(cells, rows)?;
        self.map(ch, glyph)?;
        Ok(glyph)
    }

    /// Draws characters the font has no glyph for with `glyph`.
    pub fn set_fallback(&mut self, glyph: GlyphId) -> Result<(), FontError> {
        self.check(glyph)?;
        Arc::make_mut(&mut self.data).fallback = Some(glyph);
        Ok(())
    }

    /// Appends `lookup` to the substitutions [`shape`](Self::shape) applies,
    /// which run in the order they were added.
    ///
    /// Every glyph it names must belong to the font, and every replacement
    /// must span as many cells as the glyph it replaces.
    pub fn add_lookup(&mut self, lookup: Lookup) -> Result<(), FontError> {
        let lookup = lookup.validated(|id| self.glyph(id).map(|glyph| glyph.cells()))?;
        Arc::make_mut(&mut self.data).lookups.push(lookup);
        Ok(())
    }

    /// Number of glyphs, mapped or not.
    pub fn glyph_count(&self) -> usize {
        self.data.glyphs.len()
    }

    pub fn glyph(&self, id: GlyphId) -> Option<Glyph<'_>> {
        let entry = *self.data.glyphs.get(id.index())?;
        let height = usize::from(self.data.metrics.cell_height);
        Some(Glyph {
            rows: &self.data.rows[entry.first_row..entry.first_row + height],
            cells: entry.cells,
            width: entry.cells * self.data.metrics.cell_width,
        })
    }

    /// The glyph `ch` maps to, if any; the fallback is not considered.
    pub fn glyph_id(&self, ch: char) -> Option<GlyphId> {
        match self.data.ascii.get(ch as usize) {
            Some(&slot) => slot,
            None => self
                .data
                .others
                .binary_search_by_key(&ch, |&(c, _)| c)
                .ok()
                .map(|at| self.data.others[at].1),
        }
    }

    /// The glyph drawn for characters without one of their own.
    pub fn fallback(&self) -> Option<GlyphId> {
        self.data.fallback
    }

    /// The glyph drawn for `ch`: its own, or else the fallback. Control
    /// characters draw nothing.
    pub fn resolve(&self, ch: char) -> Option<GlyphId> {
        if ch.is_control() {
            return None;
        }
        self.glyph_id(ch).or(self.data.fallback)
    }

    /// Cells `ch` takes: those of the glyph [`resolve`](Self::resolve) gives,
    /// or one when it draws nothing.
    pub fn cells(&self, ch: char) -> usize {
        self.resolve(ch)
            .and_then(|id| self.glyph(id))
            .map_or(1, |glyph| usize::from(glyph.cells()))
    }

    /// Cells `text` takes on one line.
    pub fn text_cells(&self, text: &str) -> usize {
        text.chars().map(|ch| self.cells(ch)).sum()
    }

    /// Every character with a glyph of its own, in ascending order.
    pub fn chars(&self) -> impl Iterator<Item = char> + '_ {
        let ascii = self
            .data
            .ascii
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.is_some())
            .map(|(code, _)| char::from(code as u8));
        ascii.chain(self.data.others.iter().map(|&(ch, _)| ch))
    }

    /// The glyph each character of `text` draws with once the lookups of
    /// `features` have run, one entry per character; `None` draws nothing.
    pub fn shape(&self, text: &str, features: Features) -> Vec<Option<GlyphId>> {
        let mut glyphs: Vec<_> = text.chars().map(|ch| self.resolve(ch)).collect();
        for lookup in &self.data.lookups {
            lookup.apply(&mut glyphs, features);
        }
        glyphs
    }

    fn check(&self, glyph: GlyphId) -> Result<(), FontError> {
        match self.glyph(glyph) {
            Some(_) => Ok(()),
            None => Err(FontError::UnknownGlyph(glyph)),
        }
    }
}

/// A borrowed view of one glyph's rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph<'a> {
    rows: &'a [u16],
    cells: u8,
    width: u8,
}

impl<'a> Glyph<'a> {
    /// Rows from top to bottom; bit `width - 1` is the leftmost column.
    pub fn rows(&self) -> &'a [u16] {
        self.rows
    }

    /// Cells the glyph spans.
    pub fn cells(&self) -> u8 {
        self.cells
    }

    /// Width in pixels.
    pub fn width(&self) -> u8 {
        self.width
    }

    /// Returns whether the pixel at `(x, y)` is set. Positions outside the
    /// glyph are never set.
    pub fn is_set(&self, x: usize, y: usize) -> bool {
        let width = usize::from(self.width);
        if x >= width {
            return false;
        }
        self.rows
            .get(y)
            .is_some_and(|&row| row >> (width - 1 - x) & 1 == 1)
    }
}

#[cfg(test)]
mod tests;
