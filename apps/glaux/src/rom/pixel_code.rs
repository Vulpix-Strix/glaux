//! The system font is Pixel Code's glyph atlases decoded cell by cell, plus
//! the OpenType features of its build ported to font [`Lookup`]s.
//!
//! Upstream compiles the same PNGs into outline fonts with
//! `src/build_from_images.py`. The slot rules here follow that script, and
//! the lookups follow the `.fea` files kept beside the atlases in
//! `rom/font/pixel-code/features/`. See `rom/font/README.md` for the pinned
//! revision and how the files were renamed.

use std::collections::HashMap;

use anyhow::{Context, bail, ensure};
use glaux_machine::font::{
    BitmapFont, ContextRule, Features, FontMetrics, GlyphId, GlyphSet, Ligature, Lookup,
};

use super::decode_png;

/// A Pixel Code asset's name for error messages, and its embedded bytes.
macro_rules! asset {
    ($path:literal) => {
        (
            concat!("rom/font/pixel-code/", $path),
            include_bytes!(concat!("../../../../rom/font/pixel-code/", $path)) as &[u8],
        )
    };
}

/// A Pixel Code asset's name for error messages, and its embedded bytes.
type Asset = (&'static str, &'static [u8]);

/// Pixel Code's cell size.
const CELL_WIDTH: usize = 6;
const CELL_HEIGHT: usize = 12;

/// Codepoints whose atlas slot spans two cells: those of East Asian Width
/// W or F within the atlases' ranges, which is how the upstream build lays
/// slots out. A slot keeps its width even where its glyph is blank.
const DOUBLE_WIDTH: [u32; 21] = [
    0x25FD, 0x25FE, 0x2705, 0x270A, 0x270B, 0x2728, 0x274C, 0x274E, 0x2753, 0x2754, 0x2755, 0x2757,
    0x2795, 0x2796, 0x2797, 0x27B0, 0x27BF, 0x2B1B, 0x2B1C, 0x2B50, 0x2B55,
];

/// An atlas of consecutive codepoints, laid out left to right and top to
/// bottom.
struct Atlas {
    asset: Asset,
    first: u32,
    last: u32,
}

macro_rules! atlas {
    ($first:literal, $last:literal, $file:literal) => {
        Atlas {
            asset: asset!($file),
            first: $first,
            last: $last,
        }
    };
}

/// The atlases of the default glyphs.
const ATLASES: [Atlas; 24] = [
    atlas!(0x0000, 0x007F, "0000-007F.png"),
    atlas!(0x0080, 0x00FF, "0080-00FF.png"),
    atlas!(0x0100, 0x017F, "0100-017F.png"),
    atlas!(0x02B0, 0x02FF, "02B0-02FF.png"),
    atlas!(0x0370, 0x03FF, "0370-03FF.png"),
    atlas!(0x0400, 0x045F, "0400-045F.png"),
    atlas!(0x05D0, 0x05FF, "05D0-05FF.png"),
    atlas!(0x1D00, 0x1D7F, "1D00-1D7F.png"),
    atlas!(0x1FB00, 0x1FBFF, "1FB00-1FBFF.png"),
    atlas!(0x2000, 0x206F, "2000-206F.png"),
    atlas!(0x2070, 0x209F, "2070-209F.png"),
    atlas!(0x2190, 0x21FF, "2190-21FF.png"),
    atlas!(0x2500, 0x257F, "2500-257F.png"),
    atlas!(0x2580, 0x259F, "2580-259F.png"),
    atlas!(0x25A0, 0x25FF, "25A0-25FF.png"),
    atlas!(0x2700, 0x27BF, "2700-27BF.png"),
    atlas!(0x27F0, 0x27FF, "27F0-27FF.png"),
    atlas!(0x2800, 0x28FF, "2800-28FF.png"),
    atlas!(0x2900, 0x297F, "2900-297F.png"),
    atlas!(0x2B00, 0x2BFF, "2B00-2BFF.png"),
    atlas!(0x2C60, 0x2C7F, "2C60-2C7F.png"),
    atlas!(0xA720, 0xA7FF, "A720-A7FF.png"),
    atlas!(0xE0A0, 0xE0BF, "E0A0-E0BF.png"),
    atlas!(0xEE00, 0xEE0B, "EE00-EE0B.png"),
];

