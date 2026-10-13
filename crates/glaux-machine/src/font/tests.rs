use super::*;
use pretty_assertions::assert_eq;

/// 3×4 cells, so a one-cell glyph has 3-bit rows and a two-cell one 6-bit
/// rows.
fn small() -> FontMetrics {
    FontMetrics::new(3, 4).unwrap()
}

const A: [u16; 4] = [0b010, 0b101, 0b111, 0b101];
const WIDE: [u16; 4] = [0b100_001, 0b010_010, 0b001_100, 0b111_111];
const BOX: [u16; 4] = [0b111, 0b101, 0b101, 0b111];

/// `A` and `é` one cell wide, `✅` and `𝔸` two cells wide, and a hollow
/// box as fallback.
fn font() -> BitmapFont {
    let mut font = BitmapFont::new(small());
    font.insert('A', 1, &A).unwrap();
    font.insert('é', 1, &A).unwrap();
    font.insert('✅', 2, &WIDE).unwrap();
    font.insert('𝔸', 2, &WIDE).unwrap();
    let fallback = font.add_glyph(1, &BOX).unwrap();
    font.set_fallback(fallback).unwrap();
    font
}

#[test]
fn system_metrics_are_six_by_twelve_cells() {
    let m = FontMetrics::SYSTEM;
    assert_eq!((m.cell_width(), m.cell_height()), (6, 12));
    assert_eq!(FontMetrics::new(6, 12), Ok(FontMetrics::SYSTEM));
}

#[test]
fn rejects_metrics_whose_two_cell_rows_overflow_or_that_are_empty() {
    for (w, h) in [(0, 12), (9, 12), (6, 0)] {
        assert_eq!(
            FontMetrics::new(w, h),
            Err(FontError::InvalidMetrics {
                cell_width: w,
                cell_height: h
            }),
            "{w}x{h}"
        );
    }
    assert!(
        FontMetrics::new(8, 1).is_ok(),
        "two 8-pixel cells fill a u16"
    );
}

#[test]
fn looks_up_glyphs_across_planes() {
    let font = font();
    for (ch, rows, cells) in [
        ('A', &A, 1),
        ('é', &A, 1),
        ('✅', &WIDE, 2),
        ('𝔸', &WIDE, 2),
    ] {
        let glyph = font.glyph(font.glyph_id(ch).unwrap()).unwrap();
        assert_eq!((glyph.rows(), glyph.cells()), (&rows[..], cells), "{ch:?}");
        assert_eq!(glyph.width(), cells * 3, "{ch:?}");
    }
}

#[test]
fn lists_mapped_characters_in_ascending_order() {
    let chars: Vec<char> = font().chars().collect();
    assert_eq!(chars, ['A', 'é', '✅', '𝔸']);
}

#[test]
fn characters_without_a_glyph_resolve_to_the_fallback() {
    let font = font();
    let fallback = font.fallback().unwrap();
    for ch in ['B', ' ', '中', '\u{1FBFF}'] {
        assert_eq!(font.glyph_id(ch), None, "{ch:?} has no glyph of its own");
        assert_eq!(font.resolve(ch), Some(fallback), "{ch:?}");
    }
    assert_eq!(font.resolve('✅'), font.glyph_id('✅'));
}

#[test]
fn control_characters_draw_nothing_even_with_a_fallback() {
    let font = font();
    for ch in ['\0', '\t', '\n', '\r', '\u{7F}', '\u{85}'] {
        assert_eq!(font.resolve(ch), None, "{ch:?}");
    }
}

#[test]
fn without_a_fallback_missing_characters_draw_nothing() {
    let mut font = BitmapFont::new(small());
    font.insert('A', 1, &A).unwrap();
    assert_eq!(font.resolve('B'), None);
    assert_eq!(font.cells('B'), 1);
}

#[test]
fn cells_follow_the_glyph_drawn() {
    let font = font();
    let cases = [
        ('A', 1),
        ('✅', 2),
        ('𝔸', 2),
        ('中', 1),
        ('\t', 1),
        ('\0', 1),
    ];
    for (ch, cells) in cases {
        assert_eq!(font.cells(ch), cells, "{ch:?}");
    }
    assert_eq!(font.text_cells("A✅B𝔸"), 1 + 2 + 1 + 2);
    assert_eq!(font.text_cells(""), 0);
}

