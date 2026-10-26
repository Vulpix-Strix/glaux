use glaux_machine::palette::{Rgb, ZUGHY_32};

use super::*;
use crate::system::screen::{BACKGROUND, TEXT};

/// The editor mockup's `--syn-*` tokens.
fn mockup(kind: SyntaxKind) -> Rgb {
    Rgb::from_hex(match kind {
        SyntaxKind::Keyword => 0xffaeb6,
        SyntaxKind::String => 0xb6d53c,
        SyntaxKind::Number => 0xeea160,
        SyntaxKind::Constant => 0xf47e1b,
        SyntaxKind::Function => 0x28ccdf,
        SyntaxKind::Type => 0x8aebf1,
        SyntaxKind::Variable => 0xf4cca1,
        SyntaxKind::Operator => 0xcfc6b8,
        SyntaxKind::Comment => 0xa0938e,
    })
}

fn rgb(index: u8) -> Rgb {
    ZUGHY_32[usize::from(index)]
}

/// WCAG relative luminance.
fn luminance(color: Rgb) -> f64 {
    let linear = |channel: u8| {
        let c = f64::from(channel) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn every_kind_is_a_system_color_matching_the_mockup() {
    for kind in SyntaxKind::ALL {
        assert!(color(kind) < 32, "{kind:?}");
        assert_eq!(rgb(color(kind)), mockup(kind), "{kind:?}");
    }
}

#[test]
fn kinds_have_distinct_colors() {
    for (i, a) in SyntaxKind::ALL.iter().enumerate() {
        for b in &SyntaxKind::ALL[i + 1..] {
            assert_ne!(color(*a), color(*b), "{a:?} and {b:?}");
        }
    }
}

#[test]
fn colors_stay_apart_from_plain_text() {
    for kind in SyntaxKind::ALL {
        assert_ne!(color(kind), TEXT, "{kind:?}");
    }
}

/// Every category must stay legible on the editor background. Comments are
/// meant to recede, so they only need to be readable, not to match body text.
#[test]
fn colors_are_readable_on_the_background() {
    let background = rgb(BACKGROUND);
    for kind in SyntaxKind::ALL {
        let ratio = contrast(rgb(color(kind)), background);
        let floor = if kind == SyntaxKind::Comment {
            3.0
        } else {
            4.5
        };
        assert!(ratio >= floor, "{kind:?} has contrast {ratio:.2}");
    }
}