/// Dingbats drawn at twice the resolution, upstream's source for those its
/// 1× dingbat atlas leaves blank. Most use half-pixel detail a 6×12 cell
/// cannot show, so each pixel takes ink where any of its four half-pixels
/// has some. Thin strokes stay visible at the cost of some boldness.
const DOUBLE_RESOLUTION: Atlas = atlas!(0x2700, 0x27BF, "2700-27BF@2X.png");

/// The glyph for characters the font lacks, also mapped to U+0000 and
/// U+FFFD as upstream does.
const NOTDEF: Asset = asset!("_.notdef.png");

/// Alternate glyphs and the feature directory each belongs to.
const ALTERNATES: [(Alternates, Atlas); 9] = [
    (Alternates::Calt, atlas!(0x2A, 0x2A, "calt/002A.png")),
    (Alternates::Calt, atlas!(0x2B, 0x2B, "calt/002B.png")),
    (Alternates::Calt, atlas!(0x2D, 0x2D, "calt/002D.png")),
    (Alternates::Calt, atlas!(0x3A, 0x3A, "calt/003A.png")),
    (Alternates::Calt, atlas!(0x78, 0x78, "calt/0078.png")),
    (Alternates::Onum, atlas!(0x30, 0x39, "onum/0030-0039.png")),
    (Alternates::Ss01, atlas!(0x2C, 0x2C, "ss01/002C.png")),
    (Alternates::Ss01, atlas!(0x3B, 0x3B, "ss01/003B.png")),
    (Alternates::Ss20, atlas!(0x6C, 0x6C, "ss20/006C.png")),
];

/// Which set of alternates a glyph belongs to, named after the directory
/// upstream keeps it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Alternates {
    Calt,
    Onum,
    Ss01,
    Ss20,
}

/// The programming ligatures: their text, whether they only form between
/// spaces, and their drawing, one cell per character.
const LIGATURES: [(&str, bool, Asset); 34] = [
    ("!=", false, asset!("ligatures/exclam_equal.png")),
    ("!==", false, asset!("ligatures/exclam_equal_equal.png")),
    ("**", false, asset!("ligatures/asterisk_asterisk.png")),
    ("*=", false, asset!("ligatures/asterisk_equal.png")),
    ("*|=", false, asset!("ligatures/asterisk_bar_equal.png")),
    ("+|=", false, asset!("ligatures/plus_bar_equal.png")),
    ("-->", false, asset!("ligatures/hyphen_hyphen_greater.png")),
    ("->", false, asset!("ligatures/hyphen_greater.png")),
    ("-|=", false, asset!("ligatures/hyphen_bar_equal.png")),
    ("::", false, asset!("ligatures/colon_colon.png")),
    (":=", false, asset!("ligatures/colon_equal.png")),
    (
        "<!--",
        false,
        asset!("ligatures/less_exclam_hyphen_hyphen.png"),
    ),
    ("<-", false, asset!("ligatures/less_hyphen.png")),
    ("<<", true, asset!("ligatures/less_less.lazy.png")),
    ("<<<", true, asset!("ligatures/less_less_less.lazy.png")),
    ("<<=", false, asset!("ligatures/less_less_equal.png")),
    ("<<|=", false, asset!("ligatures/less_less_bar_equal.png")),
    ("<=", false, asset!("ligatures/less_equal.png")),
    ("==", false, asset!("ligatures/equal_equal.png")),
    ("===", false, asset!("ligatures/equal_equal_equal.png")),
    ("=>", false, asset!("ligatures/equal_greater.png")),
    (">=", false, asset!("ligatures/greater_equal.png")),
    (">>", true, asset!("ligatures/greater_greater.lazy.png")),
    (">>=", false, asset!("ligatures/greater_greater_equal.png")),
    (
        ">>>",
        true,
        asset!("ligatures/greater_greater_greater.lazy.png"),
    ),
    ("?.", false, asset!("ligatures/question_period.png")),
    ("|=", false, asset!("ligatures/bar_equal.png")),
    ("||", false, asset!("ligatures/bar_bar.png")),
    ("*/", false, asset!("ligatures/asterisk_slash.png")),
    ("://", false, asset!("ligatures/colon_slash_slash.png")),
    ("/*", false, asset!("ligatures/slash_asterisk.png")),
    ("/=", false, asset!("ligatures/slash_equal.png")),
    ("//", false, asset!("ligatures/slash_slash.png")),
    ("///", false, asset!("ligatures/slash_slash_slash.png")),
];

