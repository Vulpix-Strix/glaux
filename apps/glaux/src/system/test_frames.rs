//! Framebuffer equality for tests, with a bounded failure report.
//!
//! `assert_eq!` on two framebuffers formats all 76 800 bytes of each side,
//! and `pretty_assertions` then diffs those listings; with many tests
//! failing at once that can exhaust memory. These helpers compare the bytes
//! directly and report only a count and the first few differences.

use std::fmt::Write;

use glaux_machine::framebuffer::{Framebuffer, LEN, WIDTH};

/// Differences reported by [`differences`] before it only counts.
const SHOWN: usize = 5;

/// `None` when both frames hold the same pixels; otherwise how many pixels
/// differ and the first few as `(x, y): actual != expected`.
pub(crate) fn differences(actual: &Framebuffer, expected: &Framebuffer) -> Option<String> {
    let mut count = 0;
    let mut first = String::new();
    for (i, (a, e)) in actual
        .as_bytes()
        .iter()
        .zip(expected.as_bytes())
        .enumerate()
    {
        if a != e {
            if count < SHOWN {
                let _ = write!(first, " ({}, {}): {a} != {e};", i % WIDTH, i / WIDTH);
            }
            count += 1;
        }
    }
    (count > 0)
        .then(|| format!("{count} of {LEN} pixels differ (actual != expected), first:{first}"))
}

/// Asserts that two framebuffers hold the same pixels, reporting at most a
/// few differing pixels on failure. An optional trailing format string and
/// arguments describe the case.
macro_rules! assert_frame_eq {
    ($actual:expr, $expected:expr $(,)?) => {
        if let Some(diff) = $crate::system::test_frames::differences(&$actual, &$expected) {
            panic!("frames differ: {diff}");
        }
    };
    ($actual:expr, $expected:expr, $($context:tt)+) => {
        if let Some(diff) = $crate::system::test_frames::differences(&$actual, &$expected) {
            panic!("frames differ ({}): {diff}", format_args!($($context)+));
        }
    };
}
pub(crate) use assert_frame_eq;