#[test]
fn a_glyph_may_serve_several_characters() {
    let mut font = font();
    let a = font.glyph_id('A').unwrap();
    font.map('Á', a).unwrap();
    assert_eq!(font.glyph_id('Á'), Some(a));
}

#[test]
fn rejects_a_second_glyph_for_a_character_without_adding_it() {
    let mut font = font();
    let count = font.glyph_count();
    assert_eq!(
        font.insert('é', 1, &BOX),
        Err(FontError::DuplicateChar('é'))
    );
    assert_eq!(
        font.insert('𝔸', 1, &BOX),
        Err(FontError::DuplicateChar('𝔸'))
    );
    assert_eq!(font.glyph_count(), count);
    assert_eq!(font.glyph(font.glyph_id('é').unwrap()).unwrap().rows(), &A);
}

#[test]
fn rejects_glyphs_of_another_font() {
    let stranger = font().glyph_id('𝔸').unwrap();
    let mut empty = BitmapFont::new(small());
    assert_eq!(
        empty.map('x', stranger),
        Err(FontError::UnknownGlyph(stranger))
    );
    assert_eq!(
        empty.set_fallback(stranger),
        Err(FontError::UnknownGlyph(stranger))
    );
    assert_eq!(empty.glyph_id('x'), None);
    assert_eq!(empty.fallback(), None);
}

#[test]
fn rejects_glyphs_spanning_no_cells_or_more_than_two() {
    let mut font = BitmapFont::new(small());
    for cells in [0, 3] {
        assert_eq!(
            font.add_glyph(cells, &A),
            Err(FontError::InvalidCells(cells))
        );
    }
    assert_eq!(font.glyph_count(), 0);
}

#[test]
fn rejects_wrong_row_counts() {
    let mut font = BitmapFont::new(small());
    assert_eq!(
        font.add_glyph(1, &A[..3]),
        Err(FontError::WrongRowCount {
            expected: 4,
            actual: 3
        })
    );
    assert_eq!(font.glyph_count(), 0);
}

#[test]
fn rows_may_use_every_column_of_their_cells_and_no_more() {
    let mut font = BitmapFont::new(small());
    assert_eq!(
        font.add_glyph(1, &[0b111, 0, 0b1000, 0]),
        Err(FontError::RowTooWide {
            row: 2,
            bits: 0b1000,
            width: 3
        })
    );
    assert_eq!(
        font.add_glyph(2, &[0b100_0000, 0, 0, 0]),
        Err(FontError::RowTooWide {
            row: 0,
            bits: 0b100_0000,
            width: 6
        })
    );
    assert!(font.add_glyph(2, &[0b11_1111; 4]).is_ok());
}

#[test]
fn is_set_reads_bits_left_to_right_across_both_cells() {
    let font = font();
    let wide = font.glyph(font.glyph_id('✅').unwrap()).unwrap();
    // Row 0 is 0b100_001: the first column of the left cell and the last of
    // the right one.
    let row: Vec<bool> = (0..6).map(|x| wide.is_set(x, 0)).collect();
    assert_eq!(row, [true, false, false, false, false, true]);
    let row: Vec<bool> = (0..6).map(|x| wide.is_set(x, 2)).collect();
    assert_eq!(row, [false, false, true, true, false, false]);
}

#[test]
fn is_set_is_false_outside_the_glyph() {
    let mut font = BitmapFont::new(small());
    let solid = font.add_glyph(1, &[0b111; 4]).unwrap();
    let glyph = font.glyph(solid).unwrap();
    assert!(glyph.is_set(2, 3));
    assert!(!glyph.is_set(3, 0));
    assert!(!glyph.is_set(0, 4));
    assert!(!glyph.is_set(usize::MAX, usize::MAX));
}

#[test]
fn shaping_without_lookups_resolves_each_character() {
    let font = font();
    let (a, wide, fallback) = (font.glyph_id('A'), font.glyph_id('✅'), font.fallback());
    assert_eq!(
        font.shape("A✅\tz", Features::all()),
        [a, wide, None, fallback]
    );
}