/// The glyphs no character maps to, by the names the feature rules use.
#[derive(Debug, Default)]
pub(super) struct Names {
    alternates: HashMap<(Alternates, char), GlyphId>,
    /// Each ligature's parts, by its text.
    parts: HashMap<&'static str, Vec<GlyphId>>,
}

impl Names {
    /// The alternate of `ch` in `set`.
    pub(super) fn alternate(&self, set: Alternates, ch: char) -> Option<GlyphId> {
        self.alternates.get(&(set, ch)).copied()
    }

    /// Part `index` of the ligature drawn for `text`.
    pub(super) fn part(&self, text: &str, index: usize) -> Option<GlyphId> {
        self.parts.get(text)?.get(index).copied()
    }
}

/// Builds the system font.
pub(super) fn load() -> anyhow::Result<BitmapFont> {
    build().map(|(font, _)| font)
}

/// Builds the system font, and the names of its unmapped glyphs.
pub(super) fn build() -> anyhow::Result<(BitmapFont, Names)> {
    let metrics = FontMetrics::new(CELL_WIDTH as u8, CELL_HEIGHT as u8)?;
    let mut font = BitmapFont::new(metrics);
    let mut names = Names::default();

    let (name, png) = NOTDEF;
    let notdef = slots(png, 0, 0, 1)
        .and_then(|slots| match slots.as_slice() {
            [slot] => Ok(font.add_glyph(1, &slot.rows)?),
            _ => bail!("expected one {CELL_WIDTH}x{CELL_HEIGHT} cell"),
        })
        .with_context(|| format!("invalid {name}"))?;
    font.set_fallback(notdef)?;
    font.map('\0', notdef)?;
    font.map(char::REPLACEMENT_CHARACTER, notdef)?;

    let atlases = ATLASES.iter().map(|atlas| (atlas, 1));
    for (atlas, scale) in atlases.chain([(&DOUBLE_RESOLUTION, 2)]) {
        let (name, png) = atlas.asset;
        let load = |font: &mut BitmapFont| -> anyhow::Result<()> {
            for slot in slots(png, atlas.first, atlas.last, scale)? {
                let Some(ch) = char::from_u32(slot.code) else {
                    continue;
                };
                // Upstream keeps a slot only where it draws something, and
                // the space. U+0000 already has `.notdef`, and the 2×
                // dingbats only fill what the 1× atlas leaves blank.
                let keep = slot.drawn || ch == ' ';
                if keep && ch != '\0' && font.glyph_id(ch).is_none() {
                    font.insert(ch, slot.cells, &slot.rows)?;
                }
            }
            Ok(())
        };
        load(&mut font).with_context(|| format!("invalid {name}"))?;
    }

    for (set, atlas) in &ALTERNATES {
        let (name, png) = atlas.asset;
        let load = |font: &mut BitmapFont, names: &mut Names| -> anyhow::Result<()> {
            for slot in slots(png, atlas.first, atlas.last, 1)? {
                let ch = char::from_u32(slot.code).context("invalid codepoint")?;
                ensure!(slot.drawn, "the alternate for {ch:?} is blank");
                let glyph = font.add_glyph(slot.cells, &slot.rows)?;
                names.alternates.insert((*set, ch), glyph);
            }
            Ok(())
        };
        load(&mut font, &mut names).with_context(|| format!("invalid {name}"))?;
    }

    for (text, _, (name, png)) in LIGATURES {
        let parts = ligature_parts(png, text.chars().count())
            .and_then(|parts| {
                parts
                    .iter()
                    .map(|rows| Ok(font.add_glyph(1, rows)?))
                    .collect::<anyhow::Result<Vec<_>>>()
            })
            .with_context(|| format!("invalid {name}"))?;
        names.parts.insert(text, parts);
    }

    for lookup in lookups(&font, &names)? {
        font.add_lookup(lookup)?;
    }
    Ok((font, names))
}

