//! The Cart ROM indicator the editors' status bars share.

use glaux_i18n::Arg;
use glaux_machine::font::BitmapFont;

use crate::cartridge::rom::{CART_ROM_BUDGET, RomUsage};
use crate::system::screen::{ERROR, PALE, WARN};
use crate::ui_text;

/// Share of the budget from which the indicator warns, in percent.
const WARN_PERCENT: u64 = 90;

/// The indicator's text and color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RomTag {
    pub text: String,
    pub color: u8,
}

/// KiB as a decimal with one digit, rounded up so a value never understates
/// the bytes it stands for: only a full budget reads `1024.0`.
fn kib(bytes: u64) -> String {
    let tenths = bytes.saturating_mul(10).div_ceil(1024);
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// Whole KiB, rounded up.
fn kib_whole(bytes: u64) -> String {
    bytes.div_ceil(1024).to_string()
}

/// The longest form of the indicator for `usage` that fits `free` cells of
/// `font`, or `None` when none does or nothing has been measured yet.
///
/// Within the budget it reads `ROM 123.4 KiB / 1 MiB`, over it
/// `ROM 1120.0 KiB / 1 MiB +96.0 KiB`, and while some paths could not be
/// settled `ROM >=123.4 KiB / 1 MiB !`, which is told apart from a complete
/// total by the `>=` and the `!`. Shorter forms drop words, then digits.
pub(super) fn rom_tag(usage: Option<&RomUsage>, font: &BitmapFont, free: usize) -> Option<RomTag> {
    let usage = usage?;
    let text = |id: &str, args: &[(&str, Arg<'_>)]| ui_text::get_with(id, args);
    let used = kib(usage.used);
    let used_whole = kib_whole(usage.used);
    let excess = kib(usage.excess());
    let excess_whole = kib_whole(usage.excess());
    let candidates = if usage.incomplete() {
        [
            text("workspace-rom-partial", &[("used", Arg::Text(&used))]),
            text(
                "workspace-rom-partial-tiny",
                &[("used", Arg::Text(&used_whole))],
            ),
        ]
        .to_vec()
    } else if usage.excess() > 0 {
        vec![
            text(
                "workspace-rom-over",
                &[("used", Arg::Text(&used)), ("excess", Arg::Text(&excess))],
            ),
            text(
                "workspace-rom-over-compact",
                &[("excess", Arg::Text(&excess))],
            ),
            text(
                "workspace-rom-over-tiny",
                &[("excess", Arg::Text(&excess_whole))],
            ),
        ]
    } else {
        vec![
            text("workspace-rom", &[("used", Arg::Text(&used))]),
            text("workspace-rom-compact", &[("used", Arg::Text(&used))]),
            text("workspace-rom-tiny", &[("used", Arg::Text(&used_whole))]),
        ]
    };
    let color = if usage.excess() > 0 {
        ERROR
    } else if usage.incomplete() || usage.used.saturating_mul(100) >= CART_ROM_BUDGET * WARN_PERCENT
    {
        WARN
    } else {
        PALE
    };
    candidates
        .into_iter()
        .find(|text| font.text_cells(text) <= free)
        .map(|text| RomTag { text, color })
}

#[cfg(test)]
mod tests;
