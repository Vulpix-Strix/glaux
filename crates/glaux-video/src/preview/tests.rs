use super::*;
use glaux_machine::obj_palette::OBJ_PALETTES_BYTES;
use glaux_machine::palette::Rgb;
use pretty_assertions::assert_eq;

const RESERVED: usize = 32;

fn system() -> Palette {
    Palette::new(std::array::from_fn(|i| Rgb::from_hex(0x0a0000 + i as u32)))
}

/// A master palette whose entry `i` has `i` in green and blue, so every
/// entry is distinct.
fn master() -> Palette {
    Palette::new(std::array::from_fn(|i| {
        Rgb::from_hex(((i as u32) << 8) | i as u32)
    }))
}

fn obp(bytes: impl Fn(usize, usize) -> u8) -> ObjPalettes {
    let mut out = [0; OBJ_PALETTES_BYTES];
    for (offset, byte) in out.iter_mut().enumerate() {
        if offset % 16 != 0 {
            *byte = bytes(offset / 16, offset % 16);
        }
    }
    ObjPalettes::from_bytes(&out).unwrap()
}

fn build(obj: &ObjPalettes) -> ObjPreviewPalette {
    ObjPreviewPalette::new(&system(), RESERVED, &master(), obj).unwrap()
}

#[test]
fn reserved_entries_keep_the_system_colors() {
    let preview = build(&ObjPalettes::linear());
    assert_eq!(
        preview.palette().entries()[..RESERVED],
        system().entries()[..RESERVED]
    );
}

#[test]
fn every_referenced_master_shows_its_exact_color() {
    let obj = ObjPalettes::linear();
    let preview = build(&obj);
    for bank in (0..8).filter_map(ObjBank::new) {
        for local in 1..16 {
            let index = obj.resolve(bank, local).unwrap();
            let slot = preview.slot(index).expect("referenced master has a slot");
            assert!(usize::from(slot) >= RESERVED);
            assert_eq!(preview.palette().get(slot), master().get(index));
            assert_eq!(preview.slots()[usize::from(index)], slot);
        }
    }
    // Linear banks never refer to master 0, 16, 32, ...
    assert_eq!(preview.slot(0), None);
    assert_eq!(preview.slot(16), None);
}

#[test]
fn slots_follow_bank_then_local_order_and_ignore_repeats() {
    // Bank 0 maps local i to 200 - i; every other bank repeats bank 0.
    let preview = build(&obp(|_, local| 200 - local as u8));
    for local in 1..16u8 {
        assert_eq!(preview.slot(200 - local), Some(RESERVED as u8 + local - 1));
    }
    assert_eq!(preview.slot(200), None);
    // Nothing past the 15 distinct colors is allocated.
    assert_eq!(
        preview.palette().entries()[RESERVED + 15..],
        system().entries()[RESERVED + 15..]
    );
}

#[test]
fn equal_colors_at_different_masters_keep_separate_slots() {
    let mut entries = *master().entries();
    entries[50] = Rgb::from_hex(0x123456);
    entries[60] = Rgb::from_hex(0x123456);
    let master = Palette::new(entries);
    let obj = obp(|_, local| match local {
        1 => 50,
        _ => 60,
    });
    let preview = ObjPreviewPalette::new(&system(), RESERVED, &master, &obj).unwrap();
    let (a, b) = (preview.slot(50).unwrap(), preview.slot(60).unwrap());
    assert_ne!(a, b);
    assert_eq!(preview.palette().get(a), Rgb::from_hex(0x123456));
    assert_eq!(preview.palette().get(b), Rgb::from_hex(0x123456));
}

#[test]
fn eight_full_distinct_banks_fit_beside_the_system_colors() {
    // 8 banks * 15 locals, all distinct: the most banks can refer to.
    let obj = obp(|bank, local| (bank * 15 + local) as u8);
    let preview = build(&obj);
    let slots: std::collections::BTreeSet<u8> =
        (1..=120).map(|m| preview.slot(m).unwrap()).collect();
    assert_eq!(slots.len(), 120);
    assert_eq!(slots.first(), Some(&(RESERVED as u8)));
    assert_eq!(slots.last(), Some(&(RESERVED as u8 + 119)));
}

#[test]
fn too_many_colors_for_the_free_entries_is_an_error() {
    let obj = obp(|bank, local| (bank * 15 + local) as u8);
    assert_eq!(
        ObjPreviewPalette::new(&system(), 200, &master(), &obj),
        Err(ObjPreviewPaletteError {
            reserved: 200,
            needed: 120,
            available: 56
        })
    );
}
