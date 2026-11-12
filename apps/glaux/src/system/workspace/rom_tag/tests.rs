use super::*;
use crate::cartridge::rom::RomIssue;
use crate::system::test_font::LAYOUT_FONT;
use pretty_assertions::assert_eq;

const KIB: u64 = 1024;

fn usage(used: u64) -> RomUsage {
    RomUsage {
        used,
        ..RomUsage::default()
    }
}

fn incomplete(used: u64) -> RomUsage {
    RomUsage {
        used,
        issues: vec![RomIssue::FileLimit(4096)],
        ..RomUsage::default()
    }
}

fn tag(usage: &RomUsage) -> (String, u8) {
    let tag = rom_tag(Some(usage), &LAYOUT_FONT, usize::MAX).unwrap();
    (tag.text, tag.color)
}

#[test]
fn nothing_is_shown_before_the_first_measurement() {
    assert_eq!(rom_tag(None, &LAYOUT_FONT, usize::MAX), None);
}

#[test]
fn a_cartridge_within_the_budget_shows_used_and_total_in_kib() {
    assert_eq!(
        tag(&usage(128 * KIB)),
        ("ROM 128.0 KiB / 1 MiB".to_owned(), PALE)
    );
    assert_eq!(tag(&usage(0)).0, "ROM 0.0 KiB / 1 MiB");
}

#[test]
fn values_round_up_so_they_never_understate() {
    assert_eq!(tag(&usage(1)).0, "ROM 0.1 KiB / 1 MiB");
    assert_eq!(tag(&usage(KIB + 1)).0, "ROM 1.1 KiB / 1 MiB");
    assert_eq!(tag(&usage(KIB)).0, "ROM 1.0 KiB / 1 MiB");
}

#[test]
fn it_warns_from_ninety_percent_of_the_budget() {
    // 90 % of 1 MiB is 943 718.4 bytes.
    assert_eq!(tag(&usage(943_718)).1, PALE);
    assert_eq!(tag(&usage(943_719)).1, WARN);
}

#[test]
fn a_full_budget_is_a_warning_and_one_byte_more_is_an_error() {
    assert_eq!(
        tag(&usage(CART_ROM_BUDGET)),
        ("ROM 1024.0 KiB / 1 MiB".to_owned(), WARN)
    );
    assert_eq!(
        tag(&usage(CART_ROM_BUDGET + 1)),
        ("ROM 1024.1 KiB / 1 MiB +0.1 KiB".to_owned(), ERROR)
    );
    assert_eq!(
        tag(&usage(CART_ROM_BUDGET + 96 * KIB)),
        ("ROM 1120.0 KiB / 1 MiB +96.0 KiB".to_owned(), ERROR)
    );
}

#[test]
fn an_incomplete_total_cannot_be_mistaken_for_a_complete_one() {
    assert_eq!(
        tag(&incomplete(128 * KIB)),
        ("ROM >=128.0 KiB / 1 MiB !".to_owned(), WARN)
    );
    // Already over the budget with what was found.
    assert_eq!(
        tag(&incomplete(CART_ROM_BUDGET + KIB)),
        ("ROM >=1025.0 KiB / 1 MiB !".to_owned(), ERROR)
    );
}

#[test]
fn narrow_bars_get_shorter_forms_and_then_none() {
    let cells = |text: &str| LAYOUT_FONT.text_cells(text);
    let fits = |usage: &RomUsage, free: usize| rom_tag(Some(usage), &LAYOUT_FONT, free);

    let within = usage(123 * KIB + 409);
    for (text, next) in [
        ("ROM 123.4 KiB / 1 MiB", "ROM 123.4/1024 KiB"),
        ("ROM 123.4/1024 KiB", "ROM 124K"),
    ] {
        assert_eq!(fits(&within, cells(text)).unwrap().text, text);
        assert_eq!(fits(&within, cells(text) - 1).unwrap().text, next);
    }
    assert_eq!(fits(&within, cells("ROM 124K") - 1), None);

    let over = usage(CART_ROM_BUDGET + 96 * KIB);
    assert_eq!(
        fits(&over, cells("ROM 1120.0 KiB / 1 MiB +96.0 KiB") - 1)
            .unwrap()
            .text,
        "ROM +96.0 KiB"
    );
    assert_eq!(
        fits(&over, cells("ROM +96.0 KiB") - 1).unwrap().text,
        "ROM +96K"
    );

    let partial = incomplete(128 * KIB);
    assert_eq!(
        fits(&partial, cells("ROM >=128.0 KiB / 1 MiB !") - 1)
            .unwrap()
            .text,
        "ROM >=128K !"
    );
    // Even the shortest form of an incomplete total keeps its marks.
    assert_eq!(fits(&partial, cells("ROM >=128K !") - 1), None);
}
