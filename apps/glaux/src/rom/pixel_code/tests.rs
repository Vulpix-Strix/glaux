use std::sync::LazyLock;

use super::*;
use pretty_assertions::assert_eq;

const UPSTREAM_CMAP: &str = include_str!("upstream_cmap.txt");

static FONT: LazyLock<(BitmapFont, Names)> = LazyLock::new(|| build().unwrap());

fn font() -> &'static BitmapFont {
    &FONT.0
}

fn names() -> &'static Names {
    &FONT.1
}

/// The characters the upstream font maps and the cells each spans.
fn upstream() -> Vec<(char, usize)> {
    UPSTREAM_CMAP
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let (code, cells) = line.split_once(' ').unwrap();
            let ch = char::from_u32(u32::from_str_radix(code, 16).unwrap()).unwrap();
            (ch, cells.parse().unwrap())
        })
        .collect()
}

/// Rows of `ch`'s glyph, drawn as `#` and `.`.
fn picture(ch: char) -> Vec<String> {
    let glyph = font().glyph(font().glyph_id(ch).unwrap()).unwrap();
    (0..CELL_HEIGHT)
        .map(|y| {
            (0..usize::from(glyph.width()))
                .map(|x| if glyph.is_set(x, y) { '#' } else { '.' })
                .collect()
        })
        .collect()
}

fn base(ch: char) -> Option<GlyphId> {
    font().glyph_id(ch)
}

fn part(text: &str, index: usize) -> Option<GlyphId> {
    names().part(text, index)
}

fn alt(set: Alternates, ch: char) -> Option<GlyphId> {
    names().alternate(set, ch)
}

/// `text` drawn with no substitutions.
fn plain(text: &str) -> Vec<Option<GlyphId>> {
    font().shape(text, Features::empty())
}

#[test]
fn covers_exactly_the_characters_upstream_maps_at_their_widths() {
    let ours: Vec<(char, usize)> = font().chars().map(|ch| (ch, font().cells(ch))).collect();
    assert_eq!(ours.len(), 1792);
    assert_eq!(ours, upstream());
}

#[test]
fn glyphs_keep_the_atlas_pixels_in_six_by_twelve_cells() {
    assert_eq!(font().metrics(), FontMetrics::SYSTEM);
    #[rustfmt::skip]
    let a = [
        "......", "......", "......", ".###..", "#...#.", "#####.",
        "#...#.", "#...#.", "#...#.", "#...#.", "......", "......",
    ];
    assert_eq!(picture('A'), a);
    // A sextant from the supplementary plane: the top-left third of the
    // cell, two cells high in sextant terms.
    #[rustfmt::skip]
    let sextant = [
        "###...", "###...", "###...", "###...", "......", "......",
        "......", "......", "......", "......", "......", "......",
    ];
    assert_eq!(picture('🬀'), sextant);
}

#[test]
fn double_width_glyphs_span_two_cells() {
    for ch in ['✅', '❌', '⭕', '⬛'] {
        let glyph = font().glyph(base(ch).unwrap()).unwrap();
        assert_eq!((glyph.cells(), glyph.width()), (2, 12), "{ch:?}");
    }
    assert_eq!(font().text_cells("a✅b"), 4);
}

#[test]
fn double_resolution_dingbats_ink_every_pixel_any_half_pixel_touches() {
    #[rustfmt::skip]
    let star = [
        "......", "......", "......", "......", "..#...", "..##..",
        "#####.", ".###..", "..#...", "......", "......", "......",
    ];
    assert_eq!(picture('✦'), star);
}

#[test]
fn missing_characters_draw_notdef_which_also_serves_nul_and_replacement() {
    let notdef = font().fallback().unwrap();
    assert_eq!(base('\u{FFFD}'), Some(notdef));
    assert_eq!(base('\0'), Some(notdef));
    for ch in ['中', '\u{A0}', '█', '\u{301}'] {
        assert_eq!(base(ch), None, "{ch:?} is outside the repertoire");
        assert_eq!(font().resolve(ch), Some(notdef), "{ch:?}");
        assert_eq!(font().cells(ch), 1, "{ch:?}");
    }
}