/// One atlas slot: a codepoint and its glyph.
struct Slot {
    code: u32,
    cells: u8,
    rows: Vec<u16>,
    /// Whether the slot has both ink and blank pixels in the atlas.
    /// Upstream skips the others, so a solid slot is as missing as an empty
    /// one.
    drawn: bool,
}

/// The slots of an atlas holding `first..=last` drawn at `scale` times the
/// cell size, read like the upstream build: left to right in rows of cells,
/// where a double-width slot that would cross the atlas's right edge starts
/// the next row instead. Each `scale`×`scale` block becomes one pixel, inked
/// where any of the block is.
fn slots(png: &[u8], first: u32, last: u32, scale: usize) -> anyhow::Result<Vec<Slot>> {
    let (width, height, ink) = decode_ink(png)?;
    let (cell_width, cell_height) = (CELL_WIDTH * scale, CELL_HEIGHT * scale);
    ensure!(
        width.is_multiple_of(cell_width) && height.is_multiple_of(cell_height),
        "{width}x{height} pixels is not a whole number of {cell_width}x{cell_height} cells"
    );
    let mut slots = Vec::new();
    let mut code = first;
    'rows: for top in (0..height).step_by(cell_height) {
        let mut left = 0;
        while left < width {
            let cells = if DOUBLE_WIDTH.contains(&code) { 2 } else { 1 };
            let span = usize::from(cells) * cell_width;
            if left + span > width {
                break;
            }
            let block = |x: usize, y: usize| {
                (y..y + scale).any(|y| (x..x + scale).any(|x| ink[y * width + x]))
            };
            let rows = (top..top + cell_height)
                .step_by(scale)
                .map(|y| {
                    (left..left + span)
                        .step_by(scale)
                        .fold(0u16, |row, x| row << 1 | u16::from(block(x, y)))
                })
                .collect();
            let inked = (top..top + cell_height)
                .flat_map(|y| (left..left + span).map(move |x| (x, y)))
                .filter(|&(x, y)| ink[y * width + x])
                .count();
            slots.push(Slot {
                code,
                cells,
                rows,
                drawn: inked > 0 && inked < span * cell_height,
            });
            left += span;
            code += 1;
            if code > last {
                break 'rows;
            }
        }
    }
    ensure!(
        code > last,
        "the atlas ends at U+{code:04X}, before U+{last:04X}"
    );
    Ok(slots)
}

/// The rows of each of a ligature's `count` one-cell parts.
fn ligature_parts(png: &[u8], count: usize) -> anyhow::Result<Vec<Vec<u16>>> {
    let (width, height, ink) = decode_ink(png)?;
    ensure!(
        (width, height) == (count * CELL_WIDTH, CELL_HEIGHT),
        "expected {count} cells of {CELL_WIDTH}x{CELL_HEIGHT}, got {width}x{height} pixels"
    );
    Ok((0..count)
        .map(|part| {
            (0..CELL_HEIGHT)
                .map(|y| {
                    let left = part * CELL_WIDTH;
                    (left..left + CELL_WIDTH)
                        .fold(0u16, |row, x| row << 1 | u16::from(ink[y * width + x]))
                })
                .collect()
        })
        .collect())
}

