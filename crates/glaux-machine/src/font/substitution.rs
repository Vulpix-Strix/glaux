//! Glyph substitutions, a small subset of OpenType GSUB that fixed-cell
//! fonts need. They are applied to one glyph per character.
//!
//! Every substitution replaces glyphs one for one with glyphs of the same
//! width, so it changes how text looks but never which cells its characters
//! take.

use super::{FontError, GlyphId};

bitflags::bitflags! {
    /// Optional substitutions to apply, named after the OpenType features
    /// whose rules a font ports into its [`Lookup`]s.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Features: u8 {
        /// Standard ligatures (`liga`).
        const LIGATURES = 1 << 0;
        /// Contextual alternates (`calt`).
        const CONTEXTUAL = 1 << 1;
        /// Old-style figures (`onum`).
        const OLDSTYLE_FIGURES = 1 << 2;
        /// Stylistic set 1 (`ss01`).
        const STYLISTIC_1 = 1 << 3;
        /// Stylistic set 20 (`ss20`).
        const STYLISTIC_20 = 1 << 4;
    }
}

/// A set of glyphs a rule matches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlyphSet(Vec<GlyphId>);

impl GlyphSet {
    pub fn contains(&self, glyph: GlyphId) -> bool {
        self.0.binary_search(&glyph).is_ok()
    }

    /// The glyphs in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = GlyphId> + '_ {
        self.0.iter().copied()
    }

    fn matches(&self, glyph: Option<GlyphId>) -> bool {
        glyph.is_some_and(|glyph| self.contains(glyph))
    }
}

impl FromIterator<GlyphId> for GlyphSet {
    fn from_iter<I: IntoIterator<Item = GlyphId>>(glyphs: I) -> Self {
        let mut glyphs: Vec<_> = glyphs.into_iter().collect();
        glyphs.sort_unstable();
        glyphs.dedup();
        Self(glyphs)
    }
}

/// A run of glyphs drawn as one shape split into one part per glyph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ligature {
    pub input: Vec<GlyphId>,
    /// What each input glyph is replaced with, in order.
    pub parts: Vec<GlyphId>,
    /// When set, the run only forms between two of this glyph, such as
    /// `<<` standing alone between spaces.
    pub bounded_by: Option<GlyphId>,
}

impl Ligature {
    fn matches(&self, glyphs: &[Option<GlyphId>], at: usize) -> bool {
        let end = at + self.input.len();
        let Some(run) = glyphs.get(at..end) else {
            return false;
        };
        if !run.iter().zip(&self.input).all(|(g, i)| *g == Some(*i)) {
            return false;
        }
        self.bounded_by.is_none_or(|bound| {
            at > 0 && glyphs[at - 1] == Some(bound) && glyphs.get(end) == Some(&Some(bound))
        })
    }
}

/// A substitution that applies only where the glyphs around it match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextRule {
    /// The glyphs right before the input, in text order.
    pub backtrack: Vec<GlyphSet>,
    /// The glyphs the rule covers, one set per glyph.
    pub input: Vec<GlyphSet>,
    /// The glyphs right after the input, in text order.
    pub lookahead: Vec<GlyphSet>,
    /// Replacements for the first input glyph. `None` makes the rule an
    /// exception: where it matches, no later rule of its lookup applies to
    /// its input.
    pub replace: Option<Vec<(GlyphId, GlyphId)>>,
}

impl ContextRule {
    fn matches(&self, glyphs: &[Option<GlyphId>], at: usize) -> bool {
        let end = at + self.input.len();
        let (Some(start), Some(input), Some(after)) = (
            at.checked_sub(self.backtrack.len()),
            glyphs.get(at..end),
            glyphs.get(end..end + self.lookahead.len()),
        ) else {
            return false;
        };
        let all = |sets: &[GlyphSet], glyphs: &[Option<GlyphId>]| {
            sets.iter().zip(glyphs).all(|(set, &g)| set.matches(g))
        };
        all(&self.backtrack, &glyphs[start..at])
            && all(&self.input, input)
            && all(&self.lookahead, after)
    }
}

