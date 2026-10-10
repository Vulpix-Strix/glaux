use super::*;
use pretty_assertions::assert_eq;

// An "A" in the Tamzen 5×9 style, pre-shifted to 5-bit rows.
const A_ROWS: [u8; 9] = [
    0b00000, 0b00000, 0b01100, 0b10010, 0b11110, 0b10010, 0b10010, 0b00000, 0b00000,
];

fn system_font_with_a() -> BitmapFont {
    let mut font = BitmapFont::new(FontMetrics::SYSTEM);
    font.insert('A', &A_ROWS).unwrap();
    font
}

#[test]
fn system_metrics() {
    assert_eq!(GLYPH_WIDTH, 5);
    assert_eq!(GLYPH_HEIGHT, 9);
    assert_eq!(CELL_WIDTH, 6);
    assert_eq!(CELL_HEIGHT, 10);
    assert_eq!(CELL_WIDTH, GLYPH_WIDTH + 1);
    assert_eq!(CELL_HEIGHT, GLYPH_HEIGHT + 1);

    let m = FontMetrics::SYSTEM;
    assert_eq!(
        (
            m.glyph_width(),
            m.glyph_height(),
            m.cell_width(),
            m.cell_height()
        ),
        (5, 9, 6, 10)
    );
    assert_eq!(FontMetrics::new(5, 9, 6, 10), Ok(FontMetrics::SYSTEM));
}

#[test]
fn rejects_invalid_metrics() {
    for (gw, gh, cw, ch) in [
        (0, 9, 6, 10),
        (9, 9, 10, 10),
        (5, 0, 6, 10),
        (5, 9, 4, 10),
        (5, 9, 6, 8),
    ] {
        assert!(
            matches!(
                FontMetrics::new(gw, gh, cw, ch),
                Err(FontError::InvalidMetrics { .. })
            ),
            "{gw}x{gh} in {cw}x{ch}"
        );
    }
}

#[test]
fn looks_up_inserted_glyph() {
    let font = system_font_with_a();
    let glyph = font.glyph('A').expect("'A' was inserted");
    assert_eq!(glyph.rows(), &A_ROWS);
}

#[test]
fn missing_or_unsupported_chars_have_no_glyph() {
    let font = system_font_with_a();
    for ch in ['B', ' ', '~', '\n', '\u{7F}', 'é'] {
        assert_eq!(font.glyph(ch), None, "{ch:?}");
    }
}

#[test]
fn insert_rejects_chars_outside_printable_ascii() {
    let mut font = BitmapFont::new(FontMetrics::SYSTEM);
    for ch in ['\u{1F}', '\u{7F}', 'é'] {
        assert_eq!(
            font.insert(ch, &A_ROWS),
            Err(FontError::UnsupportedChar(ch))
        );
    }
}

#[test]
fn insert_rejects_wrong_row_count() {
    let mut font = BitmapFont::new(FontMetrics::SYSTEM);
    assert_eq!(
        font.insert('A', &A_ROWS[..7]),
        Err(FontError::WrongRowCount {
            ch: 'A',
            expected: 9,
            actual: 7
        })
    );
    assert_eq!(font.glyph('A'), None);
}

#[test]
fn insert_rejects_rows_wider_than_glyph() {
    let mut font = BitmapFont::new(FontMetrics::SYSTEM);
    let mut rows = A_ROWS;
    rows[3] = 0b10_0000;
    assert_eq!(
        font.insert('A', &rows),
        Err(FontError::RowTooWide {
            ch: 'A',
            row: 3,
            bits: 0b10_0000
        })
    );
}

#[test]
fn is_set_reads_bits_left_to_right() {
    let font = system_font_with_a();
    let glyph = font.glyph('A').unwrap();

    // Row 3 is 0b10010: columns 0 and 3.
    let row3: Vec<bool> = (0..5).map(|x| glyph.is_set(x, 3)).collect();
    assert_eq!(row3, [true, false, false, true, false]);
    // Row 4 is 0b11110: columns 0..4.
    let row4: Vec<bool> = (0..5).map(|x| glyph.is_set(x, 4)).collect();
    assert_eq!(row4, [true, true, true, true, false]);
}

#[test]
fn is_set_is_false_outside_the_glyph() {
    let mut font = BitmapFont::new(FontMetrics::SYSTEM);
    font.insert('#', &[0b11111; 9]).unwrap();
    let glyph = font.glyph('#').unwrap();

    assert!(glyph.is_set(4, 8));
    assert!(!glyph.is_set(5, 0));
    assert!(!glyph.is_set(0, 9));
    assert!(!glyph.is_set(usize::MAX, usize::MAX));
}

#[test]
fn supports_other_metrics() {
    let mut font = BitmapFont::new(FontMetrics::new(3, 3, 4, 4).unwrap());
    font.insert('x', &[0b101, 0b010, 0b101]).unwrap();
    assert_eq!(
        font.insert('y', &[0b1000, 0, 0]),
        Err(FontError::RowTooWide {
            ch: 'y',
            row: 0,
            bits: 0b1000
        })
    );

    let glyph = font.glyph('x').unwrap();
    assert!(glyph.is_set(0, 0) && glyph.is_set(2, 0) && glyph.is_set(1, 1));
    assert!(!glyph.is_set(1, 0) && !glyph.is_set(3, 0) && !glyph.is_set(0, 3));
}
