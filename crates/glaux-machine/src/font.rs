//! Bitmap font covering printable ASCII.
//!
//! Glyphs are stored as one byte per row. Only the low `glyph_width` bits of
//! a row are used, and bit `glyph_width - 1` is the leftmost column. A glyph
//! occupies the top-left of its character cell; the rest of the cell is
//! spacing.

/// Glyph width of the system font, in pixels.
pub const GLYPH_WIDTH: u8 = 5;
/// Glyph height of the system font, in pixels.
pub const GLYPH_HEIGHT: u8 = 9;
/// Character cell width of the system font: the glyph plus one spacing column.
pub const CELL_WIDTH: u8 = 6;
/// Character cell height of the system font: the glyph plus one spacing row.
pub const CELL_HEIGHT: u8 = 10;

/// First character a [`BitmapFont`] can hold (start of printable ASCII).
pub const FIRST_CHAR: char = ' ';
/// Last character a [`BitmapFont`] can hold (end of printable ASCII).
pub const LAST_CHAR: char = '~';

const GLYPH_SLOTS: usize = LAST_CHAR as usize - FIRST_CHAR as usize + 1;

/// Errors raised while building a [`BitmapFont`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FontError {
    #[error(
        "invalid font metrics: glyph {glyph_width}x{glyph_height} in cell \
         {cell_width}x{cell_height} (glyph width must be 1..=8, height at \
         least 1, and the glyph must fit in the cell)"
    )]
    InvalidMetrics {
        glyph_width: u8,
        glyph_height: u8,
        cell_width: u8,
        cell_height: u8,
    },
    #[error("character {0:?} is outside the printable ASCII range")]
    UnsupportedChar(char),
    #[error("glyph {ch:?} has {actual} rows, expected {expected}")]
    WrongRowCount {
        ch: char,
        expected: usize,
        actual: usize,
    },
    #[error("glyph {ch:?} row {row} ({bits:#010b}) is wider than the glyph width")]
    RowTooWide { ch: char, row: usize, bits: u8 },
}

/// Glyph and cell dimensions of a bitmap font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontMetrics {
    glyph_width: u8,
    glyph_height: u8,
    cell_width: u8,
    cell_height: u8,
}

impl FontMetrics {
    /// Metrics of the system font: 5×9 glyphs in 6×10 cells.
    pub const SYSTEM: Self = Self {
        glyph_width: GLYPH_WIDTH,
        glyph_height: GLYPH_HEIGHT,
        cell_width: CELL_WIDTH,
        cell_height: CELL_HEIGHT,
    };

    /// Validates and creates font metrics.
    ///
    /// Glyph width must be `1..=8` because each row is stored in one byte,
    /// glyph height must be at least 1, and the glyph must fit in the cell.
    pub fn new(
        glyph_width: u8,
        glyph_height: u8,
        cell_width: u8,
        cell_height: u8,
    ) -> Result<Self, FontError> {
        let valid = (1..=8).contains(&glyph_width)
            && glyph_height >= 1
            && cell_width >= glyph_width
            && cell_height >= glyph_height;
        if !valid {
            return Err(FontError::InvalidMetrics {
                glyph_width,
                glyph_height,
                cell_width,
                cell_height,
            });
        }
        Ok(Self {
            glyph_width,
            glyph_height,
            cell_width,
            cell_height,
        })
    }

    pub const fn glyph_width(&self) -> u8 {
        self.glyph_width
    }

    pub const fn glyph_height(&self) -> u8 {
        self.glyph_height
    }

    pub const fn cell_width(&self) -> u8 {
        self.cell_width
    }

    pub const fn cell_height(&self) -> u8 {
        self.cell_height
    }
}

/// A fixed-cell bitmap font for printable ASCII (`' '..='~'`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitmapFont {
    metrics: FontMetrics,
    // One slot per character in `CHAR_RANGE`; each glyph holds exactly
    // `glyph_height` rows that fit in `glyph_width` bits.
    glyphs: Vec<Option<Box<[u8]>>>,
}

impl BitmapFont {
    /// Creates a font with the given metrics and no glyphs.
    pub fn new(metrics: FontMetrics) -> Self {
        Self {
            metrics,
            glyphs: vec![None; GLYPH_SLOTS],
        }
    }

    pub fn metrics(&self) -> FontMetrics {
        self.metrics
    }

    /// Sets the glyph for `ch`, replacing any previous one.
    ///
    /// `rows` must hold exactly `glyph_height` rows, each using only the low
    /// `glyph_width` bits.
    pub fn insert(&mut self, ch: char, rows: &[u8]) -> Result<(), FontError> {
        let slot = slot(ch).ok_or(FontError::UnsupportedChar(ch))?;

        let expected = usize::from(self.metrics.glyph_height);
        if rows.len() != expected {
            return Err(FontError::WrongRowCount {
                ch,
                expected,
                actual: rows.len(),
            });
        }
        let width = u32::from(self.metrics.glyph_width);
        if let Some((row, &bits)) = rows
            .iter()
            .enumerate()
            .find(|&(_, &bits)| u32::from(bits) >> width != 0)
        {
            return Err(FontError::RowTooWide { ch, row, bits });
        }

        self.glyphs[slot] = Some(rows.into());
        Ok(())
    }

    /// Returns the glyph for `ch`, or `None` if `ch` is outside printable
    /// ASCII or the font has no glyph for it.
    pub fn glyph(&self, ch: char) -> Option<Glyph<'_>> {
        let rows = self.glyphs[slot(ch)?].as_deref()?;
        Some(Glyph {
            rows,
            width: self.metrics.glyph_width,
        })
    }
}

/// A borrowed view of one glyph's rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph<'a> {
    rows: &'a [u8],
    width: u8,
}

impl<'a> Glyph<'a> {
    /// Rows from top to bottom; bit `glyph_width - 1` is the leftmost column.
    pub fn rows(&self) -> &'a [u8] {
        self.rows
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

fn slot(ch: char) -> Option<usize> {
    (FIRST_CHAR..=LAST_CHAR)
        .contains(&ch)
        .then(|| ch as usize - FIRST_CHAR as usize)
}

#[cfg(test)]
mod tests;
