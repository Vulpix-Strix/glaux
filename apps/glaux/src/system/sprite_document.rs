//! The loaded cartridge's video state as the editors change it.

use glaux_machine::sprite::{OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH, ObjTileBank, ObjVram};

use super::video::{
    CartridgeVideo, CartridgeVideoDefaults, VideoAssetError, VideoFiles, VideoLoadError,
};
use crate::cartridge::LoadedCartridge;

/// What `sprites.bin` held when a document was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileOrigin {
    bank: ObjTileBank,
    /// Length of `sprites.bin`, or `None` when the cartridge had none.
    file_len: Option<usize>,
}

impl TileOrigin {
    /// The exact bytes `sprites.bin` held, or `None` when it was absent.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "nothing writes the tiles back to compare against yet"
        )
    )]
    pub fn file_bytes(&self) -> Option<&[u8]> {
        self.file_len.map(|len| &self.bank.as_bytes()[..len])
    }
}

/// A cartridge's video state as the editors change it, held in memory only.
///
/// Object tiles are editable; their attributes, the object palette banks and
/// the master palette are read-only. The document remembers the tiles it was
/// read with, so it knows exactly whether it differs from them: restoring
/// every changed pixel by hand leaves it unmodified again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteDocument {
    video: CartridgeVideo,
    origin: TileOrigin,
    /// Pixels whose local index differs from `origin`'s.
    changed_pixels: usize,
}

impl SpriteDocument {
    /// Reads the cartridge's video assets, falling back to `defaults` for
    /// those it lacks.
    pub fn load(
        cartridge: &LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
    ) -> Result<Self, VideoLoadError> {
        let files = VideoFiles::read(cartridge).map_err(VideoLoadError::Cartridge)?;
        Ok(Self::from_files(&files, defaults)?)
    }

    /// A document on `files` as read from a cartridge.
    pub fn from_files(
        files: &VideoFiles,
        defaults: &CartridgeVideoDefaults,
    ) -> Result<Self, VideoAssetError> {
        let video = CartridgeVideo::decode(files, defaults)?;
        let origin = TileOrigin {
            bank: video.obj.tiles().clone(),
            file_len: files.obj_tiles.as_ref().map(Vec::len),
        };
        Ok(Self {
            video,
            origin,
            changed_pixels: 0,
        })
    }

    pub fn video(&self) -> &CartridgeVideo {
        &self.video
    }

    pub fn obj(&self) -> &ObjVram {
        &self.video.obj
    }

    pub fn tiles(&self) -> &ObjTileBank {
        self.video.obj.tiles()
    }

