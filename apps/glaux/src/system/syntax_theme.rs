//! The colors of syntax categories, as indices into the system palette.
//!
//! This is the only place that decides how a [`SyntaxKind`] looks. Grammars and
//! their queries only say what a piece of source is.

use glaux_syntax::SyntaxKind;

/// The color of `kind`. Indices are into the Zughy 32 colors that the system
/// reserves in every frame palette, so they are always below 32.
pub(super) const fn color(kind: SyntaxKind) -> u8 {
    match kind {
        SyntaxKind::Keyword => 25,
        SyntaxKind::String => 7,
        SyntaxKind::Number => 5,
        SyntaxKind::Constant => 27,
        SyntaxKind::Function => 18,
        SyntaxKind::Type => 17,
        SyntaxKind::Variable => 6,
        SyntaxKind::Operator => 15,
        SyntaxKind::Comment => 14,
    }
}

#[cfg(test)]
mod tests;
