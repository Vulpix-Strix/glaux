use crate::font::{
    BitmapFont, ContextRule, Features, FontError, FontMetrics, GlyphId, GlyphSet, Ligature, Lookup,
};
use pretty_assertions::assert_eq;

/// A font of blank one-cell glyphs for `a`..=`z`, space and `-`/`>`/`<`/`=`,
/// plus unmapped alternates. The shape of a glyph doesn't matter here, only
/// which one each character ends up with.
struct Fixture {
    font: BitmapFont,
}

impl Fixture {
    fn new() -> Self {
        let mut font = BitmapFont::new(FontMetrics::new(2, 1).unwrap());
        for ch in ('a'..='z').chain([' ', '-', '>', '<', '=']) {
            font.insert(ch, 1, &[0]).unwrap();
        }
        font.insert('W', 2, &[0]).unwrap();
        Self { font }
    }

    fn id(&self, ch: char) -> GlyphId {
        self.font.glyph_id(ch).unwrap()
    }

    /// A new unmapped glyph `cells` wide.
    fn extra(&mut self, cells: u8) -> GlyphId {
        self.font.add_glyph(cells, &[0]).unwrap()
    }

    fn set(&self, chars: &str) -> GlyphSet {
        chars.chars().map(|ch| self.id(ch)).collect()
    }

    /// `text` as unsubstituted glyphs.
    fn plain(&self, text: &str) -> Vec<Option<GlyphId>> {
        text.chars().map(|ch| Some(self.id(ch))).collect()
    }
}

#[test]
fn single_substitutions_run_only_with_their_feature() {
    let mut f = Fixture::new();
    let alt = f.extra(1);
    let a = f.id('a');
    f.font
        .add_lookup(Lookup::Single {
            feature: Features::STYLISTIC_1,
            replace: vec![(a, alt)],
        })
        .unwrap();

    let mut expected = f.plain("bab");
    assert_eq!(f.font.shape("bab", Features::LIGATURES), expected);
    expected[1] = Some(alt);
    assert_eq!(f.font.shape("bab", Features::STYLISTIC_1), expected);
}

#[test]
fn lookups_without_a_feature_always_run() {
    let mut f = Fixture::new();
    let alt = f.extra(1);
    let a = f.id('a');
    f.font
        .add_lookup(Lookup::Single {
            feature: Features::empty(),
            replace: vec![(a, alt)],
        })
        .unwrap();
    assert_eq!(f.font.shape("a", Features::empty()), [Some(alt)]);
}

#[test]
fn a_ligature_replaces_each_glyph_of_its_run_with_one_part() {
    let mut f = Fixture::new();
    let parts = vec![f.extra(1), f.extra(1)];
    let ligature = Ligature {
        input: vec![f.id('-'), f.id('>')],
        parts: parts.clone(),
        bounded_by: None,
    };
    f.font
        .add_lookup(Lookup::Ligature {
            feature: Features::LIGATURES,
            ligatures: vec![ligature],
        })
        .unwrap();

    let shaped = f.font.shape("a->-b", Features::LIGATURES);
    let mut expected = f.plain("a->-b");
    expected[1..3].copy_from_slice(&[Some(parts[0]), Some(parts[1])]);
    assert_eq!(shaped, expected, "the trailing '-' has no '>' to join");
    assert_eq!(f.font.shape("a->-b", Features::empty()), f.plain("a->-b"));
}

#[test]
fn a_formed_ligature_consumes_its_run_before_overlapping_matches() {
    let mut f = Fixture::new();
    let parts = vec![f.extra(1), f.extra(1)];
    let equal = f.id('=');
    f.font
        .add_lookup(Lookup::Ligature {
            feature: Features::LIGATURES,
            ligatures: vec![Ligature {
                input: vec![equal, equal],
                parts: parts.clone(),
                bounded_by: None,
            }],
        })
        .unwrap();
    let joined = [Some(parts[0]), Some(parts[1])];
    assert_eq!(
        f.font.shape("===", Features::LIGATURES),
        [joined[0], joined[1], Some(equal)]
    );
    assert_eq!(
        f.font.shape("====", Features::LIGATURES),
        [joined[0], joined[1], joined[0], joined[1]]
    );
}

#[test]
fn a_bounded_ligature_forms_only_between_its_bounds() {
    let mut f = Fixture::new();
    let parts = vec![f.extra(1), f.extra(1)];
    let less = f.id('<');
    f.font
        .add_lookup(Lookup::Ligature {
            feature: Features::LIGATURES,
            ligatures: vec![Ligature {
                input: vec![less, less],
                parts: parts.clone(),
                bounded_by: Some(f.id(' ')),
            }],
        })
        .unwrap();

    let shaped = f.font.shape("a << b", Features::LIGATURES);
    assert_eq!(&shaped[2..4], [Some(parts[0]), Some(parts[1])]);
    for text in ["a<<b", "<< b", "a <<", "a <<b", "<<"] {
        assert_eq!(
            f.font.shape(text, Features::LIGATURES),
            f.plain(text),
            "{text:?}"
        );
    }
}

