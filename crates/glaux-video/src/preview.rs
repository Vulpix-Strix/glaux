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

    /// Frame index for every master index. Masters no bank refers to map
    /// to 0. Every master a tile pixel can resolve to is referenced.
    pub fn slots(&self) -> &[u8; PALETTE_LEN] {
        &self.table
    }
}

#[cfg(test)]
mod tests;