    /// What the tiles were read from, to tell whether the file changed on
    /// disk since.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "nothing writes the tiles back to compare against yet"
        )
    )]
    pub fn origin(&self) -> &TileOrigin {
        &self.origin
    }

    /// Whether any pixel differs from the tiles the document was read with.
    pub fn is_modified(&self) -> bool {
        self.changed_pixels > 0
    }

    /// One tile-local palette index; `None` for an invalid tile or pixel.
    pub fn pixel(&self, tile: usize, x: usize, y: usize) -> Option<u8> {
        self.tiles().pixel(tile, x, y)
    }

    /// Writes local index `index` to one pixel. Returns whether the pixel
    /// changed; invalid positions and indices above 15 change nothing.
    pub fn set_pixel(&mut self, tile: usize, x: usize, y: usize, index: u8) -> bool {
        let Some(previous) = self.video.obj.tiles_mut().set_pixel(tile, x, y, index) else {
            return false;
        };
        if previous == index {
            return false;
        }
        let original = self.origin.bank.pixel(tile, x, y);
        if original == Some(previous) {
            self.changed_pixels += 1;
        } else if original == Some(index) {
            self.changed_pixels -= 1;
        }
        true
    }

    /// Replaces the pixels of `tile` connected to `(x, y)` through edges,
    /// that share its local index, with `index`. The fill never leaves the
    /// tile. Returns whether any pixel changed.
    pub fn fill(&mut self, tile: usize, x: usize, y: usize, index: u8) -> bool {
        let Some(target) = self.pixel(tile, x, y) else {
            return false;
        };
        if target == index || index > 0x0f {
            return false;
        }
        let mut pending = vec![(x, y)];
        while let Some((x, y)) = pending.pop() {
            if self.pixel(tile, x, y) != Some(target) {
                continue;
            }
            self.set_pixel(tile, x, y, index);
            if x > 0 {
                pending.push((x - 1, y));
            }
            if x + 1 < OBJ_TILE_WIDTH {
                pending.push((x + 1, y));
            }
            if y > 0 {
                pending.push((x, y - 1));
            }
            if y + 1 < OBJ_TILE_HEIGHT {
                pending.push((x, y + 1));
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_TILE_BYTES};
    use pretty_assertions::assert_eq;

    fn defaults() -> CartridgeVideoDefaults {
        CartridgeVideoDefaults::from_rom(&crate::rom::load().unwrap())
    }

    /// A document whose `sprites.bin` holds `tiles`.
    fn document(tiles: Option<Vec<u8>>) -> SpriteDocument {
        let files = VideoFiles {
            obj_tiles: tiles,
            ..VideoFiles::default()
        };
        SpriteDocument::from_files(&files, &defaults()).unwrap()
    }

    /// Tile 0 of `rows`, one string of 8 hex digits per pixel row.
    fn tile(rows: [&str; 8]) -> Vec<u8> {
        let pixels: Vec<u8> = rows
            .iter()
            .flat_map(|row| row.chars().map(|c| c.to_digit(16).unwrap() as u8))
            .collect();
        pixels.chunks(2).map(|p| (p[0] << 4) | p[1]).collect()
    }

    fn rows(document: &SpriteDocument, tile: usize) -> Vec<String> {
        (0..8)
            .map(|y| {
                (0..8)
                    .map(|x| format!("{:X}", document.pixel(tile, x, y).unwrap()))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn writes_change_the_native_bank_and_mark_the_document_modified() {
        let mut doc = document(None);
        assert!(!doc.is_modified());
        assert!(doc.set_pixel(3, 1, 2, 9));
        assert_eq!(doc.pixel(3, 1, 2), Some(9));
        assert_eq!(doc.tiles().as_bytes()[3 * OBJ_TILE_BYTES + 2 * 4], 0x09);
        assert!(doc.is_modified());
    }

    #[test]
    fn writing_the_value_already_there_changes_nothing() {
        let mut doc = document(None);
        assert!(!doc.set_pixel(0, 0, 0, 0));
        assert!(!doc.is_modified());
        assert!(doc.set_pixel(0, 0, 0, 5));
        assert!(!doc.set_pixel(0, 0, 0, 5));
        assert!(doc.set_pixel(0, 0, 0, 0));
        assert!(!doc.is_modified(), "one write counted once, then restored");
    }

    #[test]
    fn restoring_every_edited_pixel_makes_the_document_clean_again() {
        let mut bytes = vec![0x11; 2 * OBJ_TILE_BYTES];
        bytes[OBJ_TILE_BYTES] = 0x23;
        let mut doc = document(Some(bytes));
        doc.set_pixel(0, 4, 4, 7);
        doc.set_pixel(0, 4, 4, 8);
        doc.set_pixel(1, 0, 0, 0);
        doc.set_pixel(1, 1, 0, 0);
        doc.set_pixel(900, 7, 7, 15);
        assert!(doc.is_modified());

        doc.set_pixel(0, 4, 4, 1);
        doc.set_pixel(1, 0, 0, 2);
        doc.set_pixel(900, 7, 7, 0);
        assert!(doc.is_modified(), "one pixel is still different");
        doc.set_pixel(1, 1, 0, 3);
        assert!(!doc.is_modified());
    }

    #[test]
    fn the_eraser_index_reads_back_as_transparent() {
        let mut doc = document(Some(vec![0xff; OBJ_TILE_BYTES]));
        assert!(doc.set_pixel(0, 6, 1, 0));
        assert_eq!(doc.pixel(0, 6, 1), Some(0));
        let resolved = doc
            .obj()
            .palettes()
            .resolve(doc.obj().attributes().bank(0), 0);
        assert_eq!(resolved, None, "local 0 is transparent in every bank");
    }

    #[test]
    fn invalid_writes_change_nothing() {
        let mut doc = document(None);
        assert!(!doc.set_pixel(0, 8, 0, 1));
        assert!(!doc.set_pixel(0, 0, 0, 16));
        assert!(!doc.set_pixel(1024, 0, 0, 1));
        assert!(!doc.is_modified());
    }

    #[test]
    fn the_origin_keeps_the_bytes_read_through_edits() {
        let bytes = vec![0x12; OBJ_TILE_BYTES * 3];
        let mut doc = document(Some(bytes.clone()));
        doc.set_pixel(0, 0, 0, 9);
        doc.set_pixel(5, 0, 0, 9);
        assert_eq!(doc.origin().file_bytes(), Some(&bytes[..]));

        assert_eq!(document(None).origin().file_bytes(), None);
        assert_eq!(
            document(Some(Vec::new())).origin().file_bytes(),
            Some(&[][..])
        );
    }

    #[test]
    fn fill_replaces_the_region_joined_by_edges() {
        let mut doc = document(Some(tile([
            "11100000", //
            "10100000", //
            "11102000", //
            "00002000", //
            "00000000", //
            "22222222", //
            "00000000", //
            "00000001", //
        ])));
        assert!(doc.fill(0, 7, 0, 3));
        assert_eq!(
            rows(&doc, 0),
            [
                "11133333", //
                "10133333", //
                "11132333", //
                "33332333", //
                "33333333", //
                "22222222", //
                "00000000", //
                "00000001", //
            ]
        );
        // The enclosed pixel touches the fill only diagonally.
        assert!(doc.fill(0, 1, 1, 4));
        assert_eq!(doc.pixel(0, 1, 1), Some(4));
        assert_eq!(doc.pixel(0, 0, 0), Some(1));
    }

    #[test]
    fn fill_stays_inside_the_tile() {
        let mut doc = document(None);
        let tile = OBJ_GRID_COLUMNS + 1;
        assert!(doc.fill(tile, 7, 7, 6));
        assert!(rows(&doc, tile).iter().all(|row| row == "66666666"));
        for neighbour in [
            tile - 1,
            tile + 1,
            tile - OBJ_GRID_COLUMNS,
            tile + OBJ_GRID_COLUMNS,
        ] {
            assert!(
                rows(&doc, neighbour).iter().all(|row| row == "00000000"),
                "tile {neighbour}"
            );
        }
    }

    #[test]
    fn filling_with_the_same_color_is_a_no_op() {
        let mut doc = document(Some(vec![0x55; OBJ_TILE_BYTES]));
        assert!(!doc.fill(0, 3, 3, 5));
        assert!(!doc.is_modified());
        assert!(!doc.fill(0, 8, 0, 1));
        assert!(!doc.fill(0, 0, 0, 16));
        assert!(!doc.is_modified());
    }

    #[test]
    fn filling_back_to_the_original_color_is_clean() {
        let mut doc = document(Some(vec![0x55; OBJ_TILE_BYTES]));
        doc.fill(0, 0, 0, 2);
        assert!(doc.is_modified());
        doc.fill(0, 0, 0, 5);
        assert!(!doc.is_modified());
    }
}