/// Decodes a 1-bit drawing: white pixels are ink, black ones blank.
fn decode_ink(png: &[u8]) -> anyhow::Result<(usize, usize, Vec<bool>)> {
    let (width, height, rgba) = decode_png(png)?;
    let ink = rgba
        .iter()
        .enumerate()
        .map(|(i, &pixel)| match pixel {
            [255, 255, 255, 255] => Ok(true),
            [0, 0, 0, 255] => Ok(false),
            other => bail!(
                "pixel ({}, {}) is {other:?}, neither opaque black nor white",
                i % width,
                i / width
            ),
        })
        .collect::<anyhow::Result<_>>()?;
    Ok((width, height, ink))
}

/// Looks glyphs up by character or name while the lookups are written.
struct Glyphs<'a> {
    font: &'a BitmapFont,
    names: &'a Names,
}

impl Glyphs<'_> {
    fn ch(&self, ch: char) -> anyhow::Result<GlyphId> {
        self.font
            .glyph_id(ch)
            .with_context(|| format!("no glyph for {ch:?}"))
    }

    fn alt(&self, set: Alternates, ch: char) -> anyhow::Result<GlyphId> {
        self.names
            .alternate(set, ch)
            .with_context(|| format!("no {set:?} alternate for {ch:?}"))
    }

    fn part(&self, text: &str, index: usize) -> anyhow::Result<GlyphId> {
        self.names
            .part(text, index)
            .with_context(|| format!("no part {index} of the {text:?} ligature"))
    }

    fn chars(&self, chars: impl IntoIterator<Item = char>) -> anyhow::Result<Vec<GlyphId>> {
        chars.into_iter().map(|ch| self.ch(ch)).collect()
    }
}

/// Every lookup the font applies, in the order upstream's feature code
/// places them: the stylistic sets, then the ligatures, then the
/// `AFTER_LIGATURES` files.
fn lookups(font: &BitmapFont, names: &Names) -> anyhow::Result<Vec<Lookup>> {
    let g = Glyphs { font, names };
    let mut lookups = vec![
        // onum.fea
        Lookup::Single {
            feature: Features::OLDSTYLE_FIGURES,
            replace: ('0'..='9')
                .map(|ch| Ok((g.ch(ch)?, g.alt(Alternates::Onum, ch)?)))
                .collect::<anyhow::Result<_>>()?,
        },
        // ss01.fea
        Lookup::Single {
            feature: Features::STYLISTIC_1,
            replace: vec![
                (g.ch(',')?, g.alt(Alternates::Ss01, ',')?),
                (g.ch(';')?, g.alt(Alternates::Ss01, ';')?),
            ],
        },
        // shifted.fea
        Lookup::Context {
            feature: Features::STYLISTIC_20,
            rules: vec![ContextRule {
                backtrack: vec![set(g
                    .chars("iìíîïuùúûüqt".chars())?
                    .into_iter()
                    .chain([g.alt(Alternates::Ss20, 'l')?]))],
                input: vec![set([g.ch('l')?])],
                lookahead: vec![],
                replace: Some(vec![(g.ch('l')?, g.alt(Alternates::Ss20, 'l')?)]),
            }],
        },
    ];
    lookups.extend(ligature_lookups(&g)?);
    lookups.push(ligature_cleanup(&g)?);
    lookups.push(contextual_alternates(&g)?);
    Ok(lookups)
}

fn set(glyphs: impl IntoIterator<Item = GlyphId>) -> GlyphSet {
    glyphs.into_iter().collect()
}

