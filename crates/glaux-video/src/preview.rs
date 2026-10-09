//! A frame palette that shows system UI colors and a cartridge's object
//! colors in the same indexed frame.
//!
//! A frame is presented with a single 256-entry palette. Tools that draw
//! their own interface next to cartridge graphics keep their UI colors in
//! the low entries and place every master color the object palette banks
//! can resolve to in the entries after them.

use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, OBJ_PALETTE_COLORS, ObjBank, ObjPalettes};
use glaux_machine::palette::{PALETTE_LEN, Palette};

/// More master colors are referenced than fit after the reserved entries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the object palettes reference {needed} master colors, but only {available} \
     palette entries are free after the {reserved} reserved ones"
)]
pub struct ObjPreviewPaletteError {
    pub reserved: usize,
    pub needed: usize,
    pub available: usize,
}

/// A presentation palette for drawing object tiles beside system UI.
///
/// Entries `0..reserved` are copied from the system palette, so UI drawn
/// with those indices looks as it does elsewhere. Each master index that
/// some object palette bank maps a local index to gets one entry after
/// them, holding exactly that master color. Entries are allocated in
/// bank order, then local order, on a master index's first appearance, so
/// the same palettes always produce the same layout. Masters are never
/// merged by color: two master indices with equal RGB keep separate entries.
///
/// The palette is derived data for one frame's presentation. It owns its
/// copies and never changes the palettes it was built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjPreviewPalette {
    palette: Palette,
    /// Frame index of each master index, `None` where no bank refers to it.
    slots: [Option<u8>; PALETTE_LEN],
    /// [`slots`](Self::slots) with unreferenced masters mapped to 0, the
    /// form [`draw_obj_tile_scaled`](crate::draw::draw_obj_tile_scaled)
    /// takes.
    table: [u8; PALETTE_LEN],
}

impl ObjPreviewPalette {
    /// Builds the palette for `obj` resolved through `master`, keeping
    /// `reserved` leading entries of `system`.
    pub fn new(
        system: &Palette,
        reserved: usize,
        master: &Palette,
        obj: &ObjPalettes,
    ) -> Result<Self, ObjPreviewPaletteError> {
        let reserved = reserved.min(PALETTE_LEN);
        let mut entries = *system.entries();
        let mut slots = [None; PALETTE_LEN];
        let mut next = reserved;
        let mut needed = 0;

        for bank in (0..OBJ_PALETTE_BANKS as u8).filter_map(ObjBank::new) {
            for local in 1..OBJ_PALETTE_COLORS as u8 {
                let Some(index) = obj.resolve(bank, local) else {
                    continue;
                };
                let slot = &mut slots[usize::from(index)];
                if slot.is_some() {
                    continue;
                }
                needed += 1;
                if next < PALETTE_LEN {
                    entries[next] = master.get(index);
                    // `next` is below PALETTE_LEN, so it fits in u8.
                    *slot = Some(next as u8);
                    next += 1;
                }
            }
        }

        let available = PALETTE_LEN - reserved;
        if needed > available {
            return Err(ObjPreviewPaletteError {
                reserved,
                needed,
                available,
            });
        }
        Ok(Self {
            palette: Palette::new(entries),
            slots,
            table: slots.map(|slot| slot.unwrap_or(0)),
        })
    }

    /// The palette to present the frame with.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// The frame index holding master color `master`, if any bank refers
    /// to it.
    pub fn slot(&self, master: u8) -> Option<u8> {
        self.slots[usize::from(master)]
    }

    /// Frame index for every master index; masters no bank refers to map
    /// to 0. Every master a tile pixel can resolve to is referenced.
    pub fn slots(&self) -> &[u8; PALETTE_LEN] {
        &self.table
    }
}

#[cfg(test)]
mod tests {
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
    fn the_same_palettes_always_give_the_same_layout() {
        let obj = obp(|bank, local| (bank * 31 + local * 7) as u8);
        assert_eq!(build(&obj), build(&obj));
    }

    #[test]
    fn building_leaves_its_inputs_unchanged() {
        let (system, master, obj) = (system(), master(), ObjPalettes::linear());
        let before = (system.clone(), master.clone(), obj.clone());
        ObjPreviewPalette::new(&system, RESERVED, &master, &obj).unwrap();
        assert_eq!((system, master, obj), before);
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
}