/// One step of a font's substitutions. A lookup runs only when the
/// [`Features`] being drawn with contain its `feature`. An empty `feature`
/// always runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// Replaces every glyph in `replace` by its pair.
    Single {
        feature: Features,
        replace: Vec<(GlyphId, GlyphId)>,
    },
    /// Forms ligatures left to right. At each glyph the first ligature in
    /// `ligatures` that matches there wins, and forming one moves past its
    /// whole run.
    Ligature {
        feature: Features,
        ligatures: Vec<Ligature>,
    },
    /// Applies rules left to right. At each glyph the first rule in `rules`
    /// that matches there applies, and moves past its whole input. Rules see
    /// the glyphs before them as already substituted and those after them as
    /// not yet.
    Context {
        feature: Features,
        rules: Vec<ContextRule>,
    },
}

impl Lookup {
    fn feature(&self) -> Features {
        match self {
            Self::Single { feature, .. }
            | Self::Ligature { feature, .. }
            | Self::Context { feature, .. } => *feature,
        }
    }

    /// The lookup with its replacement tables sorted, once every glyph it
    /// names is known to `cells` and every replacement spans as many cells
    /// as the glyph it replaces.
    pub(super) fn validated(
        mut self,
        cells: impl Fn(GlyphId) -> Option<u8>,
    ) -> Result<Self, FontError> {
        let known = |id: GlyphId| cells(id).ok_or(FontError::UnknownGlyph(id));
        let same_width = |from: GlyphId, to: GlyphId| {
            if known(from)? == known(to)? {
                Ok(())
            } else {
                Err(FontError::WidthMismatch { from, to })
            }
        };
        let replacements = |pairs: &mut Vec<(GlyphId, GlyphId)>| {
            pairs.sort_unstable_by_key(|&(from, _)| from);
            pairs.dedup_by_key(|&mut (from, _)| from);
            pairs
                .iter()
                .try_for_each(|&(from, to)| same_width(from, to))
        };
        match &mut self {
            Self::Single { replace, .. } => replacements(replace)?,
            Self::Ligature { ligatures, .. } => {
                for ligature in ligatures {
                    let (input, parts) = (ligature.input.len(), ligature.parts.len());
                    if input == 0 || input != parts {
                        return Err(FontError::LigatureShape { input, parts });
                    }
                    for (&from, &to) in ligature.input.iter().zip(&ligature.parts) {
                        same_width(from, to)?;
                    }
                    ligature.bounded_by.map(known).transpose()?;
                }
            }
            Self::Context { rules, .. } => {
                for rule in rules {
                    if rule.input.is_empty() {
                        return Err(FontError::EmptyContext);
                    }
                    for set in rule
                        .backtrack
                        .iter()
                        .chain(&rule.input)
                        .chain(&rule.lookahead)
                    {
                        set.0.iter().try_for_each(|&id| known(id).map(drop))?;
                    }
                    if let Some(replace) = &mut rule.replace {
                        replacements(replace)?;
                    }
                }
            }
        }
        Ok(self)
    }

    pub(super) fn apply(&self, glyphs: &mut [Option<GlyphId>], features: Features) {
        if !features.contains(self.feature()) {
            return;
        }
        match self {
            Self::Single { replace, .. } => {
                for glyph in glyphs.iter_mut() {
                    *glyph = glyph.map(|id| replaced(replace, id));
                }
            }
            Self::Ligature { ligatures, .. } => {
                let mut at = 0;
                while at < glyphs.len() {
                    match ligatures.iter().find(|l| l.matches(glyphs, at)) {
                        Some(ligature) => {
                            for (glyph, &part) in glyphs[at..].iter_mut().zip(&ligature.parts) {
                                *glyph = Some(part);
                            }
                            at += ligature.parts.len();
                        }
                        None => at += 1,
                    }
                }
            }
            Self::Context { rules, .. } => {
                let mut at = 0;
                while at < glyphs.len() {
                    match rules.iter().find(|rule| rule.matches(glyphs, at)) {
                        Some(rule) => {
                            if let Some(replace) = &rule.replace {
                                glyphs[at] = glyphs[at].map(|id| replaced(replace, id));
                            }
                            at += rule.input.len();
                        }
                        None => at += 1,
                    }
                }
            }
        }
    }
}

/// `glyph`'s pair in the sorted table `replace`, or `glyph` itself.
fn replaced(replace: &[(GlyphId, GlyphId)], glyph: GlyphId) -> GlyphId {
    replace
        .binary_search_by_key(&glyph, |&(from, _)| from)
        .map_or(glyph, |at| replace[at].1)
}

#[cfg(test)]
mod tests;