/// The generated `liga` feature: one lookup per ligature length, longest
/// first, as upstream's sorted rules compile to. Ligatures that only form
/// between spaces are chained rules upstream. Their bound gives the same
/// match.
fn ligature_lookups(g: &Glyphs<'_>) -> anyhow::Result<Vec<Lookup>> {
    let space = g.ch(' ')?;
    let mut by_length: Vec<(usize, Vec<Ligature>)> = Vec::new();
    for (text, lazy, _) in LIGATURES {
        let length = text.chars().count();
        let ligature = Ligature {
            input: g.chars(text.chars())?,
            parts: (0..length)
                .map(|index| g.part(text, index))
                .collect::<anyhow::Result<_>>()?,
            bounded_by: lazy.then_some(space),
        };
        match by_length.iter_mut().find(|(l, _)| *l == length) {
            Some((_, ligatures)) => ligatures.push(ligature),
            None => by_length.push((length, vec![ligature])),
        }
    }
    by_length.sort_by_key(|&(length, _)| std::cmp::Reverse(length));
    Ok(by_length
        .into_iter()
        .map(|(_, ligatures)| Lookup::Ligature {
            feature: Features::LIGATURES,
            ligatures,
        })
        .collect())
}

/// `ignore.fea`: splits runs of `=`, `/`, `*` and `|` longer than a
/// ligature back into plain characters, so `====` doesn't show as `===`
/// beside a lone `=`.
fn ligature_cleanup(g: &Glyphs<'_>) -> anyhow::Result<Lookup> {
    let mut rules = Vec::new();
    // == and ===, // and ///
    for (ch, two, three) in [('=', "==", "==="), ('/', "//", "///")] {
        let base = g.ch(ch)?;
        let ish = set([base, g.part(two, 0)?, g.part(three, 0)?]);
        let end = set([base, g.part(two, 1)?, g.part(three, 2)?]);
        let (mid, last) = (set([g.part(three, 1)?]), set([g.part(three, 2)?]));
        let first = set([g.part(three, 0)?]);
        let to_base = |input: &GlyphSet| Some(input.iter().map(|id| (id, base)).collect());
        let rule =
            |backtrack: Vec<GlyphSet>, input: GlyphSet, lookahead: Vec<GlyphSet>| ContextRule {
                replace: to_base(&input),
                backtrack,
                input: vec![input],
                lookahead,
            };
        rules.extend([
            rule(vec![], end.clone(), vec![ish.clone()]),
            rule(vec![], mid.clone(), vec![last.clone(), ish.clone()]),
            rule(vec![], first, vec![mid.clone(), last.clone(), ish.clone()]),
            rule(vec![end.clone()], ish.clone(), vec![]),
            rule(vec![end.clone()], mid, vec![]),
            rule(vec![end.clone()], last, vec![]),
            rule(vec![end.clone(), ish], end, vec![]),
        ]);
    }
    // ** and ||
    for (ch, two, extra) in [
        ('*', "**", Some(('*', Alternates::Calt))),
        ('|', "||", None),
    ] {
        let base = g.ch(ch)?;
        let extra = extra
            .map(|(ch, set)| g.alt(set, ch))
            .transpose()?
            .into_iter();
        let ish = set([base, g.part(two, 0)?].into_iter().chain(extra.clone()));
        let end = set([base, g.part(two, 1)?].into_iter().chain(extra));
        let rule =
            |backtrack: Vec<GlyphSet>, input: GlyphSet, lookahead: Vec<GlyphSet>| ContextRule {
                replace: Some(input.iter().map(|id| (id, base)).collect()),
                backtrack,
                input: vec![input],
                lookahead,
            };
        rules.extend([
            rule(vec![], end.clone(), vec![ish.clone()]),
            rule(vec![end.clone()], ish.clone(), vec![]),
            rule(vec![], ish.clone(), vec![end.clone(), ish.clone()]),
            rule(vec![end.clone(), ish], end, vec![]),
        ]);
    }
    Ok(Lookup::Context {
        feature: Features::LIGATURES,
        rules,
    })
}