#[test]
fn context_rules_see_earlier_substitutions_and_later_originals() {
    let mut f = Fixture::new();
    let alt_a = f.extra(1);
    let a = f.id('a');
    // `a` becomes `alt_a` after an `x` or an already substituted `a`, but
    // only while another `a` follows.
    let after: GlyphSet = [f.id('x'), alt_a].into_iter().collect();
    f.font
        .add_lookup(Lookup::Context {
            feature: Features::CONTEXTUAL,
            rules: vec![ContextRule {
                backtrack: vec![after],
                input: vec![f.set("a")],
                lookahead: vec![f.set("a")],
                replace: Some(vec![(a, alt_a)]),
            }],
        })
        .unwrap();

    let shaped = f.font.shape("xaaa", Features::CONTEXTUAL);
    let mut expected = f.plain("xaaa");
    expected[1] = Some(alt_a);
    expected[2] = Some(alt_a);
    assert_eq!(shaped, expected, "the last a has no a after it");
    assert_eq!(f.font.shape("baaa", Features::CONTEXTUAL), f.plain("baaa"));
}

#[test]
fn an_exception_blocks_later_rules_over_its_whole_input() {
    let mut f = Fixture::new();
    let alt_a = f.extra(1);
    let a = f.id('a');
    let exception = ContextRule {
        backtrack: vec![f.set("b")],
        input: vec![f.set("a"), f.set("a")],
        lookahead: vec![],
        replace: None,
    };
    let anywhere = ContextRule {
        backtrack: vec![],
        input: vec![f.set("a")],
        lookahead: vec![],
        replace: Some(vec![(a, alt_a)]),
    };
    f.font
        .add_lookup(Lookup::Context {
            feature: Features::CONTEXTUAL,
            rules: vec![exception, anywhere],
        })
        .unwrap();

    let shaped = f.font.shape("baaa", Features::CONTEXTUAL);
    let mut expected = f.plain("baaa");
    expected[3] = Some(alt_a);
    assert_eq!(shaped, expected);
}

#[test]
fn replacements_must_keep_the_width_of_what_they_replace() {
    let mut f = Fixture::new();
    let (a, wide) = (f.id('a'), f.id('W'));
    assert_eq!(
        f.font.add_lookup(Lookup::Single {
            feature: Features::empty(),
            replace: vec![(a, wide)],
        }),
        Err(FontError::WidthMismatch { from: a, to: wide })
    );
    assert_eq!(
        f.font.add_lookup(Lookup::Ligature {
            feature: Features::empty(),
            ligatures: vec![Ligature {
                input: vec![a, a],
                parts: vec![a, wide],
                bounded_by: None,
            }],
        }),
        Err(FontError::WidthMismatch { from: a, to: wide })
    );
    assert_eq!(f.font.shape("a", Features::all()), f.plain("a"));
}

#[test]
fn ligatures_need_one_part_per_input_glyph() {
    let mut f = Fixture::new();
    let a = f.id('a');
    for (input, parts) in [(vec![a, a], vec![a]), (vec![], vec![])] {
        let (i, p) = (input.len(), parts.len());
        assert_eq!(
            f.font.add_lookup(Lookup::Ligature {
                feature: Features::empty(),
                ligatures: vec![Ligature {
                    input,
                    parts,
                    bounded_by: None,
                }],
            }),
            Err(FontError::LigatureShape { input: i, parts: p })
        );
    }
}

#[test]
fn rules_must_cover_a_glyph_and_name_only_glyphs_of_the_font() {
    let mut f = Fixture::new();
    let stranger = {
        let mut other = Fixture::new();
        (0..40).map(|_| other.extra(1)).last().unwrap()
    };
    let rule = |input: Vec<GlyphSet>, lookahead: Vec<GlyphSet>| Lookup::Context {
        feature: Features::empty(),
        rules: vec![ContextRule {
            backtrack: vec![],
            input,
            lookahead,
            replace: None,
        }],
    };
    assert_eq!(
        f.font.add_lookup(rule(vec![], vec![])),
        Err(FontError::EmptyContext)
    );
    let unknown = [stranger].into_iter().collect();
    assert_eq!(
        f.font.add_lookup(rule(vec![f.set("a")], vec![unknown])),
        Err(FontError::UnknownGlyph(stranger))
    );
}