#[test]
fn ligatures_replace_each_character_with_one_part() {
    let shaped = font().shape("a -> b", Features::LIGATURES);
    let mut expected = plain("a -> b");
    expected[2..4].copy_from_slice(&[part("->", 0), part("->", 1)]);
    assert_eq!(shaped, expected);
    assert_eq!(font().shape("a -> b", Features::empty()), plain("a -> b"));
}

#[test]
fn longer_ligatures_form_before_shorter_ones_anywhere_in_the_text() {
    // `<-` starts first, but `-->` is longer and wins.
    let shaped = font().shape("<-->", Features::LIGATURES);
    let arrow = (0..3).map(|i| part("-->", i));
    assert_eq!(
        shaped,
        [base('<')].into_iter().chain(arrow).collect::<Vec<_>>()
    );
}

#[test]
fn runs_longer_than_any_ligature_stay_plain() {
    for text in ["====", "////", "***", "|||", "=====", "a == = b"] {
        let shaped = font().shape(text, Features::LIGATURES);
        let joined = |text: &str| (0..text.len()).map(|i| part(text, i)).collect::<Vec<_>>();
        match text {
            "a == = b" => {
                let mut expected = plain(text);
                expected[2..4].copy_from_slice(&joined("=="));
                assert_eq!(shaped, expected, "{text:?}");
            }
            _ => assert_eq!(shaped, plain(text), "{text:?}"),
        }
    }
    let triple: Vec<_> = (0..3).map(|i| part("===", i)).collect();
    assert_eq!(font().shape("===", Features::LIGATURES), triple);
}

#[test]
fn shift_ligatures_form_only_between_spaces() {
    let shaped = font().shape("a << b", Features::LIGATURES);
    assert_eq!(&shaped[2..4], [part("<<", 0), part("<<", 1)]);
    for text in ["a<<b", "<< b", "a <<", "1 <<2"] {
        assert_eq!(
            font().shape(text, Features::LIGATURES),
            plain(text),
            "{text:?}"
        );
    }
}

#[test]
fn contextual_alternates_follow_their_neighbours() {
    let at = |text: &str, index: usize| font().shape(text, Features::CONTEXTUAL)[index];
    let x = alt(Alternates::Calt, 'x');
    assert_eq!(at("0xFF", 1), x, "hex prefix");
    assert_eq!(at("1920x1080", 4), x, "dimensions");
    assert_eq!(at("max", 1), base('a'));
    assert_eq!(at("max", 2), base('x'));
    assert_eq!(at("A:B", 1), alt(Alternates::Calt, ':'), "between capitals");
    assert_eq!(at("a:b", 1), base(':'));
    assert_eq!(at("c-o", 1), alt(Alternates::Calt, '-'), "before lowercase");
    assert_eq!(at("c--o", 2), base('-'), "a run of hyphens stays");
    assert_eq!(at("x+z", 1), alt(Alternates::Calt, '+'));
    assert_eq!(at("*eq", 0), alt(Alternates::Calt, '*'));
    assert_eq!(at("A* eq", 1), base('*'), "a capital before keeps it");
}

#[test]
fn stylistic_sets_and_old_style_figures_are_opt_in() {
    let all = Features::OLDSTYLE_FIGURES | Features::STYLISTIC_1 | Features::STYLISTIC_20;
    let shaped = font().shape("1,;il al", all);
    assert_eq!(shaped[0], alt(Alternates::Onum, '1'));
    assert_eq!(shaped[1], alt(Alternates::Ss01, ','));
    assert_eq!(shaped[2], alt(Alternates::Ss01, ';'));
    assert_eq!(shaped[4], alt(Alternates::Ss20, 'l'), "after an i");
    assert_eq!(shaped[7], base('l'), "after an a");
    let defaults = Features::LIGATURES | Features::CONTEXTUAL;
    assert_eq!(font().shape("1,;il", defaults), plain("1,;il"));
}

#[test]
fn substitutions_never_change_a_glyph_width() {
    let text = "a -> b <!-- c --> 0xFF A:B ** || === /// :: != <<= ✅ x";
    let widths = |glyphs: Vec<Option<GlyphId>>| {
        glyphs
            .into_iter()
            .map(|id| id.and_then(|id| font().glyph(id)).map(|g| g.cells()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        widths(font().shape(text, Features::all())),
        widths(plain(text))
    );
}