/// `calt.fea`: punctuation centred between capitals or digits, and `*`,
/// `-`, `+` and `x` lowered or shaped to sit among lowercase letters and
/// numbers.
fn contextual_alternates(g: &Glyphs<'_>) -> anyhow::Result<Lookup> {
    use Alternates::{Calt, Onum, Ss20};

    let latin1 = |range: std::ops::RangeInclusive<char>| range.filter(|&ch| ch != '×' && ch != '÷');
    let digits: Vec<GlyphId> = g.chars('0'..='9')?;
    let onum: Vec<GlyphId> = ('0'..='9')
        .map(|ch| g.alt(Onum, ch))
        .collect::<anyhow::Result<_>>()?;
    let upper = set(digits
        .iter()
        .copied()
        .chain(g.chars(('A'..='Z').chain(latin1('À'..='ß')))?)
        .chain([g.alt(Calt, ':')?]));
    let lower = set(g
        .chars(('a'..='z').chain(latin1('à'..='ÿ')))?
        .into_iter()
        .chain(onum.iter().copied())
        .chain([
            g.alt(Calt, '+')?,
            g.alt(Calt, '-')?,
            g.alt(Calt, '*')?,
            g.alt(Ss20, 'l')?,
        ]));
    let digit = set(digits.iter().chain(&onum).copied());
    let hex_digit = set(digits
        .iter()
        .chain(&onum)
        .copied()
        .chain(g.chars("abcdefABCDEF".chars())?));
    let zero = set([g.ch('0')?, g.alt(Onum, '0')?]);
    let one = |ch: char| g.ch(ch).map(|id| set([id]));
    let (space, asterisk, colon, hyphen, plus, x) = (
        one(' ')?,
        one('*')?,
        one(':')?,
        one('-')?,
        one('+')?,
        one('x')?,
    );
    let cpunct = Some(
        ['+', '-', '*', ':', 'x']
            .into_iter()
            .map(|ch| Ok((g.ch(ch)?, g.alt(Calt, ch)?)))
            .collect::<anyhow::Result<Vec<_>>>()?,
    );
    let rule = |backtrack: &[&GlyphSet], input: &[&GlyphSet], lookahead: &[&GlyphSet], sub| {
        let own = |sets: &[&GlyphSet]| sets.iter().map(|&set| set.clone()).collect();
        ContextRule {
            backtrack: own(backtrack),
            input: own(input),
            lookahead: own(lookahead),
            replace: if sub { cpunct.clone() } else { None },
        }
    };
    let rules = vec![
        // *EQ *eq
        rule(&[&upper], &[&asterisk], &[&space, &lower], false),
        rule(&[&upper, &space], &[&asterisk], &[&space, &lower], false),
        rule(&[&asterisk], &[&asterisk, &asterisk], &[&lower], false),
        rule(
            &[&asterisk],
            &[&asterisk, &asterisk],
            &[&space, &lower],
            false,
        ),
        rule(&[], &[&asterisk], &[&lower], true),
        rule(&[], &[&asterisk], &[&space, &lower], true),
        rule(&[], &[&asterisk], &[&asterisk, &lower], true),
        rule(&[], &[&asterisk], &[&asterisk, &space, &lower], true),
        // a:e A:E
        rule(&[&upper], &[&colon], &[&upper], true),
        rule(&[&upper, &space], &[&colon], &[&upper], true),
        rule(&[&upper], &[&colon], &[&space, &upper], true),
        rule(&[&upper, &space], &[&colon], &[&space, &upper], true),
        // C-O c-o
        rule(&[&hyphen], &[&hyphen], &[&lower], false),
        rule(&[], &[&hyphen], &[&lower], true),
        // X+Z x+z
        rule(&[&plus], &[&plus], &[&lower], false),
        rule(&[], &[&plus], &[&lower], true),
        // 0xFF 1920x1080
        rule(&[&zero], &[&x], &[&hex_digit], true),
        rule(&[&digit], &[&x], &[&digit], true),
    ];
    Ok(Lookup::Context {
        feature: Features::CONTEXTUAL,
        rules,
    })
}

#[cfg(test)]
mod tests;
