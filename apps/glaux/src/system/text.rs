//! Fitting text into cells of a bitmap font: clipping and wrapping.

use glaux_machine::font::BitmapFont;

/// Marks text cut short to fit.
const ELLIPSIS: &str = "..";

/// `text` cut to `cells` character cells, ending in `..` when something
/// was left out.
pub(crate) fn clip(font: &BitmapFont, text: &str, cells: usize) -> String {
    if font.text_cells(text) <= cells {
        return text.to_owned();
    }
    let room = cells.saturating_sub(ELLIPSIS.len());
    let mut used = 0;
    let mut out = String::new();
    for ch in text.chars() {
        let width = font.cells(ch);
        if used + width > room {
            break;
        }
        used += width;
        out.push(ch);
    }
    if cells >= ELLIPSIS.len() {
        out.push_str(ELLIPSIS);
    }
    out
}

/// `text` cut to `cells` character cells with [`ELLIPSIS`] at its end, for a
/// last line that has more text after it.
pub(crate) fn cut(font: &BitmapFont, text: &str, cells: usize) -> String {
    let room = cells.saturating_sub(font.text_cells(ELLIPSIS));
    let mut kept = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let width = font.cells(ch);
        if used + width > room {
            break;
        }
        used += width;
        kept.push(ch);
    }
    kept.push_str(ELLIPSIS);
    kept
}

/// Breaks `text` into lines of at most `cells` character cells, at spaces
/// when it has any and inside words otherwise. Explicit newlines are kept.
pub(crate) fn wrap(font: &BitmapFont, text: &str, cells: usize) -> Vec<String> {
    let cells = cells.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut used = 0;
        for word in paragraph.split(' ') {
            let width = font.text_cells(word);
            let space = usize::from(!line.is_empty());
            if used + space + width <= cells {
                if space == 1 {
                    line.push(' ');
                }
                line.push_str(word);
                used += space + width;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            // A word longer than a line is split wherever it reaches the edge.
            for ch in word.chars() {
                let width = font.cells(ch);
                if used + width > cells && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                }
                line.push(ch);
                used += width;
            }
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::system::test_font::LAYOUT_FONT;

    #[test]
    fn clip_keeps_text_that_fits_and_marks_what_it_cuts() {
        assert_eq!(clip(&LAYOUT_FONT, "abcd", 4), "abcd");
        assert_eq!(clip(&LAYOUT_FONT, "abcde", 4), "ab..");
        assert_eq!(clip(&LAYOUT_FONT, "abcde", 1), "");
    }

    #[test]
    fn clip_never_splits_a_wide_glyph_at_the_edge() {
        // The wide glyph would need cells 3 and 4 but only 3 remain before "..".
        assert_eq!(clip(&LAYOUT_FONT, "ab✅cd", 5), "ab..");
    }

    #[test]
    fn cut_always_marks_the_end_even_when_the_line_fits() {
        assert_eq!(cut(&LAYOUT_FONT, "abc", 6), "abc..");
        assert_eq!(cut(&LAYOUT_FONT, "abcdefgh", 6), "abcd..");
    }

    #[test]
    fn wrap_keeps_explicit_newlines_and_splits_long_words() {
        assert_eq!(
            wrap(&LAYOUT_FONT, "one two\n\nabcdefg", 5),
            ["one", "two", "", "abcde", "fg"]
        );
    }
}
