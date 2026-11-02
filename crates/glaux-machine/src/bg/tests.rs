use super::*;
use pretty_assertions::assert_eq;

fn bank(n: u8) -> BgBank {
    BgBank::new(n).unwrap()
}

#[test]
fn tile_data_is_whole_tiles_within_the_bank() {
    assert!(BgTileBank::from_bytes(&[]).is_ok());
    assert!(BgTileBank::from_bytes(&[0; BG_TILE_BANK_BYTES]).is_ok());
    assert_eq!(
        BgTileBank::from_bytes(&[0; 31]),
        Err(BgTileBankError::Misaligned { length: 31 })
    );
    assert_eq!(
        BgTileBank::from_bytes(&[0; BG_TILE_BANK_BYTES + BG_TILE_BYTES]),
        Err(BgTileBankError::TooLarge {
            length: BG_TILE_BANK_BYTES + BG_TILE_BYTES
        })
    );
}

#[test]
fn tiles_use_the_object_nibble_order() {
    let mut bytes = [0; BG_TILE_BYTES];
    bytes[0] = 0xA5;
    let bank = BgTileBank::from_bytes(&bytes).unwrap();
    assert_eq!(bank.pixel(0, 0, 0), Some(0xA));
    assert_eq!(bank.pixel(0, 1, 0), Some(0x5));
    assert_eq!(bank.pixel(1024, 0, 0), None);
}

#[test]
fn setting_a_pixel_returns_the_previous_index() {
    let mut bank = BgTileBank::new();
    assert_eq!(bank.set_pixel(3, 7, 7, 9), Some(0));
    assert_eq!(bank.set_pixel(3, 7, 7, 1), Some(9));
    assert_eq!(bank.set_pixel(3, 7, 7, 16), None);
    assert_eq!(bank.as_bytes()[3 * BG_TILE_BYTES + 7 * 4 + 3], 0x01);
}

#[test]
fn palette_data_is_exactly_four_banks() {
    assert_eq!(BG_PALETTES_BYTES, 64);
    for length in [0, 63, 65, 128] {
        assert_eq!(
            BgPalettes::from_bytes(&vec![0; length]),
            Err(BgPalettesError::Length { length })
        );
    }
    assert!(BgPalettes::from_bytes(&[0; 64]).is_ok());
}

#[test]
fn entry_zero_of_each_bank_is_reserved() {
    for b in 0..4 {
        let mut bytes = [0; 64];
        bytes[b * 16] = 1;
        assert_eq!(
            BgPalettes::from_bytes(&bytes),
            Err(BgPalettesError::ReservedEntry { bank: b, value: 1 })
        );
    }
}

#[test]
fn the_linear_default_maps_bank_n_to_16n_plus_local() {
    let palettes = BgPalettes::linear();
    assert_eq!(palettes.resolve(bank(0), 5), Some(5));
    assert_eq!(palettes.resolve(bank(3), 15), Some(63));
    assert_eq!(palettes.resolve(bank(2), 0), None);
    assert_eq!(palettes.resolve(bank(2), 16), None);
    assert_eq!(BgPalettes::from_bytes(palettes.as_bytes()), Ok(palettes));
}

#[test]
fn setting_a_color_never_touches_the_transparent_entry() {
    let mut palettes = BgPalettes::linear();
    assert_eq!(palettes.set(bank(1), 2, 200), Some(18));
    assert_eq!(palettes.resolve(bank(1), 2), Some(200));
    assert_eq!(palettes.set(bank(1), 0, 200), None);
    assert_eq!(palettes.set(bank(1), 16, 200), None);
    assert_eq!(palettes.as_bytes()[16], 0);
}
